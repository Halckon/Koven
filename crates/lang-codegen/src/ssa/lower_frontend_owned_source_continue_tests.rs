//! SPEC-0312: real ordinary source, no Map and no synthetic facts.
use super::{analyze, lower_scalar_file};
use lang_frontend::ownership_checking::{CleanupOwnerValue, IterationPhiBoundary};

fn check(live: bool) {
    for factory in ["arrayOf", "listOf", "mutableListOf"] {
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
}

#[test]
fn owned_source_continue_live_without_map() {
    check(true);
}

#[test]
fn owned_source_continue_dead_without_map() {
    check(false);
}
