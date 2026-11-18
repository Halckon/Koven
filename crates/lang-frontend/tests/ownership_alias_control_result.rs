//! Immutable nullable aliases transported through a second control expression.
#[path = "support/borrow_rejection_unit.rs"]
mod borrow_rejection_unit;
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;
use lang_frontend::type_checking::standard_environments;

#[test]
fn nullable_alias_control_results_preserve_escape_rejection() {
    let mut missing = Vec::new();
    for result in [
        "optional!!",
        "(if (again) optional else null)!!",
        "(if (again) null else optional)!!",
        "(when (again) { true -> optional; else -> null })!!",
        "((if (again) (optional) else null))!!",
        "(if (again) optional else { val direct: () -> Unit = { read(xs) }; direct })!!",
    ] {
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, flag: Boolean, again: Boolean): () -> Unit {{ val f: () -> Unit = {{ read(xs) }}; val optional = if (flag) f else null; return {result} }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "alias-control.ko",
                rooted,
                standard_environments(),
            );
            if owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>()
                != ["L0137"]
            {
                missing.push(format!("{result}/{rooted}: {:?}", owned.diagnostics()));
            }
            assert!(owned.non_null_assertions().is_empty());
            assert!(owned.loans().is_empty());
            assert!(owned.drops().is_empty());
            assert!(owned.validate().is_err());
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

#[test]
fn nullable_alias_control_results_keep_legal_values_and_release() {
    for text in [
        "fun run(flag: Boolean, again: Boolean): () -> Unit { val f: () -> Unit = {}; val optional = if(flag) f else null; return (if(again) optional else null)!! }",
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flag: Boolean, again: Boolean): () -> Unit { val f: () -> Unit = move { read(xs) }; val optional = if(flag) f else null; return (when(again) { true -> optional; else -> null })!! }",
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flag: Boolean, again: Boolean): () -> Unit { val f: () -> Unit = { read(xs) }; val optional=if(flag) f else null; return (if(again) { return ({}); optional } else null)!! }",
        "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flag: Boolean, again: Boolean): () -> Unit { val f: () -> Unit = { read(xs) }; val optional=if(flag) f else null; return (when(again) { true -> { return ({}); optional }; else -> null })!! }",
        "fun run(flag: Boolean, again: Boolean) { var n=1; val f: () -> Int = { n }; val optional=if(flag) f else null; val later=if(again) optional else null; n=3 }",
    ] {
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                text,
                "alias-control.ko",
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
fn control_result_does_not_override_moved_source_diagnostic() {
    let text = "fun read(xs: List<Int>) {}\nfun run(own xs: List<Int>, flag: Boolean, again: Boolean): () -> Unit { val f: () -> Unit = { read(xs) }; val optional = if(flag) f else null; return (if(again) optional else f)!! }";
    for rooted in [false, true] {
        let (_, owned) = borrow_rejection_unit::checked(
            text,
            "alias-control.ko",
            rooted,
            standard_environments(),
        );
        assert_eq!(
            owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect::<Vec<_>>(),
            ["L0131"]
        );
        assert!(owned.non_null_assertions().is_empty());
        assert!(owned.validate().is_err());
    }
}
