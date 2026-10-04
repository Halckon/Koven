//! SPEC-0198 compilation-unit ownership product integration tests.

use lang_frontend::{
    ast::ExpressionId,
    diagnostic::DiagnosticDetail,
    lexer::lex,
    name_resolution::{
        NameEnvironment, SourceUnitId, SourceUnitInput, UnitSymbolId,
        ValidatedCompilationUnitNames, index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{
        ClosureCaptureEffect, ClosureCaptureMode, CompilationUnitOwnership,
        ConstructionDeliveryKind, ConstructionRootKind, LoanKind, OwnershipBindingKind,
        OwnershipCheckingError, OwnershipDeferredReason, RcOwnershipEffectKind, Transferability,
        UnitCallArgumentOwnershipKind, UnitClosureCaptureSource, UnitDropPoint, UnitDropTarget,
        UnitLoanTarget, UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget,
        UnitValueDeliveryKind, check_compilation_unit_ownership,
    },
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        BuiltinType, Capability, Copyability, EnvironmentFunction, EnvironmentFunctionEffect,
        EnvironmentParameter, EnvironmentType, ParameterMode, TypeEnvironment,
        UnitCallReceiverOrigin, UnitExpressionId, UnitTypeKind, ValidatedCompilationUnitTypes,
        check_compilation_unit_types, standard_environments,
    },
};

fn parsed(sources: &mut SourceMap, name: &str, text: &str) -> (SourceId, ParsedFile) {
    let source = sources.add_source(name, text).expect("unique source");
    let lexed = lex(sources, source).expect("lexing succeeds internally");
    let parsed = parse_file(sources, &lexed).expect("parsing succeeds internally");
    assert!(
        parsed.diagnostics().is_empty(),
        "{:?}",
        parsed.diagnostics()
    );
    (source, parsed)
}

fn validated_names<'a>(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'a>],
    environment: &NameEnvironment,
) -> ValidatedCompilationUnitNames {
    let index = index_compilation_unit(sources, inputs).expect("valid unit input");
    resolve_compilation_unit_names(sources, inputs, &index, environment)
        .expect("name resolution succeeds internally")
        .validate()
        .expect("valid names")
}

fn validated_types(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    names: &ValidatedCompilationUnitNames,
    environment: &TypeEnvironment,
) -> ValidatedCompilationUnitTypes {
    check_compilation_unit_types(sources, inputs, names, environment)
        .expect("type checking succeeds internally")
        .validate()
        .expect("valid compilation-unit types")
}

fn source_unit(names: &ValidatedCompilationUnitNames, source: SourceId) -> SourceUnitId {
    names
        .names()
        .index()
        .source_units()
        .iter()
        .find(|unit| unit.source_id() == source)
        .expect("source belongs to unit")
        .id()
}

fn expression_with_text(sources: &SourceMap, parsed: &ParsedFile, text: &str) -> ExpressionId {
    parsed
        .ast()
        .expressions()
        .iter()
        .find_map(|(id, expression)| {
            sources
                .slice(expression.span())
                .is_ok_and(|actual| actual == text)
                .then_some(id)
        })
        .expect("expression text exists")
}

fn symbol_named(
    ownership: &CompilationUnitOwnership,
    names: &ValidatedCompilationUnitNames,
    source: SourceUnitId,
    name: &str,
) -> UnitSymbolId {
    let resolution = names.names().source_units()[source.index()].resolution();
    let symbol = resolution
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == name)
        .expect("symbol exists");
    ownership
        .bindings()
        .iter()
        .map(|binding| binding.symbol())
        .find(|candidate| candidate.source_unit() == source && candidate.symbol() == symbol.id())
        .expect("source-qualified ownership binding exists")
}

fn diagnostic_codes(ownership: &CompilationUnitOwnership) -> Vec<String> {
    ownership
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.code().to_string())
        .collect()
}

#[path = "multifile_ownership_checking/assignment_rollback.rs"]
mod assignment_rollback;
#[path = "multifile_ownership_checking/call_deliveries.rs"]
mod call_deliveries;
#[path = "multifile_ownership_checking/capture_escape.rs"]
mod capture_escape;
#[path = "multifile_ownership_checking/constructions.rs"]
mod constructions;
#[path = "multifile_ownership_checking/container_places.rs"]
mod container_places;
#[path = "multifile_ownership_checking/control_flow.rs"]
mod control_flow;
#[path = "multifile_ownership_checking/drop_plans.rs"]
mod drop_plans;
#[path = "multifile_ownership_checking/field_mutability.rs"]
mod field_mutability;
#[path = "multifile_ownership_checking/iteration.rs"]
mod iteration;
#[path = "multifile_ownership_checking/lambda_drop.rs"]
mod lambda_drop;
#[path = "multifile_ownership_checking/non_null.rs"]
mod non_null;
#[path = "multifile_ownership_checking/pending_lifetime.rs"]
mod pending_lifetime;
#[path = "multifile_ownership_checking/provenance_contracts.rs"]
mod provenance_contracts;
#[path = "multifile_ownership_checking/receivers.rs"]
mod receivers;
