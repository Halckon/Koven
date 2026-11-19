//! Diagnostic provenance at field storage after non-escaping callable extraction.
#[path = "support/borrow_rejection_unit.rs"]
mod borrow_rejection_unit;
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;
use lang_frontend::type_checking::standard_environments;

#[test]
fn extracted_borrowed_callable_cannot_escape_into_field() {
    let mut missing = Vec::new();
    for (parameters, setup) in [
        (
            "own xs: List<Int>, flag: Boolean, again: Boolean",
            "val holder=Holder({});",
        ),
        (
            "inout holder: Holder, own xs: List<Int>, flag: Boolean, again: Boolean",
            "",
        ),
    ] {
        for value in [
            "extracted",
            "((extracted))",
            "if(again) extracted else ({})",
            "when(again) { true -> extracted; else -> ({}) }",
        ] {
            let text = format!(
                "class Holder(var callback: () -> Unit)\nfun read(xs: List<Int>) {{}}\nfun run({parameters}) {{ {setup} val f: () -> Unit = {{ read(xs) }}; val optional=if(flag) f else null; val extracted=optional!!; holder.callback={value} }}"
            );
            for rooted in [false, true] {
                let (_, owned) = borrow_rejection_unit::checked(
                    &text,
                    "field-extracted.ko",
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
                        "{parameters}/{value}/{rooted}: {codes:?}, validate={}",
                        owned.clone().validate().is_ok()
                    ));
                } else {
                    assert!(owned.loans().is_empty());
                    assert!(owned.drops().is_empty());
                    assert!(owned.field_replacements().is_empty());
                    assert!(owned.validate().is_err());
                }
            }
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

#[test]
fn extracted_legal_callable_can_be_stored_in_field() {
    for value in ["{}", "move { read(xs) }"] {
        for (parameters, setup) in [
            ("own xs: List<Int>, flag: Boolean", "val holder=Holder({});"),
            ("inout holder: Holder, own xs: List<Int>, flag: Boolean", ""),
        ] {
            let text = format!(
                "class Holder(var callback: () -> Unit)\nfun read(xs: List<Int>) {{}}\nfun run({parameters}) {{ {setup} val f: () -> Unit = {value}; val optional=if(flag) f else null; val extracted=optional!!; holder.callback=extracted }}"
            );
            for rooted in [false, true] {
                let (_, owned) = borrow_rejection_unit::checked(
                    &text,
                    "field-extracted.ko",
                    rooted,
                    standard_environments(),
                );
                assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
                assert!(owned.validate().is_ok());
            }
        }
    }
}

#[test]
fn field_storage_keeps_error_priority_and_nonstorage_release() {
    for (field, delivery, expected) in [
        ("val", "holder.callback=extracted", &["L0134"][..]),
        (
            "var",
            "val moved=extracted; holder.callback=extracted",
            &["L0131"][..],
        ),
        ("var", "holder.callback=({ read(xs) })", &["L0137"][..]),
        ("var", "val local=extracted", &[][..]),
        (
            "var",
            "holder.callback=if(flag) { return } else { return }",
            &[][..],
        ),
    ] {
        let text = format!(
            "class Holder({field} callback: () -> Unit)\nfun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag: Boolean) {{ val holder=Holder({{}}); val f: () -> Unit = {{ read(xs) }}; val optional=if(flag) f else null; val extracted=optional!!; {delivery} }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "field-controls.ko",
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
                "{field}/{delivery}/{rooted}"
            );
            if expected.is_empty() {
                assert!(owned.field_replacements().is_empty());
                assert!(owned.validate().is_ok());
            } else {
                assert!(owned.loans().is_empty());
                assert!(owned.drops().is_empty());
                assert!(owned.field_replacements().is_empty());
                assert!(owned.validate().is_err());
            }
        }
    }
    for rooted in [false, true] {
        let (_, owned) = borrow_rejection_unit::checked(
            "fun run(flag:Boolean) { var n=1; val f: () -> Int = { n }; val optional=if(flag) f else null; val extracted=optional!!; val local=extracted; n=3 }",
            "field-release.ko",
            rooted,
            standard_environments(),
        );
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.validate().is_ok());
    }
}
