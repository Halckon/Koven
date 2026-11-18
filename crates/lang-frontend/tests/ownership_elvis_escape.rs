//! Escaping Elvis fallback tails preserve closure rejection and recovery contracts.
#[path = "support/borrow_rejection_unit.rs"]
mod borrow_rejection_unit;
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;
use lang_frontend::type_checking::standard_environments;

#[test]
fn elvis_fallback_preserves_borrowed_closure_escape_rejection() {
    for fallback in [
        "({ read(xs) })",
        "if (flag) { val f: () -> Unit = { read(xs) }; f } else ({})",
        "when (flag) { true -> { val f: () -> Unit = { read(xs) }; f }; else -> ({}) }",
    ] {
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag: Boolean): () -> Unit {{ val none: Nothing? = null\nreturn none ?: ({fallback}) }}"
        );
        for rooted in [false, true] {
            let (_, owned) =
                borrow_rejection_unit::checked(&text, "elvis.ko", rooted, standard_environments());
            assert_eq!(
                owned
                    .diagnostics()
                    .iter()
                    .filter(|d| d.code().to_string() == "L0137")
                    .count(),
                1,
                "{fallback}/{rooted}: {:?}",
                owned.diagnostics()
            );
            assert!(owned.loans().is_empty());
            assert!(owned.drops().is_empty());
            assert!(owned.validate().is_err());
        }
    }
}

#[test]
fn elvis_fallback_preserves_capture_free_return() {
    let text = "fun run(flag: Boolean): () -> Unit { val none: Nothing? = null\nreturn none ?: (if (flag) { val f: () -> Unit = {}; f } else ({})) }";
    for rooted in [false, true] {
        let (_, owned) =
            borrow_rejection_unit::checked(text, "elvis.ko", rooted, standard_environments());
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        let validated = owned.validate().unwrap();
        assert!(validated.ownership().pointer_callable_returns().is_empty());
        assert!(validated.ownership().closure_callable_returns().is_empty());
    }
}

#[test]
fn elvis_control_fallbacks_reject_in_both_layouts() {
    let mut missing = Vec::new();
    for fallback in [
        "if (flag) { val f: () -> Unit = { read(xs) }; f } else ({})",
        "when (flag) { true -> { val f: () -> Unit = { read(xs) }; f }; else -> ({}) }",
    ] {
        for elvis in [false, true] {
            let value = if elvis {
                format!("none ?: ({fallback})")
            } else {
                fallback.into()
            };
            let text = format!(
                "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag: Boolean): () -> Unit {{ val none: Nothing? = null\nreturn ({value}) }}"
            );
            for rooted in [false, true] {
                let (_, owned) = borrow_rejection_unit::checked(
                    &text,
                    "elvis.ko",
                    rooted,
                    standard_environments(),
                );
                if !owned
                    .diagnostics()
                    .iter()
                    .any(|d| d.code().to_string() == "L0137")
                {
                    missing.push(format!(
                        "{fallback}/elvis={elvis}/rooted={rooted}: {:?}",
                        owned.diagnostics()
                    ));
                }
            }
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

#[test]
fn elvis_escape_checks_preserve_owned_and_unreachable_tails() {
    for (index, text) in [
        "fun empty(): Unit {}\nfun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flag: Boolean): () -> Unit { val none: Nothing? = null\nreturn none ?: (if (flag) { val f: () -> Unit = move { read(xs) }; f } else ({})) }",
        "fun empty(): Unit {}\nfun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flag: Boolean): () -> Unit { val none: Nothing? = null\nreturn none ?: (if (flag) { return empty; val f: () -> Unit = { read(xs) }; f } else ({})) }",
    ].into_iter().enumerate() {
        for rooted in [false, true] {
            let (_, owned) =
                borrow_rejection_unit::checked(text, "elvis.ko", rooted, standard_environments());
            assert!(
                owned.diagnostics().is_empty(),
                "{text}/{rooted}: {:?}",
                owned.diagnostics()
            );
            if index == 0 {
                assert!(owned.pointer_callable_returns().is_empty());
                assert!(owned.closure_callable_returns().is_empty());
            }
            assert!(owned.validate().is_ok());
        }
    }
}
