//! SPEC-0300: real source owner facts and normal-exhaustion SSA transport.
use super::{analyze, lower_scalar_file};
use lang_frontend::ownership_checking::{
    CleanupOwnerValue, DropPoint, DropTarget, IterationPhiBoundary, IterationPhiIncomingKind,
};

const ORIGINAL: &str =
    include_str!("../../tests/fixtures/owned-null-cleanup-0296/single-loop-boundary.ko");
const NORMAL: &str = include_str!("../../tests/fixtures/map-loop-owner-0300/normal-exhaustion.ko");

#[test]
fn single_map_loop_original_remove_uses_published_header_owner() {
    let analysis = analyze(ORIGINAL);
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    let guarded = analysis
        .owned
        .drops()
        .iter()
        .find(|fact| fact.condition().is_some())
        .expect("the original source publishes a guarded Map owner");
    let DropTarget::Named(symbol) = guarded.target() else {
        panic!("the Map is a named owner")
    };
    let Some(CleanupOwnerValue::IterationPhi {
        statement,
        symbol: owner_symbol,
        boundary,
        ..
    }) = analysis
        .owned
        .cleanup_conditions()
        .owner_value(guarded.owner().unwrap())
    else {
        panic!("the drop is tied to an iteration owner")
    };
    assert_eq!(*owner_symbol, symbol);
    assert_eq!(*boundary, IterationPhiBoundary::Header);
    assert_eq!(guarded.point(), DropPoint::LoopExit(*statement));
    let plan = analysis.owned.iteration(*statement).unwrap();
    for kind in [
        IterationPhiIncomingKind::Entry,
        IterationPhiIncomingKind::Fallthrough,
        IterationPhiIncomingKind::Exhaustion,
    ] {
        assert!(
            plan.closure_phi_incomings()
                .iter()
                .any(|edge| edge.kind() == kind)
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
            "normal-exhaustion Map carrier: {error:?}; drops={:?}; incoming={:?}",
            analysis.owned.drops(),
            plan.closure_phi_incomings()
        )
    });
    crate::llvm::render_verified_program(&program).unwrap();
}

#[test]
fn unit_map_loop_original_source_is_an_independent_control() {
    assert_unit_lowers(ORIGINAL);
}

#[test]
fn single_map_loop_normal_exhaustion_preserves_dead_and_live_owners() {
    let analysis = analyze(NORMAL);
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
    let program = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    )
    .unwrap();
    crate::llvm::render_verified_program(&program).unwrap();
}

#[test]
fn unit_map_loop_normal_source_is_an_independent_control() {
    assert_unit_lowers(NORMAL);
}

fn assert_unit_lowers(source: &str) {
    use crate::ssa::{
        unit_lower::lower_scalar_unit_with_entry,
        unit_lower_test_support::{analyze, declaration, parsed},
    };
    use lang_frontend::{
        name_resolution::SourceUnitInput, source::SourceMap, type_checking::standard_environments,
    };
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "p/main.ko", &format!("package p\n{source}"));
    let inputs = [SourceUnitInput::new("root", "p/main.ko", source, &file)];
    let (environment, types) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &environment, &types);
    let (program, entry) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        declaration(&names, "p", "main"),
    )
    .unwrap();
    crate::llvm::render_verified_program_with_entry(&program, entry).unwrap();
}
