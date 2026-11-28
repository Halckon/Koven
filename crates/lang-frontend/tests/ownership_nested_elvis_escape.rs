//! Escaping Elvis nested results must retain borrowed-closure diagnostics.
#[path = "support/borrow_rejection_unit.rs"]
mod borrow_rejection_unit;
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;
use lang_frontend::type_checking::standard_environments;

const BODIES: [&str; 4] = [
    "val none: Nothing? = null; return (none ?: (if(flag) { val f: () -> Unit = CAPTURE; f } else null)) ?: ({})",
    "val none: Nothing? = null; val optional = if(flag) { val f: () -> Unit = CAPTURE; f } else null; return (none ?: optional) ?: ({})",
    "val none: Nothing? = null; val optional = when(flag) { true -> { val f: () -> Unit = CAPTURE; f }; else -> null }; val selected = none ?: optional; return selected!!",
    "val none: Nothing? = null; val optional = if(flag) { val f: () -> Unit = CAPTURE; f } else null; return (none ?: (none ?: optional))!!",
];

#[test]
fn borrowed_nested_results_reject_in_both_layouts() {
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
fn legal_nested_results_remain_allowed() {
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
fn nested_elvis_preserves_diagnostic_and_release_boundaries() {
    let cases = [
        (
            "fun run(flag:Boolean): () -> Unit { var n=1; val f: () -> Unit={n=2}; val none: Nothing?=null; return (none ?: (if(flag) f else null)) ?: ({}) }",
            Some("L0164"),
        ),
        (
            "fun read(xs: List<Int>) {}\nfun make(): () -> Unit { return ({}) }\nfun run(own xs: List<Int>, flag:Boolean, again:Boolean): () -> Unit { val f: () -> Unit={read(xs)}; val optional=if(flag) f else null; val selected=optional ?: make(); return (if(again) selected else null)!! }",
            Some("L0137"),
        ),
        (
            "fun run(flag:Boolean) { var n=1; val f: () -> Int={n}; val optional=if(flag) f else null; val none: Nothing?=null; val selected=none ?: optional; n=3 }",
            None,
        ),
        (
            "fun empty() {}\nfun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flag:Boolean): () -> Unit { val none: Nothing?=null; return (none ?: (if(flag) { return empty; val f: () -> Unit={read(xs)}; f } else null)) ?: ({}) }",
            None,
        ),
        (
            "fun make(): () -> Unit { return ({}) }\nfun run(flag:Boolean): () -> Unit { val none: Nothing?=null; val optional=if(flag) make() else null; val selected=none ?: optional; return selected!! }",
            None,
        ),
    ];
    for (text, code) in cases {
        for rooted in [false, true] {
            let (_, owned) =
                borrow_rejection_unit::checked(text, "nested.ko", rooted, standard_environments());
            let codes = owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>();
            assert_eq!(
                codes,
                code.into_iter().collect::<Vec<_>>(),
                "{text}/{rooted}"
            );
            if code.is_some() {
                assert!(owned.loans().is_empty());
                assert!(owned.drops().is_empty());
                assert!(owned.validate().is_err());
            } else {
                assert!(owned.validate().is_ok());
            }
        }
    }
}
