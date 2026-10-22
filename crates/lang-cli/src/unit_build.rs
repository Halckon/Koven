//! 同轮 compilation-unit frontend 到单一 native object 的 CLI 内部编排。

use std::{error::Error, fmt, path::Path};

use lang_codegen::{
    NativeObjectError, NativeUnitEntry, emit_native_const_owned_unit_object,
    emit_native_owned_unit_object,
};
use lang_frontend::{
    analysis::{UnitNameAnalysisError, UnitNameSnapshot, analyze_basic_unit_ownership},
    ast::AstError,
    diagnostic::Diagnostic,
    lexer::LexerInternalError,
    name_resolution::{
        CompilationUnitInputError, CompilationUnitNameError, DeclarationId, DeclarationVisibility,
        SourceUnit, SourceUnitInput, SymbolKind, UnitDiagnosticOrderError,
        ValidatedCompilationUnitNames, ordered_unit_diagnostics,
    },
    ownership_checking::{
        ConstEnabledOwnedUnit, OwnershipCheckingError, ValidatedCompilationUnitOwnership,
        check_compilation_unit_constant_ownership, const_owned_compilation_unit_view,
        owned_compilation_unit_view,
    },
    parser::{FunctionBody, FunctionForm, Item, ParserInternalError},
    source::{SourceError, SourceId, SourceMap},
    type_checking::{
        BuiltinType, CompilationUnitTypeError, CompilationUnitTypes, ConstEnabledTypedUnit,
        FinalTypeAdmissionError, FinalTypeAdmissionInternalError, IntrinsicTypeConstructor,
        ParameterMode, TypeEnvironment, UnitCallableTarget, UnitTypeKind,
        ValidatedCompilationUnitTypes, check_compilation_unit_types,
        complete_compilation_unit_types,
    },
};

use crate::{
    diagnostic_renderer::render_diagnostics_in_order,
    machine_diagnostic_renderer::render_machine_diagnostics_in_order,
};

/// CLI 内部共同 frontend、entry 或 object emission 失败。
#[derive(Debug)]
pub(crate) enum UnitBuildError {
    Source(SourceError),
    StandardSource(lang_frontend::type_checking::TypeCheckingError),
    Lexer(LexerInternalError),
    Parser(ParserInternalError),
    Input(CompilationUnitInputError),
    Name(CompilationUnitNameError),
    Type(CompilationUnitTypeError),
    TypeAdmission(FinalTypeAdmissionInternalError),
    Ownership(OwnershipCheckingError),
    DiagnosticOrder(UnitDiagnosticOrderError),
    DiagnosticRendering(String),
    FrontendDiagnostics { human: String, json: String },
    IncompleteOwnership,
    InvalidAst(AstError),
    ProjectEntry(ProjectEntryError),
    SingleEntry(SingleEntryError),
    MissingSingleSource,
    UnresolvedPackageIdentity,
    Codegen(NativeObjectError),
}

/// 单文件 driver 选择策略；不改变共同 frontend 或 native 语义。
#[derive(Clone, Copy, Debug)]
pub(crate) enum SingleEntrySelection<'a> {
    Explicit(&'a str),
    ConventionalMain,
}

/// 两种实际宿主只在 entry 选择上保留各自合同。
#[derive(Clone, Copy, Debug)]
pub(crate) enum UnitEntrySelection<'a> {
    Project(&'a [String]),
    Single {
        source: SourceId,
        selection: SingleEntrySelection<'a>,
    },
}

/// 单文件入口选择操作错误，保留原完整消息。
#[derive(Debug)]
pub(crate) enum SingleEntryError {
    Missing(String),
    Ambiguous { name: String, count: usize },
    InvalidConventionalShape(String),
}

impl fmt::Display for SingleEntryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(name) => write!(formatter, "entry `{name}` was not found"),
            Self::Ambiguous { name, count } => {
                write!(
                    formatter,
                    "entry `{name}` is ambiguous ({count} candidates)"
                )
            }
            Self::InvalidConventionalShape(name) => write!(
                formatter,
                "entry `{name}` has no supported conventional shape; expected `fun {name}(): Unit` \
                 or `fun {name}(args: Array<String>): Unit`"
            ),
        }
    }
}

/// Project selector 的失败；可见性、body 和计数合同由该宿主保留。
#[derive(Debug)]
pub(crate) enum ProjectEntryError {
    Missing(String),
    Inaccessible(String),
    InvalidShape(String),
    Ambiguous { selector: String, count: usize },
}

impl fmt::Display for ProjectEntryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing(selector) => write!(formatter, "entry `{selector}` was not found"),
            Self::Inaccessible(selector) => write!(formatter, "entry `{selector}` is private"),
            Self::InvalidShape(selector) => write!(
                formatter,
                "entry `{selector}` has no supported process shape; expected `fun {}(): Unit` or `fun {}(args: Array<String>): Unit`",
                selector.rsplit('.').next().unwrap_or(selector),
                selector.rsplit('.').next().unwrap_or(selector),
            ),
            Self::Ambiguous { selector, count } => write!(
                formatter,
                "entry `{selector}` is ambiguous ({count} valid candidates)"
            ),
        }
    }
}

impl fmt::Display for UnitBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StandardSource(error) => {
                write!(formatter, "standard source binding failed: {error}")
            }
            Self::Source(error) => write!(formatter, "source setup failed: {error}"),
            Self::Lexer(error) => write!(formatter, "lexer failed: {error}"),
            Self::Parser(error) => write!(formatter, "parser failed: {error}"),
            Self::Input(error) => write!(formatter, "compilation-unit input failed: {error}"),
            Self::Name(error) => write!(formatter, "name resolution failed: {error}"),
            Self::Type(error) => write!(formatter, "type checking failed: {error}"),
            Self::TypeAdmission(error) => {
                write!(formatter, "final type admission failed: {error}")
            }
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
            Self::InvalidAst(error) => write!(formatter, "entry AST is invalid: {error}"),
            Self::ProjectEntry(error) => error.fmt(formatter),
            Self::SingleEntry(error) => error.fmt(formatter),
            Self::UnresolvedPackageIdentity => {
                formatter.write_str("validated unit contains an unresolved package identity")
            }
            Self::MissingSingleSource => {
                formatter.write_str("single-file entry source is absent from compilation unit")
            }
            Self::Codegen(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for UnitBuildError {}

impl From<UnitNameAnalysisError> for UnitBuildError {
    fn from(error: UnitNameAnalysisError) -> Self {
        match error {
            UnitNameAnalysisError::Lexer(error) => Self::Lexer(error),
            UnitNameAnalysisError::Parser(error) => Self::Parser(error),
            UnitNameAnalysisError::Input(error) => Self::Input(error),
            UnitNameAnalysisError::Name(error) => Self::Name(error),
        }
    }
}

/// 全部共同 frontend gate 通过后的 typed/owned 产物；`check` 到此为止，build/run 继续 codegen。
pub(crate) struct CheckedUnit<'a> {
    names: &'a ValidatedCompilationUnitNames,
    products: CheckedProducts,
}

enum CheckedProducts {
    Basic {
        typed: ValidatedCompilationUnitTypes,
        owned: ValidatedCompilationUnitOwnership,
    },
    Constant {
        typed: ConstEnabledTypedUnit,
        owned: ConstEnabledOwnedUnit,
    },
}

/// 继续原名称 snapshot，执行 names、types、final type admission、ownership 与 validation；
/// 类型环境必须与建立该 snapshot 的标准环境同轮配对。不选择入口、不生成 object。
pub(crate) fn check_unit_frontend<'a>(
    snapshot: &'a UnitNameSnapshot,
    type_environment: &TypeEnvironment,
) -> Result<CheckedUnit<'a>, UnitBuildError> {
    let sources = snapshot.sources();
    let inputs = snapshot.inputs();
    let names = snapshot.names();
    if !names.diagnostics().is_empty() {
        return Err(frontend_diagnostics(
            sources,
            names.index().source_units(),
            names.diagnostics(),
        )?);
    }
    let names = match snapshot.validated_names() {
        Some(names) => names,
        None => {
            return Err(frontend_diagnostics(
                sources,
                names.index().source_units(),
                names.diagnostics(),
            )?);
        }
    };
    let typed = check_compilation_unit_types(sources, &inputs, names, type_environment)
        .map_err(UnitBuildError::Type)?;
    if !typed.diagnostics().is_empty() {
        return Err(frontend_diagnostics(
            sources,
            names.names().index().source_units(),
            typed.diagnostics(),
        )?);
    }
    match complete_compilation_unit_types(sources, &inputs, names, type_environment, &typed) {
        Ok(_) => {}
        Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) => {
            return Err(frontend_diagnostics(
                sources,
                names.names().index().source_units(),
                &diagnostics,
            )?);
        }
        Err(FinalTypeAdmissionError::Internal(error)) => {
            return Err(UnitBuildError::TypeAdmission(error));
        }
    }
    let outcome = analyze_basic_unit_ownership(sources, &inputs, names, type_environment, typed)
        .map_err(UnitBuildError::Ownership)?;
    let (typed, owned) = match outcome.into_result() {
        Ok(products) => products,
        Err(typed) => {
            // 基础 capability 拒绝常量；交给 frontend 专用 gate，不从 AST 猜测能力。
            let typed = match typed.validate_constants() {
                Ok(typed) => typed,
                Err(typed) => {
                    return Err(frontend_diagnostics(
                        sources,
                        names.names().index().source_units(),
                        typed.diagnostics(),
                    )?);
                }
            };
            let owned = check_compilation_unit_constant_ownership(
                sources,
                &inputs,
                names,
                type_environment,
                &typed,
            )
            .map_err(UnitBuildError::Ownership)?;
            if !owned.ownership().diagnostics().is_empty() {
                return Err(frontend_diagnostics(
                    sources,
                    names.names().index().source_units(),
                    owned.ownership().diagnostics(),
                )?);
            }
            let owned = owned
                .validate()
                .map_err(|_| UnitBuildError::IncompleteOwnership)?;
            return Ok(CheckedUnit {
                names,
                products: CheckedProducts::Constant { typed, owned },
            });
        }
    };
    if !owned.diagnostics().is_empty() {
        return Err(frontend_diagnostics(
            sources,
            names.names().index().source_units(),
            owned.diagnostics(),
        )?);
    }
    let owned = match owned.validate() {
        Ok(owned) => owned,
        Err(_) => return Err(UnitBuildError::IncompleteOwnership),
    };
    Ok(CheckedUnit {
        names,
        products: CheckedProducts::Basic { typed, owned },
    })
}

/// 共同 frontend gate 成功后才执行入口选择与 object emission。
pub(crate) fn emit_unit_object(
    snapshot: &UnitNameSnapshot,
    type_environment: &TypeEnvironment,
    selection: UnitEntrySelection<'_>,
    object: &Path,
) -> Result<(), UnitBuildError> {
    let checked = check_unit_frontend(snapshot, type_environment)?;
    let sources = snapshot.sources();
    let inputs = snapshot.inputs();
    let names = checked.names;
    let source_units = names.names().index().source_units();
    match &checked.products {
        CheckedProducts::Constant { typed, owned } => {
            let entry = select_unit_entry(selection, &inputs, names, typed.types())?;
            let unit = const_owned_compilation_unit_view(
                sources,
                &inputs,
                names,
                type_environment,
                typed,
                owned,
            )
            .map_err(|error| codegen_error(sources, source_units, error.into()))?;
            emit_native_const_owned_unit_object(&unit, entry, object)
                .map_err(|error| codegen_error(sources, source_units, error))
        }
        CheckedProducts::Basic { typed, owned } => {
            let entry = select_unit_entry(selection, &inputs, names, typed.types())?;
            let unit = owned_compilation_unit_view(
                sources,
                &inputs,
                names,
                type_environment,
                typed,
                owned,
            )
            .map_err(|error| codegen_error(sources, source_units, error.into()))?;
            emit_native_owned_unit_object(&unit, entry, object)
                .map_err(|error| codegen_error(sources, source_units, error))
        }
    }
}

fn select_unit_entry(
    selection: UnitEntrySelection<'_>,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
) -> Result<NativeUnitEntry, UnitBuildError> {
    match selection {
        UnitEntrySelection::Project(selector) => {
            select_project_entry(selector, inputs, names, typed)
        }
        UnitEntrySelection::Single { source, selection } => {
            select_single_entry(source, selection, names, typed)
        }
    }
}

fn select_single_entry(
    source: SourceId,
    selection: SingleEntrySelection<'_>,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
) -> Result<NativeUnitEntry, UnitBuildError> {
    let name = match selection {
        SingleEntrySelection::Explicit(name) => name,
        SingleEntrySelection::ConventionalMain => "main",
    };
    let index = names.names().index();
    let source_unit = index
        .source_units()
        .iter()
        .find(|unit| unit.source_id() == source)
        .ok_or(UnitBuildError::MissingSingleSource)?
        .id();
    let candidates = index
        .declarations()
        .iter()
        .filter(|declaration| {
            declaration.source_unit() == source_unit
                && declaration.kind() == SymbolKind::Function
                && declaration.name() == name
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err(UnitBuildError::SingleEntry(SingleEntryError::Missing(
            name.to_owned(),
        )));
    }
    if matches!(selection, SingleEntrySelection::Explicit(_)) {
        return match candidates.as_slice() {
            [entry] => Ok(NativeUnitEntry::NoArguments(entry.id())),
            entries => Err(UnitBuildError::SingleEntry(SingleEntryError::Ambiguous {
                name: name.to_owned(),
                count: entries.len(),
            })),
        };
    }
    let valid = candidates
        .into_iter()
        .filter_map(|declaration| process_entry_shape(declaration.id(), typed))
        .collect::<Vec<_>>();
    match valid.as_slice() {
        [entry] => Ok(*entry),
        [] => Err(UnitBuildError::SingleEntry(
            SingleEntryError::InvalidConventionalShape(name.to_owned()),
        )),
        entries => Err(UnitBuildError::SingleEntry(SingleEntryError::Ambiguous {
            name: name.to_owned(),
            count: entries.len(),
        })),
    }
}

fn process_entry_shape(
    declaration: DeclarationId,
    typed: &CompilationUnitTypes,
) -> Option<NativeUnitEntry> {
    let callable = typed.signatures().declaration(declaration)?.callable()?;
    if callable.target() != UnitCallableTarget::Declaration(declaration)
        || !callable.type_parameters().is_empty()
        || typed.types().get(callable.return_type())
            != Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
    {
        return None;
    }
    if callable.parameters().is_empty() {
        Some(NativeUnitEntry::NoArguments(declaration))
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
        Some(NativeUnitEntry::BorrowedArguments(declaration))
    } else {
        None
    }
}

fn select_project_entry(
    selector: &[String],
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
) -> Result<NativeUnitEntry, UnitBuildError> {
    let selector_text = selector.join(".");
    let (name, package_segments) = selector
        .split_last()
        .expect("validated project selector has at least one segment");
    let index = names.names().index();
    let mut selected_package = None;
    for package in index.packages() {
        let package_name = package
            .name()
            .ok_or(UnitBuildError::UnresolvedPackageIdentity)?;
        if package_name
            .segments()
            .iter()
            .map(String::as_str)
            .eq(package_segments.iter().map(String::as_str))
        {
            selected_package = Some(package);
        }
    }
    let Some(package) = selected_package else {
        return Err(UnitBuildError::ProjectEntry(ProjectEntryError::Missing(
            selector_text,
        )));
    };
    let candidates = index
        .declarations()
        .iter()
        .filter(|declaration| declaration.package() == package.id() && declaration.name() == name)
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err(UnitBuildError::ProjectEntry(ProjectEntryError::Missing(
            selector_text,
        )));
    }
    let visible = candidates
        .into_iter()
        .filter(|declaration| declaration.visibility() != DeclarationVisibility::Private)
        .collect::<Vec<_>>();
    if visible.is_empty() {
        return Err(UnitBuildError::ProjectEntry(
            ProjectEntryError::Inaccessible(selector_text),
        ));
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
        if let Some(entry) = process_entry_shape(declaration.id(), typed) {
            valid.push(entry);
        }
    }
    match valid.as_slice() {
        [entry] => Ok(*entry),
        [] => Err(UnitBuildError::ProjectEntry(
            ProjectEntryError::InvalidShape(selector_text),
        )),
        entries => Err(UnitBuildError::ProjectEntry(ProjectEntryError::Ambiguous {
            selector: selector_text,
            count: entries.len(),
        })),
    }
}

fn declaration_has_body(
    inputs: &[SourceUnitInput<'_>],
    source_unit: usize,
    root: lang_frontend::ast::ItemId,
) -> Result<bool, UnitBuildError> {
    let parsed = inputs
        .get(source_unit)
        .expect("validated declaration source unit exists")
        .parsed();
    let mut item = parsed
        .ast()
        .items()
        .get(root)
        .map_err(UnitBuildError::InvalidAst)?
        .payload();
    while let Item::Modified { declaration, .. } = item {
        item = parsed
            .ast()
            .items()
            .get(*declaration)
            .map_err(UnitBuildError::InvalidAst)?
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
) -> Result<UnitBuildError, UnitBuildError> {
    let ordered = ordered_unit_diagnostics(sources, source_units, diagnostics)
        .map_err(UnitBuildError::DiagnosticOrder)?;
    let human = render_diagnostics_in_order(sources, &ordered)
        .map_err(|error| UnitBuildError::DiagnosticRendering(error.to_string()))?;
    let json = render_machine_diagnostics_in_order(sources, &ordered)
        .map_err(|error| UnitBuildError::DiagnosticRendering(error.to_string()))?;
    Ok(UnitBuildError::FrontendDiagnostics { human, json })
}

fn codegen_error(
    sources: &SourceMap,
    source_units: &[SourceUnit],
    error: NativeObjectError,
) -> UnitBuildError {
    let Some(diagnostic) = error.diagnostic() else {
        return UnitBuildError::Codegen(error);
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

    use super::{ProjectEntryError, UnitBuildError, select_project_entry};

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
             object notFunction {}\n",
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
            Err(UnitBuildError::ProjectEntry(
                ProjectEntryError::Inaccessible(_)
            ))
        ));
        for selector in ["absent", "invalid", "generic", "notFunction"] {
            assert!(matches!(
                select_project_entry(&segments(selector), &inputs, &names, typed.types()),
                Err(UnitBuildError::ProjectEntry(
                    ProjectEntryError::InvalidShape(_)
                ))
            ));
        }
        assert!(matches!(
            select_project_entry(&segments("missing"), &inputs, &names, typed.types()),
            Err(UnitBuildError::ProjectEntry(ProjectEntryError::Missing(_)))
        ));
        assert!(matches!(
            select_project_entry(&segments("unknown.missing"), &inputs, &names, typed.types()),
            Err(UnitBuildError::ProjectEntry(ProjectEntryError::Missing(_)))
        ));
        assert!(matches!(
            select_project_entry(&segments("ambiguous"), &inputs, &names, typed.types()),
            Err(UnitBuildError::ProjectEntry(ProjectEntryError::Ambiguous {
                count: 2,
                ..
            }))
        ));
    }

    #[test]
    fn unit_single_entry_uses_original_user_source_after_standard_sorting() {
        use super::{SingleEntryError, SingleEntrySelection, select_single_entry};
        use lang_frontend::analysis::{UnitSourceDescriptor, analyze_unit_names};

        for standard_first in [false, true] {
            let mut sources = SourceMap::new();
            let mut descriptors = Vec::new();
            let (mut name_environment, mut type_environment) = standard_environments();
            if standard_first {
                crate::standard_sources::append_standard_sources(
                    &mut sources,
                    &mut descriptors,
                    &mut name_environment,
                    &mut type_environment,
                )
                .unwrap();
            }
            let user = sources
                .add_source(
                    "User.ko",
                    "import koven.algorithms.take as main\nfun selected(): Unit {}\n",
                )
                .unwrap();
            descriptors.push(UnitSourceDescriptor::new("single", "User.ko", user));
            if !standard_first {
                crate::standard_sources::append_standard_sources(
                    &mut sources,
                    &mut descriptors,
                    &mut name_environment,
                    &mut type_environment,
                )
                .unwrap();
            }
            let first_registered = descriptors[0].source_id();
            let standard = descriptors
                .iter()
                .find(|descriptor| descriptor.source_id() != user)
                .unwrap()
                .source_id();
            assert_ne!(user, standard);
            assert_eq!(
                first_registered,
                if standard_first { standard } else { user }
            );
            let snapshot = analyze_unit_names(sources, descriptors, name_environment).unwrap();
            assert!(
                snapshot.names().diagnostics().is_empty(),
                "{:?}",
                snapshot.names().diagnostics()
            );
            let units = snapshot.names().index().source_units();
            // koven/algorithms/ranges.ko、koven/core/result.ko 与用户源。
            assert_eq!(units.len(), 3);
            assert_eq!(units[0].source_id(), standard);
            let user_unit = units.iter().find(|unit| unit.source_id() == user).unwrap();
            assert_ne!(user_unit.id().index(), 0);
            assert_eq!(snapshot.sources().source_name(user).unwrap(), "User.ko");
            let names = snapshot.validated_names().unwrap();
            let inputs = snapshot.inputs();
            let typed =
                check_compilation_unit_types(snapshot.sources(), &inputs, names, &type_environment)
                    .unwrap()
                    .validate()
                    .unwrap();
            let entry = select_single_entry(
                user,
                SingleEntrySelection::Explicit("selected"),
                names,
                typed.types(),
            )
            .unwrap();
            let NativeUnitEntry::NoArguments(selected) = entry else {
                panic!("explicit shape");
            };
            let declaration = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|declaration| declaration.id() == selected)
                .unwrap();
            assert_eq!(declaration.source_unit(), user_unit.id());
            assert_eq!(declaration.name(), "selected");
            assert_eq!(declaration.name_span().source_id(), user);
            for (selection, expected) in [
                (SingleEntrySelection::Explicit("main"), "main"),
                (SingleEntrySelection::Explicit("take"), "take"),
                (SingleEntrySelection::ConventionalMain, "main"),
            ] {
                let error = select_single_entry(user, selection, names, typed.types()).unwrap_err();
                assert!(
                    matches!(&error, UnitBuildError::SingleEntry(SingleEntryError::Missing(name)) if name == expected)
                );
                assert_eq!(
                    error.to_string(),
                    format!("entry `{expected}` was not found")
                );
            }
        }
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
