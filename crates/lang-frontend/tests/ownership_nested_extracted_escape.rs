//! Nested move capture must preserve diagnostic-only borrowed origins.
#[path = "support/borrow_rejection_unit.rs"]
mod borrow_rejection_unit;
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;
use lang_frontend::type_checking::standard_environments;

#[test]
fn move_wrapper_cannot_hide_extracted_borrowed_callable() {
    let mut missing = Vec::new();
    for delivery in [
        "return move { extracted() }",
        "val wrapper: () -> Unit = move { extracted() }; return wrapper",
        "val wrapper: () -> Unit = move { extracted() }; val outer: () -> Unit = move { wrapper() }; return outer",
    ] {
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag: Boolean): () -> Unit {{ val f: () -> Unit = {{ read(xs) }}; val optional=if(flag) f else null; val extracted=optional!!; {delivery} }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "nested-extracted.ko",
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
                    "{delivery}/{rooted}: {codes:?}, validate={}",
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
fn legal_extracted_callable_can_escape_through_move_wrapper() {
    for value in ["{}", "move { read(xs) }"] {
        for delivery in [
            "return move { extracted() }",
            "val wrapper: () -> Unit = move { extracted() }; return wrapper",
            "val wrapper: () -> Unit = move { extracted() }; val outer: () -> Unit = move { wrapper() }; return outer",
        ] {
            let text = format!(
                "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag: Boolean): () -> Unit {{ val f: () -> Unit = {value}; val optional=if(flag) f else null; val extracted=optional!!; {delivery} }}"
            );
            for rooted in [false, true] {
                let (_, owned) = borrow_rejection_unit::checked(
                    &text,
                    "nested-extracted.ko",
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
fn local_wrapper_and_existing_errors_keep_their_contracts() {
    for (text, expected) in [
        (
            "fun read(xs:List<Int>) {}\nfun run(own xs:List<Int>,flag:Boolean) { val f: () -> Unit = { read(xs) }; val optional=if(flag) f else null; val extracted=optional!!; loop { val wrapper: () -> Unit = move { extracted() }; wrapper(); break } }",
            &[][..],
        ),
        (
            "fun read(xs:List<Int>) {}\nfun run(own xs:List<Int>,flag:Boolean) { val f: () -> Unit = { read(xs) }; val optional=if(flag) f else null; val extracted=optional!!; val wrapper: () -> Unit = move { extracted() }; wrapper() }",
            &[][..],
        ),
        (
            "fun read(xs:List<Int>) {}\nfun run(own xs:List<Int>): () -> Unit { val f: () -> Unit = { read(xs) }; return move { f() } }",
            &["L0137"][..],
        ),
        (
            "fun read(xs:List<Int>) {}\nfun run(own xs:List<Int>,flag:Boolean): () -> Unit { val f: () -> Unit = { read(xs) }; val optional=if(flag) f else null; val extracted=optional!!; val moved=extracted; return move { extracted() } }",
            &["L0131"][..],
        ),
    ] {
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                text,
                "nested-controls.ko",
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
            if expected.is_empty() {
                assert!(owned.validate().is_ok());
            } else {
                assert!(owned.validate().is_err());
            }
        }
    }
}
