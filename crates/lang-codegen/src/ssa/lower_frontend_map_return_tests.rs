#[path = "lower_frontend_map_return_structure_tests.rs"]
mod structure;
// SPEC-0313: ordinary source conditional return, independent single/unit baseline.
use super::{analyze, lower_scalar_file};
use lang_frontend::ownership_checking::{IterationExitKind, IterationPhiIncomingKind};

fn source(live: bool) -> String {
    format!(
        "fun main(): Unit {{ var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2, 3)) {{ cache.put(key, \"head\".clone()); if (key == 1) {{ return }}; cache.put(9, \"tail\".clone()) }}; {} }}",
        if live { "cache.contains(9)" } else { "" }
    )
}

fn check(live: bool) {
    let text = source(live);
    let analysis = analyze(&text);
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(
        analysis.typed.diagnostics().is_empty(),
        "{:?}",
        analysis.typed.diagnostics()
    );
    assert!(
        analysis.owned.diagnostics().is_empty(),
        "{:?}",
        analysis.owned.diagnostics()
    );
    let plan = &analysis.owned.iterations()[0];
    assert_eq!(plan.exits().len(), 3);
    assert_eq!(
        plan.exits()
            .iter()
            .filter(|e| matches!(e.kind(), IterationExitKind::Return(_)))
            .count(),
        1
    );
    assert_eq!(plan.closure_phi_incomings().len(), 3);
    for kind in [
        IterationPhiIncomingKind::Entry,
        IterationPhiIncomingKind::Fallthrough,
        IterationPhiIncomingKind::Exhaustion,
    ] {
        assert_eq!(
            plan.closure_phi_incomings()
                .iter()
                .filter(|e| e.kind() == kind)
                .count(),
            1
        );
    }
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .unwrap_or_else(|error| {
        panic!(
            "return live={live}: {error:?}; drops={:?}; plan={plan:?}; conditions={:?}",
            analysis.owned.drops(),
            analysis.owned.cleanup_conditions()
        )
    });
    crate::llvm::render_verified_program(&program).unwrap();
    let expression = plan
        .exits()
        .iter()
        .find_map(|exit| match exit.kind() {
            IterationExitKind::Return(expression) => Some(expression),
            _ => None,
        })
        .unwrap();
    structure::assert_return_structure(
        &program,
        analysis
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .unwrap()
            .span(),
        analysis
            .parsed
            .ast()
            .expressions()
            .get(plan.descriptor().source())
            .unwrap()
            .span(),
        live,
    );
}

#[test]
fn single_map_return_live_source_red() {
    check(true);
}
#[test]
fn single_map_return_dead_source_red() {
    check(false);
}

fn assert_unit_lowers(live: bool, constants: bool) {
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
    let text = format!("package p\n{}", source(live));
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
fn unit_map_return_basic_live_source_red() {
    assert_unit_lowers(true, false);
}

#[test]
fn unit_map_return_basic_dead_source_red() {
    assert_unit_lowers(false, false);
}

#[test]
fn unit_map_return_constant_live_source_red() {
    assert_unit_lowers(true, true);
}

#[test]
fn unit_map_return_constant_dead_source_red() {
    assert_unit_lowers(false, true);
}

#[path = "lower_frontend_map_return_controls_tests.rs"]
mod ordinary_controls;

#[path = "lower_frontend_map_return_matrix_tests.rs"]
mod source_matrix;

#[path = "lower_frontend_map_operand_last_use_tests.rs"]
mod operand_last_use;
