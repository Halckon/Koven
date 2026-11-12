//! Valid ownership neighbors do not inherit the single invariant-loop-use capability.
use super::*;
use crate::ssa::LoweringErrorKind;
use lang_frontend::name_resolution::{index_compilation_unit, resolve_compilation_unit_names};
use lang_frontend::ownership_checking::{
    check_compilation_unit_ownership, legacy_ownership_eq, with_loan_observation,
};
use lang_frontend::type_checking::check_compilation_unit_types;

#[test]
fn ordinary_invariant_outer_while_neighbors_stay_unsupported() {
    for (name, body) in [
        (
            "two-uses",
            "borrow val item = view(source); while (flag) { consume(item); consume(item) }",
        ),
        (
            "wrong-consumer-root",
            "borrow val item = view(source); while (flag) { consume(source) }",
        ),
        (
            "call-condition",
            "borrow val item = view(source); while (predicate(flag)) { consume(item) }",
        ),
        (
            "break",
            "borrow val item = view(source); while (flag) { consume(item); break }",
        ),
        (
            "continue",
            "borrow val item = view(source); while (flag) { consume(item); continue }",
        ),
        (
            "return",
            "borrow val item = view(source); while (flag) { consume(item); return }",
        ),
        (
            "extra-statement",
            "borrow val item = view(source); consume(source); while (flag) { consume(item) }",
        ),
        (
            "two-bindings",
            "borrow val item = view(source); borrow val sibling = view(source); while (flag) { consume(item) }",
        ),
    ] {
        let text = format!(
            "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {{}}\nfun predicate(flag: Boolean): Boolean = flag\nfun run(source: String, flag: Boolean): Unit {{ {body} }}"
        );
        let mut sources = SourceMap::new();
        let (source, parsed) =
            unit_lower_test_support::parsed(&mut sources, "invariant-neighbor.ko", &text);
        let (environment, types) = standard_environments();
        for input in [
            SourceUnitInput::new("root", "invariant-neighbor.ko", source, &parsed),
            SourceUnitInput::memory_document(source, &parsed),
        ] {
            let inputs = [input];
            let index = index_compilation_unit(&sources, &inputs).unwrap();
            let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
                .unwrap()
                .validate()
                .unwrap();
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types)
                .unwrap()
                .validate()
                .unwrap();
            let on = with_loan_observation(true, || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap()
            });
            let off = with_loan_observation(false, || {
                check_compilation_unit_ownership(&sources, &inputs, &names, &types, &typed).unwrap()
            });
            assert!(legacy_ownership_eq(&on, &off), "{name}");
            let on = on.validate().unwrap();
            let off = off.validate().unwrap();
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
                    .unwrap_or_else(|| panic!("{name}: unsupported neighbor admitted"));
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
