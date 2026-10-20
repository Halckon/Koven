//! 单文件宿主的输入、source-root、链接与共同 Unit driver 适配。

use std::{
    ffi::OsStr,
    fmt, fs, io,
    path::{Component, Path, PathBuf},
};

#[cfg(test)]
use std::process::Command;

use lang_frontend::{
    analysis::{UnitSourceDescriptor, analyze_parsed_unit_names},
    diagnostic::Diagnostic,
    lexer::lex,
    name_resolution::package_path_mismatch_diagnostic,
    parser::{ParsedFile, parse_file},
    source::SourceMap,
    type_checking::standard_environments,
};

use crate::{
    diagnostic_renderer::render_diagnostics,
    linker::{LinkerError, link_native_object},
    machine_diagnostic_renderer::render_machine_diagnostics,
    unit_build::{UnitBuildError, UnitEntrySelection, emit_unit_object},
};

pub(crate) use crate::unit_build::SingleEntrySelection as BootstrapEntry;

/// 一次单文件 bootstrap 的调用方拥有配置。
#[derive(Clone, Copy, Debug)]
pub(crate) struct BootstrapTarget<'a> {
    pub(crate) source: &'a Path,
    pub(crate) source_root: Option<&'a Path>,
    pub(crate) entry: BootstrapEntry<'a>,
    pub(crate) object: &'a Path,
    pub(crate) executable: &'a Path,
}

/// 单文件宿主失败；语言阶段只包装唯一 Unit 错误。
#[derive(Debug)]
pub(crate) enum BootstrapError {
    InvalidPaths,
    OutputExists(PathBuf),
    ReadSource {
        path: PathBuf,
        kind: io::ErrorKind,
    },
    NonUtf8SourcePath(PathBuf),
    InspectSourceRoot {
        path: PathBuf,
        kind: io::ErrorKind,
    },
    Unit(UnitBuildError),
    Linker(LinkerError),
    #[cfg(test)]
    LaunchExecutable {
        path: PathBuf,
        kind: io::ErrorKind,
    },
    #[cfg(test)]
    ProcessFailure {
        status: Option<i32>,
    },
}

impl fmt::Display for BootstrapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPaths => {
                formatter.write_str("source, object and executable paths overlap")
            }
            Self::OutputExists(path) => {
                write!(formatter, "output already exists: {}", path.display())
            }
            Self::ReadSource { path, kind } => {
                write!(formatter, "cannot read {}: {kind}", path.display())
            }
            Self::NonUtf8SourcePath(path) => write!(
                formatter,
                "source path is not valid UTF-8: {}",
                path.display()
            ),
            Self::InspectSourceRoot { path, kind } => write!(
                formatter,
                "cannot inspect single-file source root {}: {kind}",
                path.display()
            ),
            Self::Unit(error) => error.fmt(formatter),
            Self::Linker(error) => write!(formatter, "linker failed: {error:?}"),
            #[cfg(test)]
            Self::LaunchExecutable { path, kind } => {
                write!(formatter, "cannot launch {}: {kind}", path.display())
            }
            #[cfg(test)]
            Self::ProcessFailure { status } => {
                write!(formatter, "process failed with status {status:?}")
            }
        }
    }
}

impl std::error::Error for BootstrapError {}

impl From<UnitBuildError> for BootstrapError {
    fn from(error: UnitBuildError) -> Self {
        Self::Unit(error)
    }
}

/// 从一份显式用户 source 和有限受信标准源构建并链接一个 native target。
pub(crate) fn bootstrap_build(target: BootstrapTarget<'_>) -> Result<(), BootstrapError> {
    validate_paths(target)?;
    let text = fs::read_to_string(target.source).map_err(|error| BootstrapError::ReadSource {
        path: target.source.to_path_buf(),
        kind: error.kind(),
    })?;
    let source_name = target
        .source
        .to_str()
        .ok_or_else(|| BootstrapError::NonUtf8SourcePath(target.source.to_path_buf()))?;
    let mut sources = SourceMap::new();
    let source = sources
        .add_source(source_name, text)
        .map_err(UnitBuildError::Source)?;
    let lexed = lex(&sources, source).map_err(UnitBuildError::Lexer)?;
    let parsed = parse_file(&sources, &lexed).map_err(UnitBuildError::Parser)?;
    if !parsed.initial_package_header_is_complete() {
        return Err(source_diagnostics(&sources, parsed.diagnostics()));
    }
    let logical_path = single_logical_path(&sources, &parsed, target.source, target.source_root)?;
    let mut descriptors = vec![UnitSourceDescriptor::new("single", logical_path, source)];
    let (name_environment, mut type_environment) = standard_environments();
    crate::standard_sources::append_standard_sources(
        &mut sources,
        &mut descriptors,
        &mut type_environment,
    )?;
    // Loader 只追加标准 descriptor；用户原解析产物移动一次，标准源各解析一次。
    let mut descriptors = descriptors.into_iter();
    let user = descriptors
        .next()
        .ok_or(UnitBuildError::MissingSingleSource)?;
    let mut parsed_sources = vec![(user, parsed)];
    for descriptor in descriptors {
        let lexed = lex(&sources, descriptor.source_id()).map_err(UnitBuildError::Lexer)?;
        let parsed = parse_file(&sources, &lexed).map_err(UnitBuildError::Parser)?;
        parsed_sources.push((descriptor, parsed));
    }
    let snapshot = analyze_parsed_unit_names(sources, parsed_sources, name_environment)
        .map_err(UnitBuildError::from)?;
    emit_unit_object(
        &snapshot,
        &type_environment,
        UnitEntrySelection::Single {
            source,
            selection: target.entry,
        },
        target.object,
    )?;
    link_native_object(target.object, target.executable).map_err(BootstrapError::Linker)?;
    Ok(())
}

fn single_logical_path(
    sources: &SourceMap,
    parsed: &ParsedFile,
    source: &Path,
    explicit_root: Option<&Path>,
) -> Result<String, BootstrapError> {
    // 只消费原指定路径的尾部组件；剩余 root 保留 ParentDir 和宿主 prefix 语义。
    let mut components = source.components();
    let Some(Component::Normal(file_name)) = components.next_back() else {
        return Err(package_path_error(sources, parsed)?);
    };
    let mut logical_components = vec![file_name];
    if let Some(package) = parsed.package() {
        for segment in package.segments.iter().rev() {
            let name = sources
                .slice(segment.span)
                .map_err(UnitBuildError::Source)?;
            match components.next_back() {
                Some(Component::Normal(component)) if component == OsStr::new(name) => {
                    logical_components.push(component);
                }
                _ => return Err(package_path_error(sources, parsed)?),
            }
        }
    }
    if let Some(explicit_root) = explicit_root {
        let inferred_root = components.as_path();
        let inferred_root = if inferred_root.as_os_str().is_empty() {
            Path::new(".")
        } else {
            inferred_root
        };
        if inspect_source_root(inferred_root)? != inspect_source_root(explicit_root)? {
            return Err(package_path_error(sources, parsed)?);
        }
    }
    logical_components.reverse();
    logical_components
        .into_iter()
        .map(|component| {
            component
                .to_str()
                .ok_or_else(|| BootstrapError::NonUtf8SourcePath(source.to_path_buf()))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|components| components.join("/"))
}

fn inspect_source_root(path: &Path) -> Result<PathBuf, BootstrapError> {
    fs::canonicalize(path).map_err(|error| BootstrapError::InspectSourceRoot {
        path: path.to_path_buf(),
        kind: error.kind(),
    })
}

fn package_path_error(
    sources: &SourceMap,
    parsed: &ParsedFile,
) -> Result<BootstrapError, BootstrapError> {
    let span = parsed
        .package()
        .map_or_else(
            || sources.span(parsed.source_id(), 0, 0),
            |package| Ok(package.span),
        )
        .map_err(UnitBuildError::Source)?;
    let mismatch =
        package_path_mismatch_diagnostic(sources, span).map_err(UnitBuildError::Input)?;
    let mut diagnostics = parsed.diagnostics().to_vec();
    diagnostics.push(mismatch);
    Ok(source_diagnostics(sources, &diagnostics))
}

fn source_diagnostics(sources: &SourceMap, diagnostics: &[Diagnostic]) -> BootstrapError {
    let human = match render_diagnostics(sources, diagnostics) {
        Ok(human) => human,
        Err(error) => return UnitBuildError::DiagnosticRendering(error.to_string()).into(),
    };
    let json = match render_machine_diagnostics(sources, diagnostics) {
        Ok(json) => json,
        Err(error) => return UnitBuildError::DiagnosticRendering(error.to_string()).into(),
    };
    UnitBuildError::FrontendDiagnostics { human, json }.into()
}

/// 构建、链接并运行仓库 bootstrap target；测试外壳不增加另一条语义链。
#[cfg(test)]
pub(crate) fn bootstrap_and_run(target: BootstrapTarget<'_>) -> Result<(), BootstrapError> {
    bootstrap_build(target)?;
    let output = Command::new(target.executable).output().map_err(|error| {
        BootstrapError::LaunchExecutable {
            path: target.executable.to_path_buf(),
            kind: error.kind(),
        }
    })?;
    if !output.status.success() {
        return Err(BootstrapError::ProcessFailure {
            status: output.status.code(),
        });
    }
    Ok(())
}

fn validate_paths(target: BootstrapTarget<'_>) -> Result<(), BootstrapError> {
    if target.source == target.object
        || target.source == target.executable
        || target.object == target.executable
    {
        return Err(BootstrapError::InvalidPaths);
    }
    for output in [target.object, target.executable] {
        if output.exists() {
            return Err(BootstrapError::OutputExists(output.to_path_buf()));
        }
    }
    Ok(())
}
