use super::unit_lower;

use std::collections::{BTreeMap, BTreeSet};

use lang_frontend::{
    lexer::lex,
    name_resolution::{
        DeclarationId, NameEnvironment, SourceUnitInput, ValidatedCompilationUnitNames,
        index_compilation_unit, resolve_compilation_unit_names,
    },
    ownership_checking::{ValidatedCompilationUnitOwnership, check_compilation_unit_ownership},
    parser::{ParsedFile, parse_file},
    source::{SourceId, SourceMap},
    type_checking::{
        BuiltinType, IntrinsicTypeConstructor, TypeEnvironment, UnitCallTarget, UnitCallableTarget,
        UnitTypeKind, ValidatedCompilationUnitTypes, check_compilation_unit_types,
        standard_environments,
    },
};

use super::{
    LoweringErrorKind,
    unit_plan::{
        UnitInstancePlan, UnitRuntimeTypeDemand, plan_unit_instances,
        plan_unit_instances_with_limit, resolve_delegated_dispatch_owner_argument,
        resolve_inherited_dispatch_owner_argument, resolve_nominal_runtime_field_types,
        resolve_unit_call_instance,
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

fn analyze(
    sources: &SourceMap,
    inputs: &[SourceUnitInput<'_>],
    name_environment: &NameEnvironment,
    type_environment: &TypeEnvironment,
) -> (
    ValidatedCompilationUnitNames,
    ValidatedCompilationUnitTypes,
    ValidatedCompilationUnitOwnership,
) {
    let index = index_compilation_unit(sources, inputs).expect("valid unit input");
    let names = resolve_compilation_unit_names(sources, inputs, &index, name_environment)
        .expect("name resolution succeeds internally")
        .validate()
        .expect("valid names");
    let typed = check_compilation_unit_types(sources, inputs, &names, type_environment)
        .expect("type checking succeeds internally")
        .validate()
        .expect("valid types");
    let owned = check_compilation_unit_ownership(sources, inputs, &names, type_environment, &typed)
        .expect("ownership checking succeeds internally")
        .validate()
        .expect("valid ownership");
    (names, typed, owned)
}

fn only_member_call_route(
    parsed: &ParsedFile,
    typed: &ValidatedCompilationUnitTypes,
    owned: &ValidatedCompilationUnitOwnership,
) -> super::unit_plan::ResolvedUnitCallInstance {
    let call = typed
        .types()
        .calls()
        .iter()
        .find(|call| call.receiver().is_some())
        .expect("one member call");
    let target = match call.target() {
        UnitCallTarget::Declaration(declaration) => UnitCallableTarget::Declaration(declaration),
        UnitCallTarget::Symbol(symbol) => UnitCallableTarget::Symbol(symbol),
        _ => panic!("source member call has a static target"),
    };
    let span = parsed
        .ast()
        .expressions()
        .get(call.expression().expression())
        .expect("call expression")
        .span();
    resolve_unit_call_instance(
        typed.types(),
        owned.ownership(),
        target,
        call.instance().type_arguments().to_vec(),
        call.receiver().map(|receiver| receiver.ty()),
        span,
    )
    .expect("member route resolves")
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

fn plan<'a>(
    sources: &SourceMap,
    inputs: &'a [SourceUnitInput<'a>],
    names: &ValidatedCompilationUnitNames,
    type_environment: &TypeEnvironment,
    typed: &ValidatedCompilationUnitTypes,
    owned: &ValidatedCompilationUnitOwnership,
    entry: DeclarationId,
) -> UnitInstancePlan {
    plan_unit_instances(
        sources,
        inputs,
        names,
        type_environment,
        typed,
        owned,
        entry,
    )
    .expect("unit instance plan")
}

mod body_type_budget;
mod canonical_callables;
mod delegation_routes;
mod entry_identity;
mod error_order;
mod generic_containers;
mod instances;
mod layout_demand;
mod owner_recipes;
mod recipe_cycles;
