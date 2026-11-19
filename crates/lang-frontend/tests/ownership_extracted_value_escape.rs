//! Ordinary Value delivery consumes diagnostic-only extracted callable origins.
#[path = "support/borrow_rejection_unit.rs"]
mod borrow_rejection_unit;
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;
use lang_frontend::type_checking::standard_environments;

#[test]
fn extracted_callable_value_delivery_keeps_borrowed_rejection() {
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
            "fun read(xs: List<Int>) {{}}\nfun sink(own callback: () -> Unit) {{}}\nfun borrowSink(callback: () -> Unit) {{}}\nfun run(own xs: List<Int>,flag:Boolean) {{ val f: () -> Unit = {{ read(xs) }}; {setup} sink({input}) }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "extracted-value.ko",
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
fn extracted_legal_value_and_shared_call_remain_accepted() {
    for (value, call) in [
        ("{}", "sink(extracted)"),
        ("move { read(xs) }", "sink(extracted)"),
        ("{ read(xs) }", "extracted()"),
        ("{ read(xs) }", "borrowSink(extracted)"),
    ] {
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun sink(own callback: () -> Unit) {{}}\nfun borrowSink(callback: () -> Unit) {{}}\nfun run(own xs: List<Int>,flag:Boolean) {{ val f: () -> Unit = {value}; val optional=if(flag) f else null; val extracted=optional!!; {call} }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "extracted-value.ko",
                rooted,
                standard_environments(),
            );
            assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
            assert!(owned.validate().is_ok());
        }
    }
}

#[test]
fn extracted_value_preserves_moved_and_exit_priority() {
    for (delivery, expected) in [
        ("val moved=extracted; sink(extracted)", &["L0131"][..]),
        ("sink(if(flag) { return } else { return })", &[][..]),
        ("sink((extracted))", &["L0137"][..]),
        ("sink(if(flag) extracted else ({}))", &["L0137"][..]),
    ] {
        let text = format!(
            "fun read(xs:List<Int>) {{}}\nfun sink(own callback: () -> Unit) {{}}\nfun borrowSink(callback: () -> Unit) {{}}\nfun run(own xs:List<Int>,flag:Boolean) {{ val f: () -> Unit = {{ read(xs) }}; val optional=if(flag) f else null; val extracted=optional!!; {delivery} }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "value-controls.ko",
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
                "{delivery}/{rooted}"
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
