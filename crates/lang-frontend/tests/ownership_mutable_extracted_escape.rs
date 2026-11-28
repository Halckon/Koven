//! A mutable concrete callable must retain positive diagnostic origins after extraction.
#[path = "support/borrow_rejection_unit.rs"]
mod borrow_rejection_unit;
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;
use lang_frontend::type_checking::standard_environments;

#[test]
fn mutable_extracted_callable_keeps_each_borrowed_return() {
    let mut missing = Vec::new();
    for body in [
        "val saved=optional!!; return saved",
        "var saved=optional!!; return saved",
        "var saved:()->Unit={}; saved=optional!!; return saved",
        "var saved=optional!!; if(again){saved={}}; return saved",
        "var saved=optional!!; if(again){saved=make()}; return saved",
        "var saved=optional!!; if(again){var saved:()->Unit={}; saved()}; return saved",
    ] {
        let text = format!(
            "fun read(xs:List<Int>) {{}}\nfun make():()->Unit {{return ({{}})}}\nfun run(own xs:List<Int>,flag:Boolean,again:Boolean):()->Unit {{val f:()->Unit={{read(xs)}};val optional=if(flag) f else null;{body}}}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "mutable-extracted.ko",
                rooted,
                standard_environments(),
            );
            let codes: Vec<_> = owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect();
            if codes != ["L0137"] {
                missing.push(format!(
                    "{body}/{rooted}: {codes:?}, validate={}",
                    owned.clone().validate().is_ok()
                ));
            } else {
                assert!(owned.loans().is_empty());
                assert!(owned.drops().is_empty());
                assert!(owned.validate().is_err());
            }
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

#[test]
fn mutable_extracted_callable_keeps_safe_replacements() {
    for (value, body) in [
        ("{}", "var saved=optional!!; return saved"),
        (
            "{read(xs)}",
            "var saved=optional!!; if(again){var saved:()->Unit={}; saved()}; saved={}; return saved",
        ),
        ("move {read(xs)}", "var saved=optional!!; return saved"),
        ("{read(xs)}", "var saved=optional!!; saved={}; return saved"),
        (
            "{read(xs)}",
            "var saved=optional!!; if(again){saved={}}else{saved={}}; return saved",
        ),
    ] {
        let text = format!(
            "fun read(xs:List<Int>) {{}}\nfun make():()->Unit {{return ({{}})}}\nfun run(own xs:List<Int>,flag:Boolean,again:Boolean):()->Unit {{val f:()->Unit={value};val optional=if(flag) f else null;{body}}}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "mutable-extracted.ko",
                rooted,
                standard_environments(),
            );
            assert!(
                owned.diagnostics().is_empty(),
                "{body}: {:?}",
                owned.diagnostics()
            );
            assert!(owned.validate().is_ok());
        }
    }
}

#[test]
fn mutable_extracted_callable_preserves_move_and_failed_assignment() {
    for (tail, expected) in [
        ("val moved=saved; return saved", &["L0131"][..]),
        (
            "saved=if(again){consume(xs);consume(xs);({})}else({}); return saved",
            &["L0131", "L0137"][..],
        ),
    ] {
        let text = format!(
            "fun read(xs:List<Int>) {{}}\nfun consume(own xs:List<Int>) {{}}\nfun run(own xs:List<Int>,flag:Boolean,again:Boolean):()->Unit {{val f:()->Unit={{read(xs)}};val optional=if(flag) f else null;var saved=optional!!;{tail}}}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "mutable-extracted-controls.ko",
                rooted,
                standard_environments(),
            );
            assert_eq!(
                owned
                    .diagnostics()
                    .iter()
                    .map(|d| d.code().to_string())
                    .collect::<Vec<_>>(),
                expected,
                "{tail}/{rooted}"
            );
            assert!(owned.loans().is_empty());
            assert!(owned.drops().is_empty());
            assert!(owned.validate().is_err());
        }
    }
}
