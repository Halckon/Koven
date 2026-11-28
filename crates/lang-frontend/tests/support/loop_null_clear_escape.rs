//! Bounded post-loop diagnostics: real no-backedge flows and checked null writes.
use super::borrow_rejection_unit;
use lang_frontend::type_checking::standard_environments;

#[test]
fn loop_mutable_alias_preserves_zero_iteration_escape_rejection() {
    let mut failures = Vec::new();
    for (label, borrowed, body, reject) in [
        (
            "base",
            true,
            "var optional = if (choose) f else null; return optional!!",
            true,
        ),
        (
            "zero_iteration",
            true,
            "var optional = if (choose) f else null; while (iterate) { optional = null; break }; return optional!!",
            true,
        ),
        (
            "definite_overwrite",
            true,
            "var optional = if (choose) f else null; loop { optional = null; break }; return optional!!",
            false,
        ),
        (
            "capture_free",
            false,
            "var optional = if (choose) f else null; while (iterate) { optional = null; break }; return optional!!",
            false,
        ),
    ] {
        let lambda = if borrowed { "{ read(xs) }" } else { "{}" };
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, choose: Boolean, iterate: Boolean): () -> Unit {{ val f: () -> Unit = {lambda}; {body} }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "loop-alias.ko",
                rooted,
                standard_environments(),
            );
            let codes: Vec<_> = owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect();
            let raw = format!("{:?}", owned.diagnostics());
            let valid = owned.validate().is_ok();
            eprintln!("CASE {label} rooted={rooted} SOURCE={text:?} RAW={raw} VALID={valid}");
            let expected = if reject {
                codes == ["L0137"] && !valid
            } else {
                codes.is_empty() && valid
            };
            if !expected {
                failures.push(format!("{label}/{rooted}: {raw}; validate={valid}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn terminal_loop_branches_keep_only_reachable_entry_or_break_values() {
    for (body, reject) in [
        (
            "var optional = if (choose) f else null; loop { if (iterate) { optional = null; break } else { break } }; return optional!!",
            true,
        ),
        (
            "var optional = if (choose) f else null; loop { if (iterate) { optional = null; break } else { optional = (null); break } }; return optional!!",
            false,
        ),
        (
            "var optional = if (choose) f else null; loop { if (iterate) { optional = null; break } else { return ({}) } }; return optional!!",
            false,
        ),
        (
            "var optional = if (choose) f else null; while (iterate) { optional = null; return ({}) }; return optional!!",
            true,
        ),
        (
            "var optional = if (choose) f else null; loop { while (iterate) { optional = null; break }; break }; return optional!!",
            true,
        ),
        (
            "var optional = if (choose) f else null; loop { optional = null; break }; optional = if (choose) ({ read(xs) }) else null; return optional!!",
            true,
        ),
        (
            "var optional = if (choose) f else null; if (iterate) { loop { optional = null; break } }; return optional!!",
            true,
        ),
        (
            "var optional = if (choose) f else null; if (iterate) { loop { optional = null; break } } else { optional = null }; return optional!!",
            false,
        ),
        (
            "var optional = if (choose) f else null; while (if (iterate) { return ({}) } else false) { optional = null; break }; return optional!!",
            true,
        ),
        (
            "var optional = if (choose) f else null; if (iterate) {} else { loop { optional = null; break } }; return optional!!",
            true,
        ),
        (
            "var optional = if (choose) f else null; loop { while (if (iterate) { break } else false) { optional = null; break }; optional = null; break }; return optional!!",
            true,
        ),
    ] {
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun run(own xs: List<Int>, choose: Boolean, iterate: Boolean): () -> Unit {{ val f: () -> Unit = {{ read(xs) }}; {body} }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "loop-branches.ko",
                rooted,
                standard_environments(),
            );
            let codes: Vec<_> = owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect();
            let raw = format!("{:?}", owned.diagnostics());
            let valid = owned.validate().is_ok();
            eprintln!("BRANCH rooted={rooted} SOURCE={text:?} RAW={raw} VALID={valid}");
            if reject {
                assert_eq!(codes, ["L0137"], "{body}/{rooted}: {raw}");
                assert!(!valid);
            } else {
                assert!(codes.is_empty(), "{body}/{rooted}: {raw}");
                assert!(valid);
            }
        }
    }
}

#[test]
fn failed_loop_write_retains_original_error_before_any_shadow_publication() {
    let text = "fun read(xs: List<Int>) {}\nfun consume(own xs: List<Int>) {}\nfun run(own xs: List<Int>, choose:Boolean, iterate:Boolean): () -> Unit { val f: () -> Unit = { read(xs) }; var optional=if(choose) f else null; while(iterate) { optional=if(choose) { consume(xs); consume(xs); null } else null; break }; return optional!! }";
    for rooted in [false, true] {
        let (_, owned) = borrow_rejection_unit::checked(
            text,
            "loop-failed-write.ko",
            rooted,
            standard_environments(),
        );
        let codes: Vec<_> = owned
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect();
        assert_eq!(codes, ["L0131"], "{rooted}: {:?}", owned.diagnostics());
        assert!(owned.loans().is_empty());
        assert!(owned.drops().is_empty());
        assert!(owned.non_null_assertions().is_empty());
        assert!(owned.validate().is_err());
    }
}

#[test]
fn pre_loop_copy_and_unknown_branch_preserve_borrowed_escape_rejection() {
    for (label, borrowed, body, reject) in [
        (
            "pre_loop_copy_before_while_clear",
            true,
            "var optional = if (choose) f else null; val saved = optional; while (iterate) { optional = null; break }; return saved!!",
            true,
        ),
        (
            "pre_loop_copy_before_definite_loop_clear",
            true,
            "var optional = if (choose) f else null; val saved = optional; loop { optional = null; break }; return saved!!",
            true,
        ),
        (
            "pre_loop_copy_after_pre_loop_clear_is_safe",
            true,
            "var optional = if (choose) f else null; optional = null; val saved = optional; while (iterate) { optional = null; break }; return saved!!",
            false,
        ),
        (
            "pre_loop_copy_capture_free_is_safe",
            false,
            "var optional = if (choose) f else null; val saved = optional; while (iterate) { optional = null; break }; return saved!!",
            false,
        ),
        (
            "unknown_branch_then_zero_iteration_while",
            true,
            "var optional = if (choose) f else null; if (change) { optional = make() }; while (iterate) { optional = null; break }; return optional!!",
            true,
        ),
        (
            "unknown_else_branch_then_zero_iteration_while",
            true,
            "var optional = if (choose) f else null; if (change) {} else { optional = make() }; while (iterate) { optional = null; break }; return optional!!",
            true,
        ),
        (
            "unknown_branch_then_definite_loop_clear_is_safe",
            true,
            "var optional = if (choose) f else null; if (change) { optional = make() }; loop { optional = null; break }; return optional!!",
            false,
        ),
        (
            "unknown_branch_then_while_then_post_loop_clear_is_safe",
            true,
            "var optional = if (choose) f else null; if (change) { optional = make() }; while (iterate) { optional = null; break }; optional = null; return optional!!",
            false,
        ),
        (
            "mixed_unknown_and_borrowed_init_then_zero_iteration_while",
            true,
            "val borrowed = if (choose) f else null; var optional = if (change) make() else borrowed; while (iterate) { optional = null; break }; return optional!!",
            true,
        ),
        (
            "conditional_unsupported_loop_branch_preserves_skipped_borrowed_path",
            true,
            "var optional = if (choose) f else null; if (change) { loop { optional = ({}); break } }; return optional!!",
            true,
        ),
        (
            "conditional_unsupported_loop_else_branch_preserves_skipped_borrowed_path",
            true,
            "var optional = if (choose) f else null; if (change) {} else { loop { optional = ({}); break } }; return optional!!",
            true,
        ),
        (
            "conditional_unsupported_loop_then_explicit_null_clear_is_safe",
            true,
            "var optional = if (choose) f else null; if (change) { loop { optional = ({}); break } }; optional = null; return optional!!",
            false,
        ),
        (
            "unconditional_unsupported_loop_overwrite_is_not_false_positive",
            true,
            "var optional = if (choose) f else null; loop { optional = ({}); break }; return optional!!",
            false,
        ),
        (
            "elvis_return_rhs_preserves_non_null_borrowed_continuation_var",
            true,
            "var optional = if (choose) f else null; val selected = optional ?: return ({}); return selected",
            true,
        ),
        (
            "elvis_return_rhs_preserves_non_null_borrowed_continuation_val",
            true,
            "val optional = if (choose) f else null; val selected = optional ?: return ({}); return selected",
            true,
        ),
        (
            "elvis_return_rhs_capture_free_is_safe",
            false,
            "val optional = if (choose) f else null; val selected = optional ?: return ({}); return selected",
            false,
        ),
    ] {
        let lambda = if borrowed { "{ read(xs) }" } else { "{}" };
        let text = format!(
            "fun read(xs: List<Int>) {{}}\nfun make(): () -> Unit {{ return ({{}}) }}\nfun run(own xs: List<Int>, choose: Boolean, change: Boolean, iterate: Boolean): () -> Unit {{ val f: () -> Unit = {lambda}; {body} }}"
        );
        for rooted in [false, true] {
            let (_, owned) = borrow_rejection_unit::checked(
                &text,
                "loop-p2.ko",
                rooted,
                standard_environments(),
            );
            let codes: Vec<_> = owned
                .diagnostics()
                .iter()
                .map(|d| d.code().to_string())
                .collect();
            let raw = format!("{:?}", owned.diagnostics());
            let valid = owned.validate().is_ok();
            if reject {
                assert_eq!(codes, ["L0137"], "{label}/{rooted}: {raw}");
                assert!(!valid, "{label}/{rooted}");
            } else {
                assert!(codes.is_empty(), "{label}/{rooted}: {raw}");
                assert!(valid, "{label}/{rooted}");
            }
        }
    }
}
