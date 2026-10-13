#[path = "lower_frontend_map_break_structure_tests.rs"]
mod structure;
// SPEC-0310: ordinary source, immutable frontend facts and unmodified SSA/LLVM.
use super::{analyze, lower_scalar_file};
use lang_frontend::ownership_checking::{
    CleanupOwnerValue, IterationExitKind, IterationPhiBoundary, IterationPhiIncomingKind,
};

fn source(live: bool) -> String {
    format!(
        "fun main(): Unit {{ var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2)) {{ cache.put(key, \"value\".clone()); if (key == 1) {{ break }} }}; {} }}",
        if live { "cache.contains(1)" } else { "" }
    )
}

fn check(live: bool) {
    let text = source(live);
    let analysis = analyze(&text);
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    let plan = &analysis.owned.iterations()[0];
    let exit = plan
        .exits()
        .iter()
        .find(|exit| matches!(exit.kind(), IterationExitKind::Break(_)))
        .unwrap();
    let IterationExitKind::Break(expression) = exit.kind() else {
        unreachable!()
    };
    let incoming = plan
        .closure_phi_incomings()
        .iter()
        .find(|edge| edge.kind() == IterationPhiIncomingKind::Break(expression))
        .unwrap();
    assert_eq!(incoming.point(), exit.point());
    assert_eq!(incoming.condition(), exit.condition());
    assert_eq!(incoming.boundary(), IterationPhiBoundary::Exit);
    let header = plan
        .closure_phis()
        .iter()
        .find(|phi| phi.boundary() == IterationPhiBoundary::Header)
        .unwrap();
    assert!(matches!(
        analysis
            .owned
            .cleanup_conditions()
            .owner_value(header.owner()),
        Some(CleanupOwnerValue::IterationPhi { .. })
    ));
    for edge in plan
        .closure_phi_incomings()
        .iter()
        .filter(|edge| edge.boundary() == IterationPhiBoundary::Exit)
    {
        let binding = &edge.bindings()[0];
        assert_eq!(binding.values().len(), usize::from(live));
        if live {
            assert_eq!(binding.values()[0].source(), header.owner());
        }
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
            "{error:?}; drops={:?}; plan={plan:?}; conditions={:?}",
            analysis.owned.drops(),
            analysis.owned.cleanup_conditions()
        )
    });
    crate::llvm::render_verified_program(&program).unwrap();
    structure::assert_exit_structure(
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
fn single_map_break_live_source_red() {
    check(true);
}

#[test]
fn single_map_break_dead_source_red() {
    check(false);
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
    if live {
        let (program, entry) = result
            .unwrap_or_else(|error| panic!("unit constants={constants}, live={live}: {error:?}"));
        crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
    } else {
        let error = result
            .err()
            .expect("unit dead Map break remains an independent boundary");
        assert_eq!(error.kind, crate::ssa::LoweringErrorKind::UnsupportedNode);
        let span = error.span.unwrap();
        assert_eq!(span.source_id(), source_id);
        assert!(sources.slice(span).unwrap().starts_with("for (key in"));
    }
}

#[test]
fn unit_map_break_basic_live_is_independent() {
    assert_unit_boundary(true, false);
}
#[test]
fn unit_map_break_basic_dead_remains_explicit() {
    assert_unit_boundary(false, false);
}
#[test]
fn unit_map_break_constant_live_is_independent() {
    assert_unit_boundary(true, true);
}
#[test]
fn unit_map_break_constant_dead_remains_explicit() {
    assert_unit_boundary(false, true);
}

#[test]
fn single_map_break_unconditional_and_multiple_exits_remain_explicit() {
    for body in ["break", "if (key == 1) { break }; if (key == 2) { break }"] {
        let text = source(true).replace("if (key == 1) { break }", body);
        super::assert_guarded_boundary(&text);
    }
}

#[test]
fn single_map_break_matrix_has_real_ssa_and_separate_owner_drops() {
    for (text, counts) in [
        (
            include_str!("../../tests/fixtures/map-conditional-break-0310/live-dead.ko"),
            vec![
                ("removeDead", 2),
                ("removeLive", 1),
                ("putDead", 2),
                ("putLive", 1),
            ],
        ),
        (
            include_str!("../../tests/fixtures/map-conditional-break-0310/distinct-owners.ko"),
            vec![("parameterOwner", 1), ("separateOwners", 2)],
        ),
    ] {
        let analysis = analyze(text);
        assert!(analysis.parsed.diagnostics().is_empty());
        assert!(analysis.names.diagnostics().is_empty());
        assert!(analysis.typed.diagnostics().is_empty());
        assert!(analysis.owned.diagnostics().is_empty());
        let program = lower_scalar_file(
            &analysis.sources,
            &analysis.parsed,
            &analysis.names,
            &analysis.typed,
            &analysis.owned,
        )
        .unwrap();
        crate::llvm::render_verified_program(&program).unwrap();
        super::assert_map_drop_counts(&program, &counts);
    }
}
