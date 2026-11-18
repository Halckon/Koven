//! Original Value delivery contract through control-expression wrappers.
#[path = "support/borrow_rejection_unit.rs"]
mod borrow_rejection_unit;
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;
use lang_frontend::type_checking::standard_environments;

#[test]
fn value_call_elvis_fallback_cannot_hide_borrowed_closure() {
    let mut missing = Vec::new();
    for value in [
        "({ read(xs) })",
        "if (flag) { val f: () -> Unit = { read(xs) }; f } else ({})",
        "none ?: (if (flag) { val f: () -> Unit = { read(xs) }; f } else ({}))",
        "((none ?: (if (flag) { val f: () -> Unit = { read(xs) }; f } else ({}))))",
        "none ?: (when (flag) { true -> { val f: () -> Unit = { read(xs) }; f }; else -> ({}) })",
    ] {
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun deliver(own callback: () -> Unit) {{}}\nfun run(own xs: List<Int>, flag: Boolean) {{ val none: Nothing? = null; deliver({value}) }}"
        );
        for rooted in [false, true] {
            let (_, owned) =
                borrow_rejection_unit::checked(&text, "value.ko", rooted, standard_environments());
            if owned
                .diagnostics()
                .iter()
                .filter(|d| d.code().to_string() == "L0137")
                .count()
                != 1
            {
                missing.push(format!("{value}/{rooted}: {:?}", owned.diagnostics()));
            }
            assert!(owned.loans().is_empty());
            assert!(owned.drops().is_empty());
            assert!(owned.validate().is_err());
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

#[test]
fn value_call_elvis_capture_free_is_allowed() {
    let text = "fun deliver(own callback: () -> Unit) {}\nfun run(flag: Boolean) { val none: Nothing? = null; deliver(none ?: (if(flag) { val f: () -> Unit = {}; f } else ({}))) }";
    for rooted in [false, true] {
        let (_, owned) =
            borrow_rejection_unit::checked(text, "value.ko", rooted, standard_environments());
        assert!(owned.diagnostics().is_empty(), "{:?}", owned.diagnostics());
        assert!(owned.validate().is_ok());
    }
}

#[test]
fn value_delivery_keeps_owned_shared_and_unreachable_arguments() {
    for text in [
        "fun read(xs: List<Int>) {}\nfun deliver(own callback: () -> Unit) {}\nfun run(own xs: List<Int>, flag: Boolean) { val none: Nothing? = null; deliver(none ?: (if(flag) { val f: () -> Unit = move { read(xs) }; f } else ({}))) }",
        "fun read(xs: List<Int>) {}\nfun deliver(callback: () -> Unit) {}\nfun run(own xs: List<Int>, flag: Boolean) { val none: Nothing? = null; deliver(none ?: (if(flag) { val f: () -> Unit = { read(xs) }; f } else ({}))) }",
        "fun read(xs: List<Int>) {}\nfun deliver(before: Unit, own callback: () -> Unit) {}\nfun run(own xs: List<Int>, flag: Boolean) { val none: Nothing? = null; deliver(return, none ?: (if(flag) { val f: () -> Unit = { read(xs) }; f } else ({}))) }",
    ] {
        for rooted in [false, true] {
            let (_, owned) =
                borrow_rejection_unit::checked(text, "value.ko", rooted, standard_environments());
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
fn value_delivery_keeps_nullable_exclusive_diagnostic_priority() {
    let text = "fun deliver(own callback: () -> Unit) {}\nfun run(flag: Boolean) { var n = 1; val f: () -> Unit = { n = 2 }; deliver((if(flag) f else null)!!) }";
    for rooted in [false, true] {
        let (_, owned) =
            borrow_rejection_unit::checked(text, "value.ko", rooted, standard_environments());
        assert_eq!(
            owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>(),
            ["L0164"]
        );
        assert!(owned.validate().is_err());
    }
}

#[test]
fn direct_control_value_delivery_keeps_aggregated_escape_diagnostic() {
    for value in [
        "if(flag) update else update",
        "((if(flag) update else update))",
    ] {
        let text = format!(
            "fun deliver(own cb: () -> Unit) {{}}\nfun run(flag: Boolean) {{ var n = 1; val update: () -> Unit = {{ n = 2 }}; deliver({value}) }}"
        );
        for rooted in [false, true] {
            let (_, owned) =
                borrow_rejection_unit::checked(&text, "value.ko", rooted, standard_environments());
            assert_eq!(
                owned
                    .diagnostics()
                    .iter()
                    .map(|d| d.code().to_string())
                    .collect::<Vec<_>>(),
                ["L0137"]
            );
            assert!(owned.validate().is_err());
        }
    }
}
