//! Direct nullable control extraction must retain the borrowed-closure escape check.
#[path = "support/borrow_rejection_unit.rs"]
mod borrow_rejection_unit;
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;
use lang_frontend::type_checking::standard_environments;

#[test]
fn nonnull_shared_closure_return_preserves_escape_rejection() {
    let mut missing = Vec::new();
    for delivery in [
        "return f",
        "return (if (flag) f else null)!!",
        "return ((when (flag) { true -> f; else -> null }))!!",
        "return (if (flag) { val local: () -> Unit = { read(xs) }; local } else null)!!",
        "return (if (flag) { val local: () -> Unit = { read(xs) }; val wrapped: () -> Unit = move { local() }; wrapped } else null)!!",
        "return (if (flag) { var local: () -> Unit = { read(xs) }; val wrapped: () -> Unit = move { local() }; local = {}; wrapped } else null)!!",
    ] {
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag: Boolean): () -> Unit {{ val f: () -> Unit = {{ read(xs) }}; {delivery} }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "nonnull.ko",
                rooted,
                standard_environments(),
            );
            if !owned
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0137")
            {
                missing.push(format!("{delivery}/{rooted}: {:?}", owned.diagnostics()));
            }
            assert!(owned.loans().is_empty());
            assert!(owned.drops().is_empty());
            assert!(owned.validate().is_err());
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

#[test]
fn nonnull_callable_preserves_capture_free_and_exclusive_boundaries() {
    for (text, expected) in [
        (
            "fun run(flag: Boolean): () -> Unit { val f: () -> Unit = {}; val optional = if (flag) f else null; return optional!! }",
            vec![],
        ),
        (
            "fun run(flag: Boolean): () -> Unit { var n = 1; val f: () -> Unit = { n = 2 }; val optional = if (flag) f else null; return optional!! }",
            vec!["L0164"],
        ),
    ] {
        for rooted in [false, true] {
            let (_, owned) =
                borrow_rejection_unit::checked(text, "nonnull.ko", rooted, standard_environments());
            assert_eq!(
                owned
                    .diagnostics()
                    .iter()
                    .map(|d| d.code().to_string())
                    .collect::<Vec<_>>(),
                expected,
                "{text}/{rooted}"
            );
        }
    }
}

#[test]
fn direct_nonnull_control_keeps_owned_and_capture_free_results() {
    for text in [
        "fun run(flag: Boolean): () -> Unit { return (if (flag) ({}) else null)!! }",
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flag: Boolean): () -> Unit { return (if (flag) { val owned: () -> Unit = move { read(xs) }; owned } else null)!! }",
        "fun run(flag: Boolean): Unit { var n = 1; val f: () -> Int = { n }; val optional = if (flag) f else null; n = 3 }",
    ] {
        for rooted in [false, true] {
            let (_, owned) =
                borrow_rejection_unit::checked(text, "nonnull.ko", rooted, standard_environments());
            assert!(
                owned.diagnostics().is_empty(),
                "{text}/{rooted}: {:?}",
                owned.diagnostics()
            );
            assert!(owned.validate().is_ok());
        }
    }
}

#[test]
fn direct_nonnull_control_preserves_exclusive_diagnostic_priority() {
    for operand in [
        "if (flag) f else null",
        "when (flag) { true -> f; else -> null }",
    ] {
        let text = format!(
            "fun run(flag: Boolean): () -> Unit {{ var n = 1; val f: () -> Unit = {{ n = 2 }}; return (({operand}))!! }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "nonnull.ko",
                rooted,
                standard_environments(),
            );
            assert_eq!(
                owned
                    .diagnostics()
                    .iter()
                    .map(|d| d.code().to_string())
                    .collect::<Vec<_>>(),
                ["L0164"],
                "{text}/{rooted}"
            );
            assert!(owned.loans().is_empty());
            assert!(owned.drops().is_empty());
            assert!(owned.validate().is_err());
        }
    }
}
