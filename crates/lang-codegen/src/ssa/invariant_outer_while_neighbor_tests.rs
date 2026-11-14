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
            "mixed-root-first",
            "borrow val item = view(source); while (flag) { consume(source); consume(item); consume(item) }",
        ),
        (
            "mixed-root-middle",
            "borrow val item = view(source); while (flag) { consume(item); consume(source); consume(item) }",
        ),
        (
            "mixed-root-last",
            "borrow val item = view(source); while (flag) { consume(item); consume(item); consume(source) }",
        ),
        (
            "empty-body",
            "borrow val item = view(source); while (flag) {}",
        ),
        (
            "nested-block",
            "borrow val item = view(source); while (flag) { { consume(item) } }",
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
            "conditional-break-call-condition",
            "borrow val item = view(source); while (flag) { consume(item); if (predicate(flag)) { break } }",
        ),
        (
            "conditional-continue-call-condition",
            "borrow val item = view(source); while (flag) { consume(item); if (predicate(flag)) { continue } }",
        ),
        (
            "continue-without-consumer",
            "borrow val item = view(source); while (flag) { continue }",
        ),
        (
            "nested-continue",
            "borrow val item = view(source); while (flag) { consume(item); { continue } }",
        ),
        (
            "break-without-consumer",
            "borrow val item = view(source); while (flag) { break }",
        ),
        (
            "nested-break",
            "borrow val item = view(source); while (flag) { consume(item); { break } }",
        ),
        (
            "conditional-break-else",
            "borrow val item = view(source); while (flag) { consume(item); if (flag) { break } else {} }",
        ),
        (
            "conditional-break-extra-consumer",
            "borrow val item = view(source); while (flag) { consume(item); if (flag) { consume(item); break } }",
        ),
        (
            "conditional-break-nested-if",
            "borrow val item = view(source); while (flag) { consume(item); if (flag) { if (flag) { break } } }",
        ),
        (
            "conditional-return",
            "borrow val item = view(source); while (flag) { consume(item); if (flag) { return } }",
        ),
        (
            "return-without-consumer",
            "borrow val item = view(source); while (flag) { return }",
        ),
        (
            "nested-return",
            "borrow val item = view(source); while (flag) { consume(item); { return } }",
        ),
        (
            "return-call-value",
            "borrow val item = view(source); while (flag) { consume(item); return consume(item) }",
        ),
        (
            "conditional-continue-else",
            "borrow val item = view(source); while (flag) { consume(item); if (flag) { continue } else {} }",
        ),
        (
            "conditional-continue-extra-consumer",
            "borrow val item = view(source); while (flag) { consume(item); if (flag) { consume(item); continue } }",
        ),
        (
            "conditional-continue-nested-if",
            "borrow val item = view(source); while (flag) { consume(item); if (flag) { if (flag) { continue } } }",
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
            let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
            if name == "return-call-value" {
                assert!(
                    typed
                        .diagnostics()
                        .iter()
                        .any(|diagnostic| diagnostic.code().to_string() == "L0087")
                );
                assert!(
                    typed.validate().is_err(),
                    "a Unit callable requires bare return; do not pass invalid source to CG"
                );
                continue;
            }
            let typed = typed.validate().unwrap();
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
