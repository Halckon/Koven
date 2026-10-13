//! SPEC-0310: ordinary source, immutable frontend facts and unmodified SSA/LLVM.
use super::{analyze, assert_unit_lowers, lower_scalar_file};
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
}

#[test]
fn single_map_break_live_source_red() {
    check(true);
}

#[test]
fn single_map_break_dead_source_red() {
    check(false);
}

#[test]
fn unit_map_break_same_sources_are_independent_controls() {
    for live in [true, false] {
        assert_unit_lowers(&source(live));
    }
}
