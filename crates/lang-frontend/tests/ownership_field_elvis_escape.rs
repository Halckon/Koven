//! Probe ordinary closure field storage separately from inout/SSA admission.
#[path = "support/borrow_rejection_unit.rs"]
mod borrow_rejection_unit;
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;
use lang_frontend::type_checking::standard_environments;

#[test]
fn field_elvis_cannot_hide_a_borrowed_closure() {
    let mut missing = Vec::new();
    for (parameters, setup) in [
        (
            "own xs: List<Int>, flag: Boolean",
            "val holder = Holder({});",
        ),
        ("inout holder: Holder, own xs: List<Int>, flag: Boolean", ""),
    ] {
        for value in [
            "({ read(xs) })",
            "if (flag) { val f: () -> Unit = { read(xs) }; f } else ({})",
            "none ?: (if (flag) { val f: () -> Unit = { read(xs) }; f } else ({}))",
            "((none ?: (if (flag) { val f: () -> Unit = { read(xs) }; f } else ({}))))",
            "none ?: (when (flag) { true -> { val f: () -> Unit = { read(xs) }; f }; else -> ({}) })",
        ] {
            let text = format!(
                "class Holder(var callback: () -> Unit)\nfun read(xs: List<Int>) {{}}\nfun run({parameters}) {{ {setup} val none: Nothing? = null; holder.callback = {value} }}"
            );
            for rooted in [false, true] {
                let (_, owned) = borrow_rejection_unit::checked(
                    &text,
                    "field.ko",
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
                    missing.push(format!(
                        "{parameters}/{value}/{rooted}: {:?}",
                        owned.diagnostics()
                    ));
                }
                assert!(owned.loans().is_empty());
                assert!(owned.drops().is_empty());
                assert!(owned.field_replacements().is_empty());
                assert!(owned.validate().is_err());
            }
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

#[test]
fn field_elvis_capture_free_is_allowed_for_both_targets() {
    for (parameters, setup) in [
        ("flag: Boolean", "val holder = Holder({});"),
        ("inout holder: Holder, flag: Boolean", ""),
    ] {
        let text = format!(
            "class Holder(var callback: () -> Unit)\nfun run({parameters}) {{ {setup} val none: Nothing? = null; holder.callback = none ?: (if(flag) {{ val f: () -> Unit = {{}}; f }} else ({{}})) }}"
        );
        for rooted in [false, true] {
            let (_, owned) =
                borrow_rejection_unit::checked(&text, "field.ko", rooted, standard_environments());
            assert!(
                owned.diagnostics().is_empty(),
                "{parameters}/{rooted}: {:?}",
                owned.diagnostics()
            );
            assert!(owned.validate().is_ok());
        }
    }
}

#[test]
fn field_elvis_preserves_immutable_diagnostic() {
    let text = "class Holder(val callback: () -> Unit)\nfun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flag: Boolean) { val holder = Holder({}); val none: Nothing? = null; holder.callback = none ?: (if(flag) { val f: () -> Unit = { read(xs) }; f } else ({})) }";
    for rooted in [false, true] {
        let (_, owned) =
            borrow_rejection_unit::checked(text, "field.ko", rooted, standard_environments());
        assert_eq!(
            owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>(),
            ["L0134"]
        );
        assert!(owned.field_replacements().is_empty());
        assert!(owned.loans().is_empty());
        assert!(owned.drops().is_empty());
        assert!(owned.validate().is_err());
    }
}

#[test]
fn field_elvis_preserves_owned_values_and_exit() {
    for (case, text) in [
        "class Holder(var callback: () -> Unit)\nfun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flag: Boolean) { val holder = Holder({}); val none: Nothing? = null; holder.callback = none ?: (if(flag) { val f: () -> Unit = move { read(xs) }; f } else ({})) }",
        "class Holder(var callback: () -> Unit)\nfun run(flag: Boolean) { val holder = Holder({}); val none: Nothing? = null; holder.callback = none ?: (if(flag) return else return) }",
        "class Holder(var callback: () -> Unit)\nfun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flag: Boolean) { var local: () -> Unit = {}; val none: Nothing? = null; local = none ?: (if(flag) { val f: () -> Unit = { read(xs) }; f } else ({})); local() }",
    ].into_iter().enumerate() {
        for rooted in [false, true] {
            let (_, owned) =
                borrow_rejection_unit::checked(text, "field.ko", rooted, standard_environments());
            assert!(
                owned.diagnostics().is_empty(),
                "{text}/{rooted}: {:?}",
                owned.diagnostics()
            );
            if case == 1 {
                assert!(owned.field_replacements().is_empty());
            }
            assert!(owned.validate().is_ok());
        }
    }
}
