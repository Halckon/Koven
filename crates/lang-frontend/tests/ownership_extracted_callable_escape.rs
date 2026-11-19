//! Diagnostic provenance across a non-escaping non-null extraction.
#[path = "support/borrow_rejection_unit.rs"]
mod borrow_rejection_unit;
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;
use lang_frontend::type_checking::standard_environments;

#[test]
fn extracted_borrowed_callable_keeps_escape_rejection() {
    let mut missing = Vec::new();
    for delivery in [
        "return extracted",
        "val second=extracted; val third=second; return third",
        "return (if (again) extracted else ({}))",
        "val wrapped = if(again) extracted else null; return wrapped!!",
    ] {
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag: Boolean, again: Boolean): () -> Unit {{ val f: () -> Unit = {{ read(xs) }}; val optional = if(flag) f else null; val extracted = optional!!; {delivery} }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "extracted.ko",
                rooted,
                standard_environments(),
            );
            let codes: Vec<_> = owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect();
            if codes == ["L0137"] {
                assert!(owned.loans().is_empty());
                assert!(owned.drops().is_empty());
                assert!(owned.non_null_assertions().is_empty());
            }
            if codes != ["L0137"] {
                missing.push(format!("{delivery}/{rooted}: {codes:?}"));
            }
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

#[test]
fn extracted_legal_callable_remains_accepted() {
    for value in ["{}", "move { read(xs) }"] {
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag: Boolean): () -> Unit {{ val f: () -> Unit = {value}; val optional=if(flag) f else null; val extracted=optional!!; return extracted }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "extracted.ko",
                rooted,
                standard_environments(),
            );
            assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
            assert!(owned.validate().is_ok());
        }
    }
}

#[test]
fn extraction_keeps_failure_priority_and_unused_release() {
    for (text, expected) in [
        (
            "fun read(xs:List<Int>) {}\nfun run(own xs:List<Int>,flag:Boolean): () -> Unit { val f: () -> Unit = { read(xs) }; val optional=if(flag) f else null; val extracted=optional!!; val other=extracted; return extracted }",
            &["L0131"][..],
        ),
        (
            "fun read(xs:List<Int>) {}\nfun run(own xs:List<Int>,flag:Boolean): () -> Unit { val f: () -> Unit = { read(xs) }; val optional=if(flag) f else null; val other=optional; val extracted=optional!!; return extracted }",
            &["L0131"][..],
        ),
        (
            "fun run(flag:Boolean) { var n=1; val f: () -> Int = { n }; val optional=if(flag) f else null; val extracted=optional!!; n=3 }",
            &[][..],
        ),
        (
            "fun run(flag:Boolean): () -> Unit { val f: () -> Unit = {}; val optional=if(flag) f else null; val extracted=optional!!; val other=extracted; return extracted }",
            &["L0131"][..],
        ),
        (
            "fun run(flag:Boolean): () -> Unit { val f: () -> Unit = {}; val optional=if(flag) f else null; val other=optional; val extracted=optional!!; return extracted }",
            &["L0131"][..],
        ),
        (
            "fun read(xs:List<Int>) {}\nfun run(own xs:List<Int>,flag:Boolean): () -> Unit { val f: () -> Unit = { read(xs) }; var optional=if(flag) f else null; val extracted=optional!!; optional=null; return extracted }",
            &["L0137"][..],
        ),
    ] {
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                text,
                "extracted-controls.ko",
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
                "{text}/{rooted}"
            );
            assert_eq!(owned.validate().is_ok(), expected.is_empty());
        }
    }
}
