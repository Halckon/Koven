//! A local wrapper still checks actual escaping consumers in its callable body.
#[path = "support/borrow_rejection_unit.rs"]
mod borrow_rejection_unit;
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;
use lang_frontend::type_checking::standard_environments;

#[test]
fn captured_callable_value_delivery_keeps_borrowed_rejection() {
    let mut missing = Vec::new();
    for (setup, input) in [
        ("", "f"),
        (
            "val optional=if(flag) f else null; val extracted=optional!!;",
            "extracted",
        ),
        (
            "val optional=if(flag) f else null; val extracted=optional!!; val alias=extracted;",
            "alias",
        ),
    ] {
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun sink(own callback: () -> Unit) {{}}\nfun run(own xs: List<Int>,flag:Boolean) {{ val f: () -> Unit = {{ read(xs) }}; {setup} val wrapper: () -> Unit = move {{ sink({input}) }}; wrapper() }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "captured-body.ko",
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
                    "{input}/{rooted}: {codes:?}, validate={}",
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
fn captured_legal_value_and_shared_call_remain_accepted() {
    for (value, call) in [
        ("{}", "sink(extracted)"),
        ("move { read(xs) }", "sink(extracted)"),
        ("{ read(xs) }", "extracted()"),
    ] {
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun sink(own callback: () -> Unit) {{}}\nfun run(own xs: List<Int>,flag:Boolean) {{ val f: () -> Unit = {value}; val optional=if(flag) f else null; val extracted=optional!!; val wrapper: () -> Unit = move {{ {call} }}; wrapper() }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "captured-body.ko",
                rooted,
                standard_environments(),
            );
            assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
            assert!(owned.validate().is_ok());
        }
    }
}

#[test]
fn captured_body_preserves_move_write_and_local_call_contracts() {
    for (before, body, expected) in [
        ("val moved=extracted;", "sink(extracted)", &["L0131"][..]),
        ("", "extracted={}", &["L0134"][..]),
        ("", "borrowSink(extracted)", &[][..]),
        ("", "val local=extracted; local()", &[][..]),
    ] {
        let text = format!(
            "fun read(xs:List<Int>) {{}}\nfun sink(own callback: () -> Unit) {{}}\nfun borrowSink(callback: () -> Unit) {{}}\nfun run(own xs:List<Int>,flag:Boolean) {{ val f: () -> Unit = {{ read(xs) }}; val optional=if(flag) f else null; val extracted=optional!!; {before} val wrapper: () -> Unit = move {{ {body} }}; wrapper() }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "body-controls.ko",
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
                "{before}/{body}/{rooted}"
            );
            if expected.is_empty() {
                assert!(owned.validate().is_ok());
            } else {
                assert!(owned.loans().is_empty());
                assert!(owned.drops().is_empty());
                assert!(owned.validate().is_err());
            }
        }
    }
}
