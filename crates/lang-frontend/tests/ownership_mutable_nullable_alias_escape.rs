//! Mutable nullable alias escape diagnostics must follow committed assignments, not loans.
#[path = "support/borrow_rejection_unit.rs"]
mod borrow_rejection_unit;
#[path = "support/parser_test_assertions.rs"]
mod parser_test_assertions;
use lang_frontend::type_checking::standard_environments;

#[test]
fn mutable_nullable_alias_escape_tracks_each_reachable_value() {
    let mut missing = Vec::new();
    for body in [
        "var optional = if (flag) f else null; return optional!!",
        "var optional = if (flag) ({}) else null; optional = if (flag) f else null; return optional!!",
        "var optional = if (flag) ({}) else null; if (flag) { optional = if (flag) f else null }; return optional!!",
        "var optional = if (flag) f else null; if (flag) { optional = null }; return optional!!",
        "var optional = if (flag) f else null; val snapshot = optional; optional = null; return snapshot!!",
    ] {
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun make(): () -> Unit {{ return ({{}}) }}\nfun run(own xs: List<Int>, flag: Boolean): () -> Unit {{ val f: () -> Unit = {{ read(xs) }}; {body} }}"
        );
        for rooted in [false, true] {
            let (sources, owned) = borrow_rejection_unit::checked(
                &text,
                "mutable-alias.ko",
                rooted,
                standard_environments(),
            );
            let rejected = owned
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0137");
            if !rejected {
                missing.push(format!("{body}/{rooted}: {:?}", owned.diagnostics()));
            }
            for d in owned
                .diagnostics()
                .iter()
                .filter(|d| d.code().to_string() == "L0137")
            {
                assert_eq!(
                    sources.slice(d.primary_span()),
                    Ok(body.rsplit("return ").next().unwrap())
                );
            }
        }
    }
    assert!(missing.is_empty(), "{}", missing.join("\n"));
}

#[test]
fn committed_replacement_clears_diagnostic_origins_without_retaining_resources() {
    for body in [
        "var optional = if (flag) f else null; optional = null; return optional!!",
        "var optional = if (flag) ({}) else null; optional = null; optional = if (flag) f else null; optional = null; return optional!!",
        "var optional = if (flag) ({}) else null; optional = null; optional = if (flag) f else null; optional = if (flag) ({}) else null; return optional!!",
        "var optional = if (flag) f else null; if (flag) { optional = make() } else { optional = null }; return optional!!",
        "var optional = if (flag) f else null; optional = make(); return optional!!",
        "var optional = if (flag) f else null; optional = if (flag) { return ({}) } else null; return optional!!",
        "var optional = if (flag) f else null; loop { optional = null; break }; return optional!!",
        "var optional = if (flag) f else null; optional = if (flag) ({}) else null; return optional!!",
        "var optional = if (flag) f else null; if (flag) { optional = null } else { optional = if (flag) ({}) else null }; return optional!!",
        "var optional = if (flag) f else null; if (flag) { var optional = if (flag) ({}) else null; return optional!! }; return ({})",
    ] {
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun make(): () -> Unit {{ return ({{}}) }}\nfun run(own xs: List<Int>, flag: Boolean): () -> Unit {{ val f: () -> Unit = {{ read(xs) }}; {body} }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "mutable-alias.ko",
                rooted,
                standard_environments(),
            );
            assert!(
                owned.diagnostics().is_empty(),
                "{body}/{rooted}: {:?}",
                owned.diagnostics()
            );
            assert!(owned.validate().is_ok());
        }
    }
    let text = "fun run(flag: Boolean) { var n = 1; val f: () -> Int = { n }; var optional = if (flag) f else null; n = 3; optional = null }";
    for rooted in [false, true] {
        let (_, owned) = borrow_rejection_unit::checked(
            text,
            "mutable-alias.ko",
            rooted,
            standard_environments(),
        );
        assert!(
            owned.diagnostics().is_empty(),
            "{rooted}: {:?}",
            owned.diagnostics()
        );
        assert!(owned.validate().is_ok());
    }
}

#[test]
fn rejected_rhs_restores_the_previous_mutable_escape_witness() {
    let text = "fun read(xs: List<Int>) {}\nfun consume(own xs: List<Int>) {}\nfun run(own xs: List<Int>, flag: Boolean): () -> Unit { val f: () -> Unit = { read(xs) }; var optional = if (flag) f else null; optional = if (flag) { consume(xs); consume(xs); null } else null; return optional!! }";
    for rooted in [false, true] {
        let (sources, owned) = borrow_rejection_unit::checked(
            text,
            "mutable-alias.ko",
            rooted,
            standard_environments(),
        );
        assert!(
            owned
                .diagnostics()
                .iter()
                .any(|d| d.code().to_string() == "L0131"),
            "{rooted}: {:?}",
            owned.diagnostics()
        );
        let escape = owned
            .diagnostics()
            .iter()
            .find(|d| d.code().to_string() == "L0137")
            .expect("failed RHS must restore the old alias evidence");
        assert_eq!(sources.slice(escape.primary_span()), Ok("optional!!"));
        assert!(owned.validate().is_err());
    }
}

#[test]
fn unknown_branch_does_not_erase_a_reachable_borrowed_value() {
    for body in [
        "var optional=if(flag) f else null; if(again) { optional=make() }; return optional!!",
        "var optional=if(flag) f else null; if(again) {} else { optional=make() }; return optional!!",
        "var optional=if(flag) ({}) else null; optional=make(); optional=if(flag) f else null; return optional!!",
        "val borrowed=if(flag) f else null; var optional=if(again) make() else borrowed; return optional!!",
        "val borrowed=if(flag) f else null; var optional=if(flag) ({}) else null; optional=if(again) make() else borrowed; return (when(again) { true -> optional; else -> null })!!",
    ] {
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun make(): () -> Unit {{ return ({{}}) }}\nfun run(own xs: List<Int>, flag:Boolean, again:Boolean): () -> Unit {{ val f: () -> Unit = {{ read(xs) }}; {body} }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "mutable-unknown-join.ko",
                rooted,
                standard_environments(),
            );
            assert_eq!(
                owned
                    .diagnostics()
                    .iter()
                    .map(|d| d.code().to_string())
                    .collect::<Vec<_>>(),
                ["L0137"],
                "{body}/{rooted}: {:?}",
                owned.diagnostics()
            );
            assert!(owned.non_null_assertions().is_empty());
            assert!(owned.loans().is_empty());
            assert!(owned.drops().is_empty());
            assert!(owned.validate().is_err());
        }
    }
}

#[test]
fn mutable_control_result_wrappers_preserve_replacement_and_rollback() {
    for result in [
        "((optional))!!",
        "(if(again) optional else null)!!",
        "(when(again) { true -> optional; else -> null })!!",
    ] {
        for (update, expected) in [
            ("", vec!["L0137"]),
            ("optional=null;", vec![]),
            ("optional=if(flag) ({}) else null;", vec![]),
            (
                "if(again) { optional=make() } else { optional=null };",
                vec![],
            ),
            (
                "optional=if(flag) { consume(xs); consume(xs); null } else null;",
                vec!["L0131", "L0137"],
            ),
        ] {
            let text = format!(
                "fun read(xs: List<Int>) {{}}\nfun consume(own xs: List<Int>) {{}}\nfun make(): () -> Unit {{ return ({{}}) }}\nfun run(own xs: List<Int>, flag:Boolean, again:Boolean): () -> Unit {{ val f: () -> Unit = {{ read(xs) }}; var optional=if(flag) f else null; {update} return {result} }}"
            );
            for rooted in [false, true] {
                let (_, owned) = borrow_rejection_unit::checked(
                    &text,
                    "mutable-control.ko",
                    rooted,
                    standard_environments(),
                );
                let codes = owned
                    .diagnostics()
                    .iter()
                    .map(|d| d.code().to_string())
                    .collect::<Vec<_>>();
                assert_eq!(
                    codes,
                    expected,
                    "{update}/{result}/{rooted}: {:?}",
                    owned.diagnostics()
                );
                if !expected.is_empty() {
                    assert!(owned.loans().is_empty());
                    assert!(owned.drops().is_empty());
                    assert!(owned.non_null_assertions().is_empty());
                }
                assert_eq!(owned.validate().is_ok(), expected.is_empty());
            }
        }
    }
}

#[path = "support/loop_null_clear_escape.rs"]
mod loop_null_clear_escape;
