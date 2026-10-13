//! SPEC-0312: real ordinary source, no Map and no synthetic facts.
use super::{analyze, lower_scalar_file};
use lang_frontend::ownership_checking::{CleanupOwnerValue, IterationPhiBoundary};

fn check(live: bool, factory: &str) {
    let text = format!(
        "fun main(): Unit {{ val keys = {factory}(1, 2, 3); for (key in keys) {{ if (key == 2) {{ continue }} }}; {} }}",
        if live { "keys.size" } else { "" }
    );
    let analysis = analyze(&text);
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    let fact = analysis.owned.drops().iter().find(|fact| {
            matches!(fact.owner().and_then(|owner| analysis.owned.cleanup_conditions().owner_value(owner)),
                Some(CleanupOwnerValue::IterationPhi { boundary, .. })
                if *boundary == if live { IterationPhiBoundary::Exit } else { IterationPhiBoundary::Header })
        }).expect("real source phi cleanup");
    assert!(fact.condition().is_some());
    let result = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    );
    if let Err(error) = &result {
        assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
        assert_eq!(analysis.sources.slice(error.span.unwrap()).unwrap(), "keys");
    }
    let program = result.unwrap_or_else(|error| panic!("{factory} live={live}: {error:?}"));
    crate::llvm::render_verified_program(&program).unwrap();
}

#[test]
fn owned_source_continue_array_live_without_map() {
    check(true, "arrayOf");
}
#[test]
fn owned_source_continue_array_dead_without_map() {
    check(false, "arrayOf");
}
#[test]
fn owned_source_continue_list_live_without_map() {
    check(true, "listOf");
}
#[test]
fn owned_source_continue_list_dead_without_map() {
    check(false, "listOf");
}
#[test]
fn owned_source_continue_mutable_list_live_without_map() {
    check(true, "mutableListOf");
}
#[test]
fn owned_source_continue_mutable_list_dead_without_map() {
    check(false, "mutableListOf");
}
fn assert_unit_boundary(live: bool, constants: bool) {
    use crate::ssa::unit_lower_test_support::{declaration, parsed};
    use lang_frontend::{
        name_resolution::{
            SourceUnitInput, index_compilation_unit, resolve_compilation_unit_names,
        },
        ownership_checking::{
            check_compilation_unit_constant_ownership, check_compilation_unit_ownership,
        },
        source::SourceMap,
        type_checking::{check_compilation_unit_types, standard_environments},
    };
    let text = format!(
        "package p\n{}",
        format!(
            "fun main(): Unit {{ val keys = arrayOf(1, 2, 3); for (key in keys) {{ if (key == 2) {{ continue }} }}; {} }}",
            if live { "keys.size" } else { "" }
        )
    );
    let mut sources = SourceMap::new();
    let (source_id, file) = parsed(&mut sources, "p/main.ko", &text);
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source_id, &file)];
    let (env, types) = standard_environments();
    let index = index_compilation_unit(&sources, &inputs).unwrap();
    let names = resolve_compilation_unit_names(&sources, &inputs, &index, &env)
        .unwrap()
        .validate()
        .unwrap();
    let result = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
    let entry = declaration(&names, "p", "main");
    let result = if constants {
        let typed = result.validate_constants().unwrap();
        let owned =
            check_compilation_unit_constant_ownership(&sources, &inputs, &names, &types, &typed)
                .unwrap()
                .validate()
                .unwrap();
        crate::ssa::unit_lower::constant::lower_constant_unit_with_entry(
            &sources, &inputs, &names, &types, &typed, &owned, entry,
        )
    } else {
        let typed = result.validate().unwrap();
        let owned = check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
            .unwrap()
            .validate()
            .unwrap();
        crate::ssa::unit_lower::lower_scalar_unit_with_entry(
            &sources, &inputs, &names, &types, &typed, &owned, entry,
        )
    };
    let (program, entry) =
        result.unwrap_or_else(|error| panic!("unit constants={constants}, live={live}: {error:?}"));
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
}
#[test]
fn owned_source_continue_unit_basic_dead() {
    assert_unit_boundary(false, false);
}
#[test]
fn owned_source_continue_unit_basic_live() {
    assert_unit_boundary(true, false);
}
#[test]
fn owned_source_continue_unit_constant_dead() {
    assert_unit_boundary(false, true);
}
#[test]
fn owned_source_continue_unit_constant_live() {
    assert_unit_boundary(true, true);
}
