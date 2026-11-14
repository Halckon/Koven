//! Local-owner neighbors remain outside the new straight-line dynamic-loop capability.
use super::*;
use crate::ssa::LoweringErrorKind;
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};
#[test]
fn ordinary_local_loop_lineage_neighbors_remain_unsupported() {
    for (name, owner, condition, body) in [
        ("call-owner", "val source=make()", "flag", "consume(item)"),
        (
            "mutable-owner",
            "var source=\"value\"",
            "flag",
            "consume(item)",
        ),
        (
            "interpolated-owner",
            "val source=\"${1}\"",
            "flag",
            "consume(item)",
        ),
        (
            "constant-false",
            "val source=\"value\"",
            "false",
            "consume(item)",
        ),
        (
            "constant-true",
            "val source=\"value\"",
            "true",
            "consume(item)",
        ),
        (
            "break",
            "val source=\"value\"",
            "flag",
            "consume(item);break",
        ),
        (
            "continue",
            "val source=\"value\"",
            "flag",
            "consume(item);continue",
        ),
        (
            "return",
            "val source=\"value\"",
            "flag",
            "consume(item);return",
        ),
        (
            "nested-consumer",
            "val source=\"value\"",
            "flag",
            "{consume(item)}",
        ),
        (
            "nested-loop",
            "val source=\"value\"",
            "flag",
            "while(flag){consume(item);break}",
        ),
        (
            "wrong-root",
            "val source=\"value\"",
            "flag",
            "consume(source)",
        ),
        ("empty-body", "val source=\"value\"", "flag", ""),
    ] {
        let text = format!(
            "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {{}}\nfun make(): String = \"value\"\nfun run(flag: Boolean): Unit {{{owner};borrow val item=view(source);while({condition}){{{body}}}}}"
        );
        let mut sources = SourceMap::new();
        let (source, parsed) =
            unit_lower_test_support::parsed(&mut sources, "local-loop-neighbor.ko", &text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "local-loop-neighbor.ko", source, &parsed),
            SourceUnitInput::memory_document(source, &parsed),
        ] {
            let inputs = [input];
            let (names, typed, on) =
                unit_lower_test_support::analyze(&sources, &inputs, &environment, &types);
            let off = with_loan_observation(false, || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed)
                    .unwrap()
                    .validate()
                    .unwrap()
            });
            assert!(
                legacy_ownership_eq(on.ownership(), off.ownership()),
                "{name}"
            );
            assert!(
                on.ownership()
                    .loan_action_observations()
                    .iter()
                    .all(|c| c.loop_invariant_lineages().is_empty()),
                "{name}"
            );
            let entry = names
                .names()
                .index()
                .declarations()
                .iter()
                .find(|d| d.name() == "run")
                .unwrap()
                .id();
            let mut previous = None;
            for owned in [&on, &off] {
                let (result, verified) = crate::ssa::unit_lower::observe_final_verifier(|| {
                    crate::ssa::unit_lower::lower_scalar_unit_with_entry(
                        &sources, &inputs, &names, &types, &typed, owned, entry,
                    )
                });
                assert!(verified.is_empty(), "{name}");
                let error = result
                    .err()
                    .unwrap_or_else(|| panic!("{name}: neighbor admitted"));
                assert_eq!(error.kind, LoweringErrorKind::UnsupportedNode, "{name}");
                let span = error.span.unwrap();
                assert_eq!(span.source_id(), source);
                assert!(!text[span.start()..span.end()].is_empty());
                let summary = format!("{error:?}");
                if let Some(previous) = previous.as_ref() {
                    assert_eq!(&summary, previous, "{name}");
                }
                previous = Some(summary);
            }
        }
    }
}
