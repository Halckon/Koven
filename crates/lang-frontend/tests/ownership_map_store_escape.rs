//! Map put and subscript storage must obey the same Value escape boundary.
#[path = "support/borrow_rejection_unit.rs"]
mod borrow_rejection_unit;
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;
use lang_frontend::type_checking::standard_environments;

#[test]
fn map_storage_rejects_borrowed_callable_for_both_spellings() {
    let mut missing = Vec::new();
    for (setup, input) in [
        ("", "f"),
        ("", "(f)"),
        (
            "",
            "if(flag) { val local:()->Unit={read(xs)}; local } else ({})",
        ),
        (
            "",
            "when { flag -> { val local:()->Unit={read(xs)}; local }; else -> ({}) }",
        ),
        (
            "val optional=if(flag) f else null; val extracted=optional!!;",
            "extracted",
        ),
    ] {
        for operation in [format!("m.put(1,{input})"), format!("m[1]={input}")] {
            let text = format!(
                "fun read(xs:List<Int>) {{}}\nfun run(own xs:List<Int>,flag:Boolean) {{var m=mutableMapOf<Int,()->Unit>();val f:()->Unit={{read(xs)}};{setup}{operation}}}"
            );
            for rooted in [false, true] {
                let (_, owned) = borrow_rejection_unit::checked(
                    &text,
                    "map-storage.ko",
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
                        "{operation}/{rooted}: {codes:?}, validate={}",
                        owned.clone().validate().is_ok()
                    ));
                } else {
                    assert!(owned.loans().is_empty());
                    assert!(owned.drops().is_empty());
                    assert!(owned.validate().is_err());
                }
            }
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

#[test]
fn map_storage_accepts_capturefree_and_owned_callable() {
    for value in ["{}", "move {read(xs)}"] {
        for operation in ["m.put(1,f)", "m[1]=f"] {
            let text = format!(
                "fun read(xs:List<Int>) {{}}\nfun run(own xs:List<Int>) {{var m=mutableMapOf<Int,()->Unit>();val f:()->Unit={value};{operation}}}"
            );
            for rooted in [false, true] {
                let (_, owned) = borrow_rejection_unit::checked(
                    &text,
                    "map-storage.ko",
                    rooted,
                    standard_environments(),
                );
                assert!(
                    owned.diagnostics().is_empty(),
                    "{operation}: {:?}",
                    owned.diagnostics()
                );
                assert!(owned.validate().is_ok());
            }
        }
    }
}

#[test]
fn map_storage_preserves_existing_error_and_exit_priority() {
    for (binding, setup, operation, expected) in [
        ("val", "", "m[1]=f", &["L0134"][..]),
        ("var", "val moved=f;", "m[1]=f", &["L0131"][..]),
        ("val", "val moved=f;", "m[1]=f", &["L0131"][..]),
        ("var", "", "m[return]=f", &[][..]),
        ("var", "", "m[1]=return", &[][..]),
        ("val", "", "m[1]=return", &[][..]),
    ] {
        let text = format!(
            "fun read(xs:List<Int>) {{}}\nfun run(own xs:List<Int>) {{{binding} m=mutableMapOf<Int,()->Unit>();val f:()->Unit={{read(xs)}};{setup}{operation}}}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "map-storage-controls.ko",
                rooted,
                standard_environments(),
            );
            let codes: Vec<_> = owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect();
            assert_eq!(codes, expected, "{binding}/{setup}/{operation}/{rooted}");
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

#[test]
fn map_storage_preserves_active_receiver_loan_priority() {
    let text = "fun read(xs:List<Int>) {}\nfun observe(m:MutableMap<Int,()->Unit>) {}\nfun run(own xs:List<Int>) { var m=mutableMapOf<Int,()->Unit>(); val watch:()->Unit={observe(m)}; val f:()->Unit={read(xs)}; m[1]=f; watch() }";
    for rooted in [false, true] {
        let (_, owned) = borrow_rejection_unit::checked(
            text,
            "map-storage-loan.ko",
            rooted,
            standard_environments(),
        );
        assert_eq!(
            owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>(),
            ["L0135"]
        );
        assert!(owned.loans().is_empty());
        assert!(owned.drops().is_empty());
        assert!(owned.validate().is_err());
    }
}
