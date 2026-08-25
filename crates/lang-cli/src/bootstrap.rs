//! 仓库拥有的单文件标准库 bootstrap 编排。

use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};

#[cfg(test)]
use std::process::Command;

use lang_codegen::{NativeObjectError, emit_native_object};
use lang_frontend::{
    diagnostic::Diagnostic,
    lexer::{LexerInternalError, lex},
    name_resolution::{NameResolutionError, ScopeKind, SymbolKind, resolve_names},
    ownership_checking::{OwnershipCheckingError, check_ownership},
    parser::{ParserInternalError, parse_file},
    source::{SourceError, SourceMap},
    type_checking::{TypeCheckingError, check_types, standard_environments},
};

use crate::linker::{LinkerError, link_native_object};
use crate::{
    diagnostic_renderer::render_diagnostics,
    machine_diagnostic_renderer::render_machine_diagnostics,
};

/// 仓库 bootstrap 中产生用户诊断的 frontend 阶段。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrontendStage {
    Lexer,
    Parser,
    NameResolution,
    TypeChecking,
    OwnershipChecking,
}

/// 一次显式单文件 bootstrap 的调用方拥有配置。
#[derive(Clone, Copy, Debug)]
pub(crate) struct BootstrapTarget<'a> {
    pub(crate) source: &'a Path,
    pub(crate) entry_name: &'a str,
    pub(crate) object: &'a Path,
    pub(crate) executable: &'a Path,
}

/// 标准库 bootstrap 各阶段的结构化失败。
#[derive(Debug)]
pub(crate) enum BootstrapError {
    InvalidPaths,
    OutputExists(PathBuf),
    ReadSource {
        path: PathBuf,
        kind: io::ErrorKind,
    },
    NonUtf8SourcePath(PathBuf),
    Source(SourceError),
    Lexer(LexerInternalError),
    Parser(ParserInternalError),
    NameResolution(NameResolutionError),
    TypeChecking(TypeCheckingError),
    OwnershipChecking(OwnershipCheckingError),
    FrontendDiagnostics {
        stage: FrontendStage,
        diagnostics: Vec<Diagnostic>,
        human: String,
        json: String,
    },
    DiagnosticRendering(String),
    MissingEntry(String),
    AmbiguousEntry {
        name: String,
        count: usize,
    },
    Codegen(NativeObjectError),
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
            Self::NonUtf8SourcePath(path) => {
                write!(
                    formatter,
                    "source path is not valid UTF-8: {}",
                    path.display()
                )
            }
            Self::Source(error) => write!(formatter, "source setup failed: {error}"),
            Self::Lexer(error) => write!(formatter, "lexer failed: {error}"),
            Self::Parser(error) => write!(formatter, "parser failed: {error}"),
            Self::NameResolution(error) => write!(formatter, "name resolution failed: {error}"),
            Self::TypeChecking(error) => write!(formatter, "type checking failed: {error}"),
            Self::OwnershipChecking(error) => {
                write!(formatter, "ownership checking failed: {error}")
            }
            Self::FrontendDiagnostics {
                stage, diagnostics, ..
            } => write!(
                formatter,
                "{stage:?} produced {} diagnostic(s)",
                diagnostics.len()
            ),
            Self::DiagnosticRendering(error) => {
                write!(formatter, "diagnostic rendering failed: {error}")
            }
            Self::MissingEntry(name) => write!(formatter, "entry `{name}` was not found"),
            Self::AmbiguousEntry { name, count } => {
                write!(
                    formatter,
                    "entry `{name}` is ambiguous ({count} candidates)"
                )
            }
            Self::Codegen(error) => write!(formatter, "{error}"),
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

/// 从一份显式 Koven source 构建并链接一个 native target。
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
        .map_err(BootstrapError::Source)?;

    let lexed = lex(&sources, source).map_err(BootstrapError::Lexer)?;
    reject_diagnostics(FrontendStage::Lexer, &sources, lexed.diagnostics())?;
    let parsed = parse_file(&sources, &lexed).map_err(BootstrapError::Parser)?;
    reject_diagnostics(FrontendStage::Parser, &sources, parsed.diagnostics())?;

    let (name_environment, type_environment) = standard_environments();
    let names = resolve_names(&sources, &parsed, &name_environment)
        .map_err(BootstrapError::NameResolution)?;
    reject_diagnostics(FrontendStage::NameResolution, &sources, names.diagnostics())?;
    let typed = check_types(&sources, &parsed, &names, &type_environment)
        .map_err(BootstrapError::TypeChecking)?;
    reject_diagnostics(FrontendStage::TypeChecking, &sources, typed.diagnostics())?;
    let owned = check_ownership(&sources, &parsed, &names, &typed)
        .map_err(BootstrapError::OwnershipChecking)?;
    reject_diagnostics(
        FrontendStage::OwnershipChecking,
        &sources,
        owned.diagnostics(),
    )?;

    let mut entries = names.symbols().iter().filter(|symbol| {
        symbol.name() == target.entry_name
            && symbol.kind() == SymbolKind::Function
            && names
                .scopes()
                .get(symbol.scope().index())
                .is_some_and(|scope| scope.kind() == ScopeKind::File && scope.parent().is_none())
    });
    let entry = entries
        .next()
        .ok_or_else(|| BootstrapError::MissingEntry(target.entry_name.to_owned()))?;
    let count = 1 + entries.count();
    if count != 1 {
        return Err(BootstrapError::AmbiguousEntry {
            name: target.entry_name.to_owned(),
            count,
        });
    }

    emit_native_object(
        &sources,
        &parsed,
        &names,
        &typed,
        &owned,
        entry.id(),
        target.object,
    )
    .map_err(BootstrapError::Codegen)?;
    link_native_object(target.object, target.executable).map_err(BootstrapError::Linker)?;
    Ok(())
}

/// 从一份显式 Koven source 构建、链接并运行仓库 bootstrap target。
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

fn reject_diagnostics(
    stage: FrontendStage,
    sources: &SourceMap,
    diagnostics: &[Diagnostic],
) -> Result<(), BootstrapError> {
    if diagnostics.is_empty() {
        Ok(())
    } else {
        let human = render_diagnostics(sources, diagnostics)
            .map_err(|error| BootstrapError::DiagnosticRendering(error.to_string()))?;
        let json = render_machine_diagnostics(sources, diagnostics)
            .map_err(|error| BootstrapError::DiagnosticRendering(error.to_string()))?;
        Err(BootstrapError::FrontendDiagnostics {
            stage,
            diagnostics: diagnostics.to_vec(),
            human,
            json,
        })
    }
}
