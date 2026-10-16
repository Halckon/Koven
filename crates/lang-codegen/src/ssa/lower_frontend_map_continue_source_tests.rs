//! Original SPEC-0311 source probes, now accepted after SPEC-0312 actual-main integration.
use super::{analyze, lower_scalar_file};
use lang_frontend::ownership_checking::{
    CleanupOwnerValue, DropPoint, DropTarget, IterationPhiBoundary, LoanTarget,
};

fn check(text: &str, source_gap: Option<IterationPhiBoundary>) {
    let analysis = analyze(text);
    assert!(analysis.parsed.diagnostics().is_empty());
    assert!(analysis.names.diagnostics().is_empty());
    assert!(analysis.typed.diagnostics().is_empty());
    assert!(analysis.owned.diagnostics().is_empty());
    let result = lower_scalar_file(
        &analysis.sources,
        &analysis.parsed,
        &analysis.names,
        &analysis.typed,
        &analysis.owned,
    );
    if let Some(boundary) = source_gap {
        let symbol = analysis
            .names
            .symbols()
            .iter()
            .find(|symbol| symbol.name() == "keys")
            .unwrap();
        let fact = analysis
            .owned
            .drops()
            .iter()
            .find(|fact| fact.target() == DropTarget::Named(symbol.id()))
            .unwrap();
        let DropTarget::Named(symbol) = fact.target() else {
            panic!("named source")
        };
        let Some(CleanupOwnerValue::IterationPhi {
            statement,
            symbol: identity,
            boundary: actual,
            ..
        }) = analysis
            .owned
            .cleanup_conditions()
            .owner_value(fact.owner().unwrap())
        else {
            panic!("source IterationPhi")
        };
        assert_eq!(*identity, symbol);
        assert_eq!(*actual, boundary);
        assert!(fact.condition().is_some());
        let plan = analysis.owned.iteration(*statement).unwrap();
        assert!(
            matches!(plan.source(), Some(LoanTarget::Place(place)) if place.is_root() && place.root() == symbol)
        );
        if boundary == IterationPhiBoundary::Header {
            assert_eq!(fact.point(), DropPoint::LoopExit(*statement));
        } else {
            assert_ne!(fact.point(), DropPoint::LoopExit(*statement));
        }
    }
    crate::llvm::render_verified_program(&result.unwrap()).unwrap();
}

#[test]
fn map_continue_source_owned_after_use_without_map_now_lowers() {
    check(
        "fun main(): Unit { val keys = arrayOf(1, 2, 3); for (key in keys) { if (key == 2) { continue } }; keys.size }",
        Some(IterationPhiBoundary::Exit),
    );
}

#[test]
fn map_continue_source_owned_without_after_use_with_map_now_lowers() {
    check(
        "fun main(): Unit { val keys = arrayOf(1, 2, 3); var cache = mutableMapOf<Int, String>(); for (key in keys) { cache.put(key, \"head\".clone()); if (key == 2) { continue }; cache.put(9, \"tail\".clone()) }; cache.contains(9) }",
        Some(IterationPhiBoundary::Header),
    );
}

#[test]
fn map_continue_source_temporary_with_map_lowers() {
    check(
        "fun main(): Unit { var cache = mutableMapOf<Int, String>(); for (key in arrayOf(1, 2, 3)) { cache.put(key, \"head\".clone()); if (key == 2) { continue }; cache.put(9, \"tail\".clone()) }; cache.contains(9) }",
        None,
    );
}

#[test]
fn map_continue_source_original_owned_fixture_now_lowers() {
    check(
        include_str!("../../tests/fixtures/map-conditional-continue-0311/providers-cleanup.ko"),
        Some(IterationPhiBoundary::Exit),
    );
}
