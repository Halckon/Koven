//! Construction field Value delivery through Elvis control results.
#[path = "support/borrow_rejection_unit.rs"]
mod borrow_rejection_unit;
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;
use lang_frontend::type_checking::standard_environments;

#[test]
fn construction_elvis_cannot_store_borrowed_closure() {
    let mut missing = Vec::new();
    for value in [
        "({ read(xs) })",
        "if (flag) { val f: () -> Unit = { read(xs) }; f } else ({})",
        "none ?: (if (flag) { val f: () -> Unit = { read(xs) }; f } else ({}))",
        "((none ?: (if (flag) { val f: () -> Unit = { read(xs) }; f } else ({}))))",
        "none ?: (when (flag) { true -> { val f: () -> Unit = { read(xs) }; f }; else -> ({}) })",
    ] {
        let text = format!(
            "class Holder(val callback: () -> Unit)\nfun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag: Boolean) {{ val none: Nothing? = null; val holder = Holder({value}) }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "construction.ko",
                rooted,
                standard_environments(),
            );
            if owned
                .diagnostics()
                .iter()
                .filter(|d| d.code().to_string() == "L0137")
                .count()
                != 1
            {
                missing.push(format!("{value}/{rooted}: {:?}", owned.diagnostics()));
            }
            assert!(owned.construction_plans().is_empty());
            assert!(owned.loans().is_empty());
            assert!(owned.drops().is_empty());
            assert!(owned.validate().is_err());
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

#[test]
fn construction_elvis_capture_free_is_allowed() {
    let text = "class Holder(val callback: () -> Unit)\nfun run(flag: Boolean) { val none: Nothing? = null; val holder = Holder(none ?: (if(flag) { val f: () -> Unit = {}; f } else ({}))) }";
    for rooted in [false, true] {
        let (_, owned) = borrow_rejection_unit::checked(
            text,
            "construction.ko",
            rooted,
            standard_environments(),
        );
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.validate().is_ok());
    }
}

#[test]
fn construction_elvis_keeps_owned_and_early_exit_arguments() {
    for text in [
        "class Holder(val callback: () -> Unit)\nfun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flag: Boolean) { val none: Nothing? = null; val h = Holder(none ?: (if(flag) { val f: () -> Unit = move { read(xs) }; f } else ({}))) }",
        "class Holder(val before: Unit, val callback: () -> Unit)\nfun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flag: Boolean) { val none: Nothing? = null; val h = Holder(return, none ?: (if(flag) { val f: () -> Unit = { read(xs) }; f } else ({}))) }",
        "class Holder(val callback: () -> Unit, val before: Unit)\nfun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flag: Boolean) { val none: Nothing? = null; val h = Holder(before = return, callback = none ?: (if(flag) { val f: () -> Unit = { read(xs) }; f } else ({}))) }",
    ] {
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                text,
                "construction.ko",
                rooted,
                standard_environments(),
            );
            assert!(
                owned.diagnostics().is_empty(),
                "{text}/{rooted}: {:?}",
                owned.diagnostics()
            );
            assert!(owned.validate().is_ok());
        }
    }
}

#[test]
fn construction_direct_control_keeps_single_escape_diagnostic() {
    let text = "class Holder(val callback: () -> Unit)\nfun run(flag: Boolean) { var n = 1; val update: () -> Unit = { n = 2 }; val h = Holder(((if(flag) update else update))) }";
    for rooted in [false, true] {
        let (_, owned) = borrow_rejection_unit::checked(
            text,
            "construction.ko",
            rooted,
            standard_environments(),
        );
        assert_eq!(
            owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>(),
            ["L0137"]
        );
        assert!(owned.construction_plans().is_empty());
        assert!(owned.validate().is_err());
    }
}
