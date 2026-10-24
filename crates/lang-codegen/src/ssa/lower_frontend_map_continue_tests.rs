#[path = "lower_frontend_map_continue_structure_tests.rs"]
mod structure;
// SPEC-0311: ordinary frontend facts, verified SSA and unmodified LLVM.
use super::{analyze, lower_scalar_file};
use lang_frontend::ownership_checking::{
    CleanupOwnerValue, IterationExitKind, IterationPhiBoundary, IterationPhiIncomingKind,
};

pub(super) const LIVE_DEAD: &str =
    include_str!("../../tests/fixtures/map-conditional-continue-0311/live-dead.ko");
pub(super) const DISTINCT: &str =
    include_str!("../../tests/fixtures/map-conditional-continue-0311/distinct-owners.ko");
pub(super) const PROVIDERS: &str = include_str!(
    "../../tests/fixtures/map-conditional-continue-0311/borrowed-temporary-providers.ko"
);

// Keep two distinct updates: each re-entry must carry its own latest owner.
pub(super) fn source(live: bool) -> String {
    format!(
        "fun main(): Unit {{ var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2, 3)) {{ cache.put(key, \"head\".clone()); if (key == 1) {{ continue }}; cache.put(9, \"tail\".clone()) }}; {} }}",
        if live { "cache.contains(9)" } else { "" }
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
    let exits = plan
        .exits()
        .iter()
        .filter(|exit| matches!(exit.kind(), IterationExitKind::Continue(_)))
        .collect::<Vec<_>>();
    assert_eq!(exits.len(), 1);
    let exit = exits[0];
    let IterationExitKind::Continue(expression) = exit.kind() else {
        unreachable!()
    };
    let incoming = plan
        .closure_phi_incomings()
        .iter()
        .find(|edge| edge.kind() == IterationPhiIncomingKind::Continue(expression))
        .unwrap();
    assert_eq!(incoming.point(), exit.point());
    assert_eq!(incoming.condition(), exit.condition());
    assert_eq!(incoming.boundary(), IterationPhiBoundary::Header);
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
    for kind in [
        IterationPhiIncomingKind::Continue(expression),
        IterationPhiIncomingKind::Fallthrough,
    ] {
        let edge = plan
            .closure_phi_incomings()
            .iter()
            .find(|edge| edge.kind() == kind)
            .unwrap();
        assert_eq!(edge.boundary(), IterationPhiBoundary::Header);
        let binding = edge
            .bindings()
            .iter()
            .find(|binding| binding.target() == header.owner())
            .unwrap();
        assert_eq!(binding.values().len(), 1);
        assert_eq!(binding.values()[0].source(), header.owner());
    }
    let exhaustion = plan
        .closure_phi_incomings()
        .iter()
        .find(|edge| edge.kind() == IterationPhiIncomingKind::Exhaustion)
        .unwrap();
    assert_eq!(exhaustion.boundary(), IterationPhiBoundary::Exit);
    let binding = &exhaustion.bindings()[0];
    assert_eq!(binding.values().len(), usize::from(live));
    if live {
        assert_eq!(binding.values()[0].source(), header.owner());
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
            "continue live={live}: {error:?}; drops={:?}; plan={plan:?}",
            analysis.owned.drops()
        )
    });
    crate::llvm::render_verified_program(&program).unwrap();
    structure::assert_continue_structure(
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
fn single_map_continue_live_source_red() {
    check(true);
}

#[test]
fn single_map_continue_dead_source_red() {
    check(false);
}

// These four independent controls initially require success. Their actual baseline
// must be observed; SPEC-0310's unit break boundary says nothing about continue.
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
fn unit_map_continue_basic_live_source_red() {
    assert_unit_lowers(true, false);
}

#[test]
fn unit_map_continue_basic_dead_source_red() {
    assert_unit_lowers(false, false);
}

#[test]
fn unit_map_continue_constant_live_source_red() {
    assert_unit_lowers(true, true);
}

#[test]
fn unit_map_continue_constant_dead_source_red() {
    assert_unit_lowers(false, true);
}

fn assert_source_lowers(text: &str) {
    use lang_frontend::name_resolution::SymbolKind;

    let (program, analysis) = super::lower_map_unit(text, "main");
    if text == DISTINCT {
        let declaration = analysis.declaration("separateOwners", SymbolKind::Function);
        structure::assert_distinct_owner_backedges(
            &program,
            analysis.declaration_span(declaration),
        );
    }
}

#[test]
fn single_map_continue_live_dead_source_matrix() {
    assert_source_lowers(LIVE_DEAD);
}

#[test]
fn single_map_continue_own_seed_and_same_type_owners() {
    assert_source_lowers(DISTINCT);
}

#[test]
fn single_map_continue_borrowed_and_temporary_providers() {
    assert_source_lowers(PROVIDERS);
}

#[path = "lower_frontend_map_continue_source_tests.rs"]
mod source_presence;
