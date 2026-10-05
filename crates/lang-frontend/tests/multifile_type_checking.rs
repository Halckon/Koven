//! SPEC-0197 production compilation-unit body driver integration tests.

use lang_frontend::{
    ast::{ExpressionId, StatementId, TypeRefId},
    diagnostic::{Diagnostic, DiagnosticDetail},
    lexer::lex,
    name_resolution::{
        DeclarationId, NameEnvironment, Namespace, SourceUnitId, SourceUnitInput, SymbolKind,
        UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames, index_compilation_unit,
        resolve_compilation_unit_names,
    },
    parser::{AssignmentOperator, Expression, ParsedFile, Statement, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        BuiltinType, CompilationUnitTypes, ConstValue, ContainerConstructionKind, DeferredReason,
        DestructuringMode, EnvironmentFunction, EnvironmentFunctionEffect, EnvironmentParameter,
        EnvironmentType, ExpressionCategory, IntrinsicTypeConstructor, ParameterMode,
        RcOperationKind, SequentialContainerKind, TypeEnvironment, UnitAggregateProjectionKind,
        UnitAggregateProjectionReceiver, UnitCallReceiverOrigin, UnitCallTarget,
        UnitConstructionTarget, UnitExpressionId, UnitStatementId, UnitTypeKind, UnitTypeRefId,
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

fn declaration(names: &ValidatedCompilationUnitNames, name: &str) -> DeclarationId {
    names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|declaration| declaration.name() == name)
        .expect("declaration exists")
        .id()
}

fn expression_with_text(sources: &SourceMap, file: &ParsedFile, text: &str) -> ExpressionId {
    file.ast()
        .expressions()
        .iter()
        .find(|(_, node)| sources.slice(node.span()) == Ok(text))
        .expect("expression text exists")
        .0
}

fn expressions_with_text(sources: &SourceMap, file: &ParsedFile, text: &str) -> Vec<ExpressionId> {
    file.ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| (sources.slice(node.span()) == Ok(text)).then_some(id))
        .collect()
}

fn type_refs_with_text(sources: &SourceMap, file: &ParsedFile, text: &str) -> Vec<TypeRefId> {
    file.ast()
        .type_refs()
        .iter()
        .filter_map(|(id, node)| (sources.slice(node.span()) == Ok(text)).then_some(id))
        .collect()
}

fn when_expressions(file: &ParsedFile) -> Vec<ExpressionId> {
    file.ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| matches!(node.payload(), Expression::When { .. }).then_some(id))
        .collect()
}

fn symbol_named(
    typed: &CompilationUnitTypes,
    names: &ValidatedCompilationUnitNames,
    source: SourceUnitId,
    name: &str,
) -> UnitSymbolId {
    let symbol = names.names().source_units()[source.index()]
        .resolution()
        .symbols()
        .iter()
        .find(|symbol| symbol.name() == name)
        .expect("source-local symbol exists")
        .id();
    typed
        .body_symbol_types()
        .keys()
        .find(|candidate| candidate.source_unit() == source && candidate.symbol() == symbol)
        .copied()
        .expect("source-local body symbol has a typed fact")
}

#[path = "multifile_type_checking/assignments.rs"]
mod assignments;
#[path = "multifile_type_checking/baseline_regressions.rs"]
mod baseline_regressions;
#[path = "multifile_type_checking/body_recovery.rs"]
mod body_recovery;
#[path = "multifile_type_checking/control_flow.rs"]
mod control_flow;
#[path = "multifile_type_checking/delegation.rs"]
mod delegation;
#[path = "multifile_type_checking/destructuring.rs"]
mod destructuring;
#[path = "multifile_type_checking/dispatch_receivers.rs"]
mod dispatch_receivers;
#[path = "multifile_type_checking/expression_tails.rs"]
mod expression_tails;
#[path = "multifile_type_checking/external_calls.rs"]
mod external_calls;
#[path = "multifile_type_checking/generics.rs"]
mod generics;
#[path = "multifile_type_checking/initializers.rs"]
mod initializers;
#[path = "multifile_type_checking/intrinsic_box_rc.rs"]
mod intrinsic_box_rc;
#[path = "multifile_type_checking/intrinsic_containers.rs"]
mod intrinsic_containers;
#[path = "multifile_type_checking/iteration.rs"]
mod iteration;
#[path = "multifile_type_checking/lambdas.rs"]
mod lambdas;
#[path = "multifile_type_checking/members.rs"]
mod members;
#[path = "multifile_type_checking/nullable_flow.rs"]
mod nullable_flow;
#[path = "multifile_type_checking/runtime_layout.rs"]
mod runtime_layout;
#[path = "multifile_type_checking/source_construction.rs"]
mod source_construction;

#[path = "multifile_type_checking/container_size.rs"]
mod container_size;
