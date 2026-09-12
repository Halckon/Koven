//! Validated project source-set 到单一 native object 的编排。

use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use lang_codegen::{
    NativeObjectError, NativeUnitEntry, emit_native_constant_unit_object, emit_native_unit_object,
};
use lang_frontend::{
    ast::AstError,
    diagnostic::Diagnostic,
    lexer::{LexerInternalError, lex},
    name_resolution::{
        CompilationUnitInputError, CompilationUnitNameError, DeclarationVisibility, SourceUnit,
        SourceUnitInput, SymbolKind, UnitDiagnosticOrderError, ValidatedCompilationUnitNames,
        index_compilation_unit, ordered_unit_diagnostics, resolve_compilation_unit_names,
    },
    ownership_checking::{
        OwnershipCheckingError, check_compilation_unit_constant_ownership,
        check_compilation_unit_ownership,
    },
    parser::{FunctionBody, FunctionForm, Item, ParsedFile, ParserInternalError, parse_file},
    source::{SourceError, SourceMap},
    type_checking::{
        BuiltinType, CompilationUnitTypeError, CompilationUnitTypes, IntrinsicTypeConstructor,
        ParameterMode, UnitCallableTarget, UnitTypeKind, check_compilation_unit_types,
        standard_environments,
    },
};

use crate::{
    diagnostic_renderer::render_diagnostics_in_order,
    machine_diagnostic_renderer::render_machine_diagnostics_in_order,
    project::{ProjectLoadError, ProjectSourceSet, load_project_source_set},
};

/// Project build 的 frontend、entry 或 object emission 失败。
#[derive(Debug)]
pub(crate) enum ProjectBuildError {
    Project(ProjectLoadError),
    Source(SourceError),
    Lexer(LexerInternalError),
    Parser(ParserInternalError),
    Input(CompilationUnitInputError),
    Name(CompilationUnitNameError),
    Type(CompilationUnitTypeError),
    Ownership(OwnershipCheckingError),
    DiagnosticOrder(UnitDiagnosticOrderError),
    DiagnosticRendering(String),
    FrontendDiagnostics { human: String, json: String },
    IncompleteOwnership,
    InvalidPaths(String),
    InspectPath { path: PathBuf, kind: io::ErrorKind },
    InvalidAst(AstError),
    MissingEntry(String),
    InaccessibleEntry(String),
    InvalidEntryShape(String),
    AmbiguousEntry { selector: String, count: usize },
    Codegen(NativeObjectError),
}

impl fmt::Display for ProjectBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Project(error) => write!(formatter, "project load failed: {error}"),
            Self::Source(error) => write!(formatter, "source setup failed: {error}"),
            Self::Lexer(error) => write!(formatter, "lexer failed: {error}"),
            Self::Parser(error) => write!(formatter, "parser failed: {error}"),
            Self::Input(error) => write!(formatter, "compilation-unit input failed: {error}"),
            Self::Name(error) => write!(formatter, "name resolution failed: {error}"),
            Self::Type(error) => write!(formatter, "type checking failed: {error}"),
            Self::Ownership(error) => write!(formatter, "ownership checking failed: {error}"),
            Self::DiagnosticOrder(error) => {
                write!(formatter, "diagnostic ordering failed: {error}")
            }
            Self::DiagnosticRendering(error) => {
                write!(formatter, "diagnostic rendering failed: {error}")
            }
            Self::FrontendDiagnostics { .. } => {
                formatter.write_str("frontend produced diagnostics")
            }
            Self::IncompleteOwnership => {
                formatter.write_str("ownership analysis contains deferred codegen facts")
            }
            Self::InvalidPaths(detail) => formatter.write_str(detail),
            Self::InspectPath { path, kind } => write!(
                formatter,
                "cannot inspect project output path {}: {kind}",
                path.display()
            ),
            Self::InvalidAst(error) => write!(formatter, "entry AST is invalid: {error}"),
            Self::MissingEntry(selector) => {
                write!(formatter, "project entry `{selector}` was not found")
            }
            Self::InaccessibleEntry(selector) => {
                write!(formatter, "project entry `{selector}` is private")
            }
            Self::InvalidEntryShape(selector) => write!(
                formatter,
                "project entry `{selector}` has no supported process shape; expected `fun {}(): Unit` or `fun {}(args: Array<String>): Unit`",
                selector.rsplit('.').next().unwrap_or(selector),
                selector.rsplit('.').next().unwrap_or(selector),
            ),
            Self::AmbiguousEntry { selector, count } => write!(
                formatter,
                "project entry `{selector}` is ambiguous ({count} valid candidates)"
            ),
            Self::Codegen(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for ProjectBuildError {}

/// 加载显式 project、完成 validated unit frontend，并生成一个 project-owned object。
pub(crate) fn emit_project_object(
    manifest: &Path,
    selector: &[String],
    object: &Path,
    protected_outputs: &[&Path],
) -> Result<(), ProjectBuildError> {
    let project = load_project_source_set(manifest).map_err(ProjectBuildError::Project)?;
    validate_project_paths(manifest, &project, protected_outputs)?;
    let mut sources = SourceMap::new();
    let mut source_ids = Vec::with_capacity(project.sources().len());
    let mut parsed = Vec::with_capacity(project.sources().len());
    for source in project.sources() {
        let source_name = format!("{}/{}", source.root_identity(), source.logical_path());
        let source_id = sources
            .add_source(source_name, source.text())
            .map_err(ProjectBuildError::Source)?;
        let lexed = lex(&sources, source_id).map_err(ProjectBuildError::Lexer)?;
        parsed.push(parse_file(&sources, &lexed).map_err(ProjectBuildError::Parser)?);
        source_ids.push(source_id);
    }
    let inputs = project_inputs(&project, &source_ids, &parsed);
    let index = index_compilation_unit(&sources, &inputs).map_err(ProjectBuildError::Input)?;
    let (name_environment, type_environment) = standard_environments();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
        .map_err(ProjectBuildError::Name)?;
    if !names.diagnostics().is_empty() {
        return Err(frontend_diagnostics(
            &sources,
            names.index().source_units(),
            names.diagnostics(),
        )?);
    }
    let names = match names.validate() {
        Ok(names) => names,
        Err(names) => {
            return Err(frontend_diagnostics(
                &sources,
                names.index().source_units(),
                names.diagnostics(),
            )?);
        }
    };
    let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
        .map_err(ProjectBuildError::Type)?;
    if !typed.diagnostics().is_empty() {
        return Err(frontend_diagnostics(
            &sources,
            names.names().index().source_units(),
            typed.diagnostics(),
        )?);
    }
    let typed = match typed.validate() {
        Ok(typed) => typed,
        Err(typed) => {
            // 基础 capability 拒绝常量；交给 frontend 专用 gate，不从 AST 猜测能力。
            let typed = match typed.validate_constants() {
                Ok(typed) => typed,
                Err(typed) => {
                    return Err(frontend_diagnostics(
                        &sources,
                        names.names().index().source_units(),
                        typed.diagnostics(),
                    )?);
                }
            };
            let owned = check_compilation_unit_constant_ownership(
                &sources,
                &inputs,
                &names,
                &type_environment,
                &typed,
            )
            .map_err(ProjectBuildError::Ownership)?;
            if !owned.ownership().diagnostics().is_empty() {
                return Err(frontend_diagnostics(
                    &sources,
                    names.names().index().source_units(),
                    owned.ownership().diagnostics(),
                )?);
            }
            let owned = owned
                .validate()
                .map_err(|_| ProjectBuildError::IncompleteOwnership)?;
            let entry = select_project_entry(selector, &inputs, &names, typed.types())?;
            return emit_native_constant_unit_object(
                &sources,
                &inputs,
                &names,
                &type_environment,
                &typed,
                &owned,
                entry,
                object,
            )
            .map_err(|error| codegen_error(&sources, names.names().index().source_units(), error));
        }
    };
    let owned =
        check_compilation_unit_ownership(&sources, &inputs, &names, &type_environment, &typed)
            .map_err(ProjectBuildError::Ownership)?;
    if !owned.diagnostics().is_empty() {
        return Err(frontend_diagnostics(
            &sources,
            names.names().index().source_units(),
            owned.diagnostics(),
        )?);
    }
    let owned = match owned.validate() {
        Ok(owned) => owned,
        Err(_) => return Err(ProjectBuildError::IncompleteOwnership),
    };
    let entry = select_project_entry(selector, &inputs, &names, typed.types())?;
    emit_native_unit_object(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        entry,
        object,
    )
    .map_err(|error| codegen_error(&sources, names.names().index().source_units(), error))
}

fn validate_project_paths(
    manifest: &Path,
    project: &ProjectSourceSet,
    outputs: &[&Path],
) -> Result<(), ProjectBuildError> {
    let manifest = fs::canonicalize(manifest).map_err(|error| ProjectBuildError::InspectPath {
        path: manifest.to_path_buf(),
        kind: error.kind(),
    })?;
    let sources = project
        .sources()
        .iter()
        .map(|source| {
            fs::canonicalize(source.presentation_path()).map_err(|error| {
                ProjectBuildError::InspectPath {
                    path: source.presentation_path().to_path_buf(),
                    kind: error.kind(),
                }
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let roots = project
        .roots()
        .iter()
        .map(|root| {
            fs::canonicalize(root.presentation_path()).map_err(|error| {
                ProjectBuildError::InspectPath {
                    path: root.presentation_path().to_path_buf(),
                    kind: error.kind(),
                }
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    for output in outputs {
        if let Ok(output) = fs::canonicalize(output)
            && (output == manifest || sources.contains(&output))
        {
            return Err(ProjectBuildError::InvalidPaths(format!(
                "project output overlaps manifest or source: {}",
                output.display()
            )));
        }
        if output
            .extension()
            .is_some_and(|extension| extension == "ko")
        {
            let parent = output.parent().unwrap_or_else(|| Path::new("."));
            let canonical_parent =
                fs::canonicalize(parent).map_err(|error| ProjectBuildError::InspectPath {
                    path: parent.to_path_buf(),
                    kind: error.kind(),
                })?;
            if roots.iter().any(|root| canonical_parent.starts_with(root)) {
                return Err(ProjectBuildError::InvalidPaths(format!(
                    "project output would enter the next source discovery: {}",
                    output.display()
                )));
            }
        }
    }
    Ok(())
}

fn project_inputs<'a>(
    project: &'a ProjectSourceSet,
    source_ids: &'a [lang_frontend::source::SourceId],
    parsed: &'a [ParsedFile],
) -> Vec<SourceUnitInput<'a>> {
    project
        .sources()
        .iter()
        .zip(source_ids.iter().copied())
        .zip(parsed)
        .map(|((source, source_id), parsed)| {
            SourceUnitInput::new(
                source.root_identity(),
                source.logical_path(),
                source_id,
                parsed,
            )
        })
        .collect()
}

fn select_project_entry(
    selector: &[String],
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
) -> Result<NativeUnitEntry, ProjectBuildError> {
    let selector_text = selector.join(".");
    let (name, package_segments) = selector
        .split_last()
        .expect("validated project selector has at least one segment");
    let index = names.names().index();
    let Some(package) = index.packages().iter().find(|package| {
        package
            .name()
            .segments()
            .iter()
            .map(String::as_str)
            .eq(package_segments.iter().map(String::as_str))
    }) else {
        return Err(ProjectBuildError::MissingEntry(selector_text));
    };
    let candidates = index
        .declarations()
        .iter()
        .filter(|declaration| declaration.package() == package.id() && declaration.name() == name)
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err(ProjectBuildError::MissingEntry(selector_text));
    }
    let visible = candidates
        .into_iter()
        .filter(|declaration| declaration.visibility() != DeclarationVisibility::Private)
        .collect::<Vec<_>>();
    if visible.is_empty() {
        return Err(ProjectBuildError::InaccessibleEntry(selector_text));
    }
    let mut valid = Vec::new();
    for declaration in visible {
        if declaration.kind() != SymbolKind::Function
            || !declaration_has_body(
                inputs,
                declaration.source_unit().index(),
                declaration.root(),
            )?
        {
            continue;
        }
        let Some(callable) = typed
            .signatures()
            .declaration(declaration.id())
            .and_then(|signature| signature.callable())
        else {
            continue;
        };
        if callable.target() != UnitCallableTarget::Declaration(declaration.id())
            || !callable.type_parameters().is_empty()
            || typed.types().get(callable.return_type())
                != Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
        {
            continue;
        }
        if callable.parameters().is_empty() {
            valid.push(NativeUnitEntry::NoArguments(declaration.id()));
        } else if matches!(
            callable.parameters(),
            [parameter]
                if parameter.mode() == ParameterMode::Borrow
                    && matches!(
                        typed.types().get(parameter.ty()),
                        Some(UnitTypeKind::Intrinsic {
                            constructor: IntrinsicTypeConstructor::Array,
                            arguments,
                        }) if matches!(arguments.as_slice(), [string]
                            if typed.types().get(*string)
                                == Some(&UnitTypeKind::Builtin(BuiltinType::String)))
                    )
        ) {
            valid.push(NativeUnitEntry::BorrowedArguments(declaration.id()));
        }
    }
    match valid.as_slice() {
        [entry] => Ok(*entry),
        [] => Err(ProjectBuildError::InvalidEntryShape(selector_text)),
        entries => Err(ProjectBuildError::AmbiguousEntry {
            selector: selector_text,
            count: entries.len(),
        }),
    }
}

fn declaration_has_body(
    inputs: &[SourceUnitInput<'_>],
    source_unit: usize,
    root: lang_frontend::ast::ItemId,
) -> Result<bool, ProjectBuildError> {
    let parsed = inputs
        .get(source_unit)
        .expect("validated declaration source unit exists")
        .parsed();
    let mut item = parsed
        .ast()
        .items()
        .get(root)
        .map_err(ProjectBuildError::InvalidAst)?
        .payload();
    while let Item::Modified { declaration, .. } = item {
        item = parsed
            .ast()
            .items()
            .get(*declaration)
            .map_err(ProjectBuildError::InvalidAst)?
            .payload();
    }
    Ok(matches!(
        item,
        Item::Function {
            form: FunctionForm::ImplicitUnitBlock(_),
            ..
        } | Item::Function {
            form: FunctionForm::Explicit {
                body: FunctionBody::Expression { .. } | FunctionBody::Block(_),
                ..
            },
            ..
        }
    ))
}

fn frontend_diagnostics(
    sources: &SourceMap,
    source_units: &[SourceUnit],
    diagnostics: &[Diagnostic],
) -> Result<ProjectBuildError, ProjectBuildError> {
    let ordered = ordered_unit_diagnostics(sources, source_units, diagnostics)
        .map_err(ProjectBuildError::DiagnosticOrder)?;
    let human = render_diagnostics_in_order(sources, &ordered)
        .map_err(|error| ProjectBuildError::DiagnosticRendering(error.to_string()))?;
    let json = render_machine_diagnostics_in_order(sources, &ordered)
        .map_err(|error| ProjectBuildError::DiagnosticRendering(error.to_string()))?;
    Ok(ProjectBuildError::FrontendDiagnostics { human, json })
}

fn codegen_error(
    sources: &SourceMap,
    source_units: &[SourceUnit],
    error: NativeObjectError,
) -> ProjectBuildError {
    let Some(diagnostic) = error.diagnostic() else {
        return ProjectBuildError::Codegen(error);
    };
    match frontend_diagnostics(sources, source_units, std::slice::from_ref(diagnostic)) {
        Ok(rendered) => rendered,
        Err(rendering) => rendering,
    }
}

#[cfg(test)]
mod tests {
    use lang_codegen::NativeUnitEntry;
    use lang_frontend::{
        lexer::lex,
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        parser::{ParsedFile, parse_file},
        source::{SourceId, SourceMap},
        type_checking::{check_compilation_unit_types, standard_environments},
    };

    use super::{ProjectBuildError, select_project_entry};

    #[test]
    fn entry_resolver_filters_visibility_body_generics_and_process_shape() {
        let mut sources = SourceMap::new();
        let (root_source, root) = parsed(
            &mut sources,
            "root/Root.ko",
            "fun defaultEntry(): Unit {}\n\
             private fun secret(): Unit {}\n\
             fun absent(): Unit\n\
             fun invalid(number: Int): Unit {}\n\
             fun mixed(number: Int): Unit {}\n\
             fun mixed(): Unit {}\n\
             fun ambiguous(): Unit {}\n\
             fun ambiguous(args: Array<String>): Unit {}\n\
             fun <T> generic(): Unit {}\n\
             val notFunction = 1\n",
        );
        let (tools_source, tools) = parsed(
            &mut sources,
            "root/tools/Entries.ko",
            "package tools\ninternal fun argv(args: Array<String>): Unit {}\n",
        );
        let inputs = [
            SourceUnitInput::new("root", "Root.ko", root_source, &root),
            SourceUnitInput::new("root", "tools/Entries.ko", tools_source, &tools),
        ];
        let (name_environment, type_environment) = standard_environments();
        let index = index_compilation_unit(&sources, &inputs).expect("unit index");
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &name_environment)
            .expect("unit names")
            .validate()
            .expect("validated names");
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &type_environment)
            .expect("unit types")
            .validate()
            .expect("validated types");

        assert!(matches!(
            select_project_entry(&segments("defaultEntry"), &inputs, &names, typed.types()),
            Ok(NativeUnitEntry::NoArguments(_))
        ));
        assert!(matches!(
            select_project_entry(&segments("tools.argv"), &inputs, &names, typed.types()),
            Ok(NativeUnitEntry::BorrowedArguments(_))
        ));
        assert!(matches!(
            select_project_entry(&segments("mixed"), &inputs, &names, typed.types()),
            Ok(NativeUnitEntry::NoArguments(_))
        ));
        assert!(matches!(
            select_project_entry(&segments("secret"), &inputs, &names, typed.types()),
            Err(ProjectBuildError::InaccessibleEntry(_))
        ));
        for selector in ["absent", "invalid", "generic", "notFunction"] {
            assert!(matches!(
                select_project_entry(&segments(selector), &inputs, &names, typed.types()),
                Err(ProjectBuildError::InvalidEntryShape(_))
            ));
        }
        assert!(matches!(
            select_project_entry(&segments("missing"), &inputs, &names, typed.types()),
            Err(ProjectBuildError::MissingEntry(_))
        ));
        assert!(matches!(
            select_project_entry(&segments("unknown.missing"), &inputs, &names, typed.types()),
            Err(ProjectBuildError::MissingEntry(_))
        ));
        assert!(matches!(
            select_project_entry(&segments("ambiguous"), &inputs, &names, typed.types()),
            Err(ProjectBuildError::AmbiguousEntry { count: 2, .. })
        ));
    }

    fn parsed(sources: &mut SourceMap, name: &str, text: &str) -> (SourceId, ParsedFile) {
        let source = sources.add_source(name, text).expect("unique source");
        let lexed = lex(sources, source).expect("lexing succeeds");
        let parsed = parse_file(sources, &lexed).expect("parsing succeeds");
        assert!(
            parsed.diagnostics().is_empty(),
            "{:?}",
            parsed.diagnostics()
        );
        (source, parsed)
    }

    fn segments(selector: &str) -> Vec<String> {
        selector.split('.').map(str::to_owned).collect()
    }
}
