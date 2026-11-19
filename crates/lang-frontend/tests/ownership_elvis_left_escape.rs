//! Escaping Elvis left-hand results must retain borrowed-closure diagnostics.
#[path = "support/borrow_rejection_unit.rs"]
mod borrow_rejection_unit;
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;
use lang_frontend::type_checking::standard_environments;

const BODIES: [&str; 3] = [
    "return (if(flag) { val f: () -> Unit = CAPTURE; f } else null) ?: ({})",
    "return (when(flag) { true -> { val f: () -> Unit = CAPTURE; f }; else -> null }) ?: ({})",
    "val optional = if(flag) { val f: () -> Unit = CAPTURE; f } else null; return optional ?: ({})",
];

#[test]
fn borrowed_left_results_reject_in_both_layouts() {
    let mut failures = Vec::new();
    for body in BODIES {
        let body = body.replace("CAPTURE", "{ read(xs) }");
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag: Boolean): () -> Unit {{ {body} }}"
        );
        for rooted in [false, true] {
            let (_, owned) =
                borrow_rejection_unit::checked(&text, "left.ko", rooted, standard_environments());
            let codes = owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>();
            assert!(owned.loans().is_empty());
            assert!(owned.drops().is_empty());
            let invalid = owned.validate().is_err();
            if codes != ["L0137"] || !invalid {
                failures.push(format!("{body}/{rooted}: {codes:?}, invalid={invalid}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn legal_left_results_remain_allowed() {
    for body in BODIES {
        for capture in ["{}", "move { read(xs) }"] {
            let body = body.replace("CAPTURE", capture);
            let text = format!(
                "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag: Boolean): () -> Unit {{ {body} }}"
            );
            for rooted in [false, true] {
                let (_, owned) = borrow_rejection_unit::checked(
                    &text,
                    "left.ko",
                    rooted,
                    standard_environments(),
                );
                assert!(
                    owned.diagnostics().is_empty(),
                    "{body}/{rooted}: {:?}",
                    owned.diagnostics()
                );
                assert!(owned.validate().is_ok());
            }
        }
    }
}

#[test]
fn left_elvis_preserves_exclusive_priority_and_non_escaping_release() {
    for left in [
        "if(flag) f else null",
        "when(flag){true -> f; else -> null}",
    ] {
        let text = format!(
            "fun run(flag: Boolean): () -> Unit {{ var n = 1; val f: () -> Unit = {{ n = 2 }}; return ({left}) ?: ({{}}) }}"
        );
        for rooted in [false, true] {
            let (_, owned) =
                borrow_rejection_unit::checked(&text, "left.ko", rooted, standard_environments());
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
    for text in [
        "fun run(flag: Boolean) { var n = 1; val f: () -> Int = { n }; val optional = if(flag) f else null; val result = optional ?: ({ 0 }); n = 3 }",
        "fun empty() {}\nfun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flag: Boolean): () -> Unit { return (if(flag) { return empty; val f: () -> Unit = { read(xs) }; f } else null) ?: ({}) }",
    ] {
        for rooted in [false, true] {
            let (_, owned) =
                borrow_rejection_unit::checked(text, "left.ko", rooted, standard_environments());
            assert!(
                owned.diagnostics().is_empty(),
                "{text}/{rooted}: {:?}",
                owned.diagnostics()
            );
            assert!(owned.validate().is_ok());
        }
    }
}
