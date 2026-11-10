use super::*;

#[path = "rejected_call_recovery/call_writeback.rs"]
mod call_writeback;
#[path = "rejected_call_recovery/closure_writeback.rs"]
mod closure_writeback;
#[path = "rejected_call_recovery/external_constraints.rs"]
mod external_constraints;
#[path = "rejected_call_recovery/primitive_writeback.rs"]
mod primitive_writeback;
#[path = "rejected_call_recovery/writable_roots.rs"]
mod writable_roots;
use lang_frontend::type_checking::{FinalTypeAdmissionError, complete_compilation_unit_types};

#[test]
fn pending_lambda_with_known_non_callable_parameter_reports_mismatch() {
    for parameter in ["Carrier<T>", "Carrier<T>?", "Int"] {
        assert_call_recovery(
            &format!("fun <T> take(value: T, callback: {parameter}): Int = 0"),
            "take(outside(), { inside() })",
            Some(("L0084", "{ inside() }")),
            &[("L0165", "outside"), ("L0084", "{ inside() }")],
            None,
        );
    }
}

#[test]
fn poisoned_parameter_does_not_add_lambda_shape_error() {
    assert_call_recovery(
        "fun <T> take(value: T, callback: Int<String>): Int = 0",
        "take(outside(), { inside() })",
        Some(("L0082", "String")),
        &[("L0082", "String"), ("L0165", "outside")],
        None,
    );
}

#[test]
fn poisoned_explicit_type_argument_still_checks_ordinary_arguments() {
    assert_call_recovery(
        "fun <T> take(value: T): Int = 0",
        "take<Int<String>>(1)",
        Some(("L0082", "String")),
        &[("L0082", "String")],
        None,
    );
    assert_call_recovery(
        "fun <T> take(value: T, cb: (borrow Int) -> Int): Int = 0",
        "take<Int<String>>(outside(), { item -> inside() })",
        Some(("L0082", "String")),
        &[("L0082", "String"), ("L0165", "outside")],
        None,
    );
    assert_call_recovery(
        "fun <T> take(value: T): Int = 0\nfun keep(value: Int): Int = value",
        "take<Int<String>>(keep(1))",
        Some(("L0082", "String")),
        &[("L0082", "String")],
        Some("keep(1)"),
    );
    assert_call_recovery(
        "fun <T> take(value: T): Int = 0\n\
         fun <T> take(value: String): Int = 1\n\
         fun keep(value: Int): Int = value",
        "take<Int<String>>(keep(1))",
        Some(("L0082", "String")),
        &[("L0082", "String")],
        Some("keep(1)"),
    );
}

#[test]
fn rejected_nominal_storage_is_diagnosed_once_and_keeps_independent_arguments() {
    assert_call_recovery(
        "interface Pattern<T>\n\
         class Marker<T>(val item: Int) : Pattern<Rc<T>>\n\
         fun keep(value: Int): Int = value\n\
         fun <T> take(value: T, independent: Int): Int = 0",
        "take<Marker<Nothing>>(Marker<Nothing>(keep(1)), outside())",
        Some(("L0125", "Marker<Nothing>")),
        &[("L0125", "Marker<Nothing>"), ("L0165", "outside")],
        Some("keep(1)"),
    );
}

#[test]
fn storage_diagnostic_reuse_is_scoped_to_the_same_nominal_instance() {
    let declarations = "interface Pattern<T>\n\
        class Marker<T> : Pattern<Rc<T>>\n\
        class Second<T> : Pattern<Rc<T>>\n\
        fun <T> take(value: T): Int = 0";
    let diagnostics = &[("L0125", "Marker<Nothing>"), ("L0125", "Nothing")];
    assert_call_recovery_with_raw(
        declarations,
        "= take<Marker<Nothing>>(Second<Nothing>())",
        &["take<Marker<Nothing>>(Second<Nothing>())"],
        diagnostics,
        diagnostics,
        None,
    );
    assert_call_recovery_with_raw(
        declarations,
        "{ val first = take<Marker<Nothing>>(Marker<Nothing>());\n\
           val second = Marker<Nothing>(); return 0 }",
        &["take<Marker<Nothing>>(Marker<Nothing>())"],
        diagnostics,
        diagnostics,
        None,
    );
}

#[test]
fn reused_storage_error_prevents_publication_through_string_and_lambda() {
    for (overloads, nested) in [
        (
            "fun keep(value: String): Int = 0",
            r#"keep("${Marker<Nothing>()}")"#,
        ),
        (
            "fun keep(value: (borrow Int) -> Int): Int = 0\n\
             fun keep(value: (borrow Boolean) -> Boolean): Int = 1",
            "keep({ item -> Marker<Nothing>() })",
        ),
    ] {
        let declarations = format!(
            "interface Pattern<T>\nclass Marker<T> : Pattern<Rc<T>>\n\
             {overloads}\nfun <T> take(value: T): Int = 0"
        );
        let call = format!("take<Marker<Nothing>>({nested})");
        let diagnostic = &[("L0125", "Marker<Nothing>")];
        assert_call_recovery_with_raw(
            &declarations,
            &format!("= {call}"),
            &[&call, nested],
            diagnostic,
            diagnostic,
            None,
        );
    }
}

#[test]
fn rejected_construction_still_checks_explicit_type_references() {
    let declarations = "interface Pattern<T>\nclass Marker<T>(val item: Int) : Pattern<Rc<T>>\n\
        class Wrapper<T, U>(val item: Int)\nfun <T> take(value: T): Int = 0";
    for (nested, diagnostics) in [
        (
            "Marker<Nothing>(wrong = 1)",
            &[("L0125", "Marker<Nothing>"), ("L0120", "wrong")][..],
        ),
        (
            "Wrapper<Int<String>, Int>(1)",
            &[("L0125", "Marker<Nothing>"), ("L0082", "String")][..],
        ),
        (
            "Rc<Int>(wrong = 1)",
            &[("L0125", "Marker<Nothing>"), ("L0120", "wrong")][..],
        ),
        (
            "listOf<Int, String>(1)",
            &[
                ("L0125", "Marker<Nothing>"),
                ("L0091", "listOf<Int, String>(1)"),
            ][..],
        ),
        (
            "mapOf<Int>(1)",
            &[("L0125", "Marker<Nothing>"), ("L0091", "mapOf<Int>(1)")][..],
        ),
        (
            "mapOf<Int, Int>(1)",
            &[
                ("L0125", "Marker<Nothing>"),
                ("L0121", "mapOf<Int, Int>(1)"),
            ][..],
        ),
    ] {
        let call = format!("take<Marker<Nothing>>({nested})");
        assert_call_recovery_with_raw(
            declarations,
            &format!("= {call}"),
            &[&call, nested],
            diagnostics,
            diagnostics,
            None,
        );
    }
}

#[test]
fn reused_storage_error_prevents_enclosing_construction_publication() {
    let declarations = "interface Pattern<T>\nclass Marker<T> : Pattern<Rc<T>>\n\
        class Holder(val value: String)\nclass Wrapper<T>(val value: T)\n\
        fun <T> take(value: T): Int = 0";
    let diagnostic = &[("L0125", "Marker<Nothing>")];
    for nested in [
        r#"Holder("${Marker<Nothing>()}")"#,
        r#"Wrapper("${Marker<Nothing>()}")"#,
        r#"Rc("${Marker<Nothing>()}")"#,
        r#"listOf("${Marker<Nothing>()}")"#,
        r#"Array<String>(1, { index -> "${Marker<Nothing>()}" })"#,
        r#""${Marker<Nothing>()}".clone()"#,
    ] {
        let call = format!("take<Marker<Nothing>>({nested})");
        assert_call_recovery_with_raw(
            declarations,
            &format!("= {call}"),
            &[&call, nested],
            diagnostic,
            diagnostic,
            None,
        );
    }
}

#[test]
fn rejected_storage_reuse_keeps_independent_arity_and_mapping_errors() {
    for (call, independent) in [
        (
            "take<Marker<Nothing>, Int>(Marker<Nothing>())",
            ("L0091", "Int"),
        ),
        (
            "take<Marker<Nothing>>(wrong = Marker<Nothing>())",
            ("L0120", "wrong"),
        ),
    ] {
        let diagnostics = &[("L0125", "Marker<Nothing>"), independent];
        assert_call_recovery_with_raw(
            "interface Pattern<T>\nclass Marker<T> : Pattern<Rc<T>>\nfun <T> take(value: T): Int = 0",
            &format!("= {call}"),
            &[call],
            diagnostics,
            diagnostics,
            None,
        );
    }
}

#[test]
fn overloads_rejected_for_storage_preserve_unresolved_ordinary_argument() {
    assert_call_recovery(
        "fun <T> store(value: List<Carrier<T>>): Int = 0\n\
         fun <T> store(value: Array<Carrier<T>>): Int = 1",
        "store<Nothing>(outside())",
        Some(("L0123", "store")),
        &[("L0123", "store"), ("L0165", "outside")],
        None,
    );
}

#[test]
fn overloads_rejected_for_storage_preserve_independent_selected_call() {
    assert_call_recovery(
        "fun <T> store(value: List<Carrier<T>>): Int = 0\n\
         fun <T> store(value: Array<Carrier<T>>): Int = 1\n\
         fun keep(value: Int): Int = value",
        "store<Nothing>(keep(1))",
        Some(("L0123", "store")),
        &[("L0123", "store")],
        Some("keep(1)"),
    );
}

#[test]
fn member_overloads_rejected_before_lambda_typing_keep_final_diagnostics() {
    assert_call_recovery(
        "class Factory {\n\
             fun <T> store(value: List<Carrier<T>>, cb: (borrow Int) -> Int): Int = 0\n\
             fun <T> store(value: Array<Carrier<T>>, cb: (borrow Int) -> Int): Int = 1\n\
         }",
        "Factory().store<Nothing>(outside(), { item -> inside() })",
        Some(("L0123", "Factory().store")),
        &[("L0123", "Factory().store"), ("L0165", "outside")],
        None,
    );
}

#[test]
fn member_overloads_rejected_by_known_argument_leave_lambda_uncommitted() {
    assert_call_recovery(
        "class Factory {\n\
             fun route(value: Int, cb: (borrow Int) -> Int): Int = 0\n\
             fun route(value: String, cb: (borrow Int) -> Int): Int = 1\n\
         }",
        "Factory().route(true, { item -> inside() })",
        Some(("L0123", "Factory().route")),
        &[("L0123", "Factory().route")],
        None,
    );
}

#[test]
fn member_lambda_trial_rollback_keeps_source_diagnostics() {
    for (second_result, body, code) in [("Boolean", "\"no\"", "L0123"), ("Int", "1", "L0124")] {
        assert_call_recovery(
            &format!(
                "class Factory {{\n\
                     fun route(value: Int, cb: (borrow Int) -> Int): Int = 0\n\
                     fun route(value: Int, cb: (borrow Boolean) -> {second_result}): Int = 1\n\
                 }}"
            ),
            &format!("Factory().route(0, {{ item -> {body} }})"),
            Some((code, "Factory().route")),
            &[(code, "Factory().route")],
            None,
        );
    }
}

#[test]
fn bound_external_lambda_trial_rollback_keeps_source_diagnostics() {
    assert_call_recovery(
        "",
        "externalRoute(0, { item -> \"no\" })",
        Some(("L0123", "externalRoute")),
        &[("L0123", "externalRoute")],
        None,
    );
}

#[test]
fn ordinary_argument_precheck_error_does_not_consume_uncommitted_lambda() {
    for (declarations, callee) in [
        (
            "fun route(value: String, cb: (borrow Int) -> Int): Int = 0\n\
             fun route(value: String, cb: (borrow Boolean) -> Boolean): Int = 1",
            "route",
        ),
        (
            "class Factory {\n\
                 fun route(value: String, cb: (borrow Int) -> Int): Int = 0\n\
                 fun route(value: String, cb: (borrow Boolean) -> Boolean): Int = 1\n\
             }",
            "Factory().route",
        ),
    ] {
        let call = format!(r#"{callee}("${{if (true) 7 else false}}", {{ item -> inside() }})"#);
        assert_call_recovery(
            declarations,
            &call,
            Some(("L0089", "else")),
            &[("L0089", "else")],
            None,
        );
    }
}

#[test]
fn inference_precheck_error_does_not_add_overload_failure() {
    for declarations in [
        "fun <T> route(value: T, cb: (borrow Int) -> Int): Int = 0",
        "fun <T> route(value: T, cb: (borrow Int) -> Int): Int = 0\n\
         fun <T> route(value: T, cb: (borrow Boolean) -> Boolean): Int = 1",
    ] {
        assert_call_recovery(
            declarations,
            "route(if (true) 7 else false, { item -> inside() })",
            Some(("L0089", "else")),
            &[("L0089", "else")],
            None,
        );
    }
}

#[test]
fn repeated_pending_parameter_rejects_inconsistent_invariant_positions() {
    assert_call_recovery(
        "fun <T> take(value: Pair<T, T>): Int = 0",
        "take<Unbound>(Pair<Int, Boolean>(1, true))",
        Some(("L0084", "Pair<Int, Boolean>(1, true)")),
        &[
            ("L0165", "Unbound"),
            ("L0084", "Pair<Int, Boolean>(1, true)"),
        ],
        Some("Pair<Int, Boolean>(1, true)"),
    );
}

#[test]
fn repeated_pending_parameter_rejects_inconsistent_arguments() {
    assert_call_recovery(
        "fun <T> take(first: Carrier<T>, second: Carrier<T>): Int = 0",
        "take<Unbound>(Carrier<Int>(1), Carrier<Boolean>(true))",
        Some(("L0084", "Carrier<Boolean>(true)")),
        &[("L0165", "Unbound"), ("L0084", "Carrier<Boolean>(true)")],
        None,
    );
    assert_call_recovery(
        "fun <T> take(first: T, second: T): Int = 0",
        "take<Unbound>(1, true)",
        Some(("L0084", "true")),
        &[("L0165", "Unbound"), ("L0084", "true")],
        None,
    );
}

#[test]
fn repeated_pending_inout_parameter_requires_identical_storage_types() {
    assert_call_recovery_in_entry(
        "fun <T> take(inout first: T, inout second: T): Int = 0",
        "{ var x: Int = 1; var y: Int? = null; return take<Unbound>(&x, &y) }",
        "take<Unbound>(&x, &y)",
        Some(("L0084", "y")),
        &[("L0165", "Unbound"), ("L0084", "y")],
        None,
    );
}

#[test]
fn separate_pending_parameters_and_compatible_uses_stay_pending() {
    for (declarations, call, diagnostics) in [
        (
            "fun <T, U> take(value: Pair<T, U>): Int = 0",
            "take<Unbound, Other>(Pair<Int, Boolean>(1, true))",
            &[("L0165", "Unbound"), ("L0165", "Other")][..],
        ),
        (
            "fun <T> take(first: T, second: T): Int = 0",
            "take<Unbound>(1, null)",
            &[("L0165", "Unbound")][..],
        ),
    ] {
        assert_call_recovery(declarations, call, None, diagnostics, None);
    }
}

#[test]
fn unresolved_explicit_arguments_preserve_context_dependent_if_and_list_forms() {
    for (declarations, call, diagnostic) in [
        (
            "fun <T> take(value: T): Int = 0",
            "take<Unbound>(if (true) 1L else 2)",
            None,
        ),
        (
            "fun <T> take(value: List<T>): Int = 0",
            "take<Unbound>(listOf())",
            None,
        ),
        (
            "fun <T> take(value: List<T>): Int = 0\n\
             fun <T> take(value: Array<T>): Int = 1",
            "take<Unbound>(listOf())",
            None,
        ),
        (
            "fun <T> take(value: Array<T>): Int = 0",
            "take<Unbound>(listOf())",
            Some(("L0084", "listOf()")),
        ),
        (
            "fun <T> take(value: T): Int = 0",
            "take<Unbound>(if (true) 1L else true)",
            Some(("L0089", "else")),
        ),
        (
            "fun <T> take(value: T): Int = 0",
            "take<Unbound>(if (true) (if (true) 1L else 2) else true)",
            Some(("L0089", "else")),
        ),
        (
            "fun <T> take(value: T): Int = 0",
            "take<Unbound>(if (true) 1L else (if (true) 2 else 3))",
            None,
        ),
        (
            "fun <T> take(value: T): Int = 0",
            "take<Unbound>(if (true) listOf() else listOf<Long>(1L))",
            None,
        ),
        (
            "fun <T> take(value: T): Int = 0\n\
             fun left(): Long = 1L\nfun right(): Int = 2",
            "take<Unbound>(if (true) left() else right())",
            Some(("L0089", "else")),
        ),
        (
            "fun <T> take(value: T): Int = 0\nfun other(): Boolean? = null",
            "take<Unbound>(if (true) (if (true) 1L else 2) else other())",
            Some(("L0089", "else")),
        ),
        (
            "fun <T> take(value: T): Int = 0",
            "take<Unbound>(if (true) (if (true) 1L else 2) else null)",
            Some(("L0089", "else")),
        ),
    ] {
        let mut diagnostics = vec![("L0165", "Unbound")];
        diagnostics.extend(diagnostic);
        let mut rejected = vec![call];
        if call.contains("listOf()") {
            rejected.push("listOf()");
        }
        assert_call_recovery_with_raw(
            declarations,
            &format!("= {call}"),
            &rejected,
            diagnostic.as_slice(),
            &diagnostics,
            None,
        );
    }
    for (first, second, argument) in [
        ("Int", "Boolean", "if (true) 1L else 2"),
        ("Long?", "Boolean", "if (true) 1L else 2"),
        (
            "Long",
            "Boolean",
            "if (true) (if (true) 1 else 2) else null",
        ),
        (
            "List<Int>",
            "List<Boolean>",
            "if (true) listOf() else listOf<Long>(1L)",
        ),
    ] {
        assert_call_recovery(
            &format!(
                "fun <T> take(value: {first}, rest: T): Int = 0\n\
                 fun <T> take(value: {second}, rest: T): Int = 1"
            ),
            &format!("take<Unbound>({argument}, true)"),
            Some(("L0123", "take")),
            &[("L0123", "take"), ("L0165", "Unbound")],
            None,
        );
    }
    assert_call_recovery_with_raw(
        "fun <T> take(value: T): Int = 0",
        "= take<Unbound>(if (true) { outside(); 1L } else 2)",
        &["take<Unbound>(if (true) { outside(); 1L } else 2)"],
        &[],
        &[("L0165", "Unbound"), ("L0165", "outside")],
        None,
    );
}

#[test]
fn unresolved_explicit_arguments_preserve_all_when_branch_constraints() {
    for branch in ["2", "fixed()"] {
        let call = format!("take<Unbound>(when (1) {{ 0 -> 1; 1 -> {branch}; else -> 1L }})");
        let conflict = (branch == "fixed()").then_some(("L0112", "1L"));
        let mut diagnostics = vec![("L0165", "Unbound")];
        diagnostics.extend(conflict);
        assert_call_recovery(
            "fun <T> take(value: T): Int = 0\nfun fixed(): Int = 2",
            &call,
            conflict,
            &diagnostics,
            None,
        );
    }
    for (last, conflict) in [
        ("listOf( )", None),
        ("listOf<Int>(2)", Some(("L0112", "listOf<Int>(2)"))),
    ] {
        let call = format!(
            "take<Unbound>(when (1) {{ 0 -> listOf(); 1 -> listOf<Long>(1L); else -> {last} }})"
        );
        let mut diagnostics = vec![("L0165", "Unbound")];
        diagnostics.extend(conflict);
        let mut rejected = vec![call.as_str(), "listOf()"];
        if conflict.is_none() {
            rejected.push(last);
        }
        assert_call_recovery_with_raw(
            "fun <T> take(value: List<T>): Int = 0",
            &format!("= {call}"),
            &rejected,
            conflict.as_slice(),
            &diagnostics,
            Some("listOf<Long>(1L)"),
        );
    }
    assert_call_recovery(
        "fun <T> take(value: List<Int>, rest: T): Int = 0\n\
         fun <T> take(value: List<Boolean>, rest: T): Int = 1",
        "take<Unbound>(when (1) { 0 -> listOf(); 1 -> listOf<Long>(1L); else -> listOf() }, true)",
        Some(("L0123", "take")),
        &[("L0123", "take"), ("L0165", "Unbound")],
        None,
    );
    for (branches, primary) in [
        ("0 -> 1; 1 -> 1L; else -> null", "null"),
        ("0 -> null; 1 -> 1; else -> 1L", "1L"),
    ] {
        assert_call_recovery(
            "fun <T> take(value: T): Int = 0",
            &format!("take<Unbound>(when (1) {{ {branches} }})"),
            Some(("L0112", primary)),
            &[("L0165", "Unbound"), ("L0112", primary)],
            None,
        );
    }
    for (branches, primary) in [
        (
            "1 -> listOf(); 2 -> listOf<Long>(1L); else -> listOf<Int>(2)",
            "listOf<Int>(2)",
        ),
        ("1 -> 1L; else -> false", "false"),
    ] {
        assert_call_recovery(
            "fun <T> take(value: T): Int = 0\nfun <T> pending(): Int = 0",
            &format!("take<Unbound>(when (1) {{ 0 -> pending<Other>(); {branches} }})"),
            Some(("L0112", primary)),
            &[("L0165", "Unbound"), ("L0165", "Other"), ("L0112", primary)],
            None,
        );
    }
    assert_call_recovery(
        "fun <T> take(value: T): Int = 0\nfun <T> pending(): Int = 0",
        "take<Unbound>(when (1) { 0 -> listOf<Long>(1L); 1 -> pending<Other>(); 2 -> listOf(); else -> listOf<Int>(2) })",
        Some(("L0112", "listOf<Int>(2)")),
        &[
            ("L0165", "Unbound"),
            ("L0165", "Other"),
            ("L0112", "listOf<Int>(2)"),
        ],
        None,
    );
    assert_call_recovery(
        "fun <T> take(value: List<Int>, rest: T): Int = 0\n\
         fun <T> take(value: Boolean, rest: T): Int = 1\nfun <T> pending(): Int = 0",
        "take<Unbound>(when (1) { 0 -> listOf<Long>(1L); else -> pending<Other>() }, true)",
        Some(("L0123", "take")),
        &[("L0123", "take"), ("L0165", "Unbound"), ("L0165", "Other")],
        None,
    );
    assert_call_recovery(
        "fun <T> take(value: Long, rest: T): Int = 0\n\
         fun <T> take(value: Boolean, rest: T): Int = 1",
        "take<Unbound>(when (1) { else -> 2 }, true)",
        None,
        &[("L0165", "Unbound")],
        None,
    );
}

#[test]
fn correlation_rejects_overload_before_contextual_lambda_check() {
    assert_call_recovery(
        "fun <T> take(value: Pair<T, T>, cb: (borrow Boolean) -> Boolean): Int = 0\n\
         fun <T> take(value: Pair<Int, T>, cb: (borrow Int) -> Int): Int = 1\n\
         fun keep(value: Int): Int = value",
        "take<Unbound>(Pair<Int, Boolean>(1, true), { item -> keep(item) })",
        None,
        &[("L0165", "Unbound")],
        Some("keep(item)"),
    );
}

#[test]
fn correlation_rejects_all_pending_overloads_without_lambda_arguments() {
    assert_call_recovery(
        "fun <T> take(value: Pair<T, T>, extra: T): Int = 0\n\
         fun <T> take(value: Pair<T, T>, extra: Int): Int = 1",
        "take<Unbound>(Pair<Int, Boolean>(1, true), 1)",
        Some(("L0123", "take")),
        &[("L0123", "take"), ("L0165", "Unbound")],
        Some("Pair<Int, Boolean>(1, true)"),
    );
}

#[test]
fn complementary_partial_argument_types_still_prove_conflict() {
    assert_call_recovery_in_entry(
        "class Triple<A, B, C>(val a: A, val b: B, val c: C)\n\
         fun <T> take(first: Carrier<T>?, second: Carrier<T>?, third: Carrier<T>?): Int = 0",
        "{ val first: Carrier<Triple<Int, Unbound, Other>>? = null;\n\
           val second: Carrier<Triple<Other, Boolean, Unbound>>? = null;\n\
           val third: Carrier<Triple<Unbound, String, Other>>? = null;\n\
           return take(first, second, third) }",
        "take(first, second, third)",
        Some(("L0084", "third")),
        &[
            ("L0165", "Unbound"),
            ("L0165", "Other"),
            ("L0165", "null"),
            ("L0165", "Other"),
            ("L0165", "Unbound"),
            ("L0165", "null"),
            ("L0165", "Unbound"),
            ("L0165", "Other"),
            ("L0165", "null"),
            ("L0084", "third"),
        ],
        None,
    );
}

#[test]
fn repeated_pending_owner_parameter_keeps_its_identity() {
    assert_call_recovery_in_entry(
        "class Owner<T> { fun take(pair: Pair<T, T>): Int = 0 }",
        "{ val owner: Owner<Unbound>? = null;\n\
           if (owner != null) { return owner.take(Pair<Int, Boolean>(1, true)) };\n\
           return 0 }",
        "owner.take(Pair<Int, Boolean>(1, true))",
        Some(("L0084", "Pair<Int, Boolean>(1, true)")),
        &[
            ("L0165", "Unbound"),
            ("L0165", "null"),
            ("L0165", "null"),
            ("L0084", "Pair<Int, Boolean>(1, true)"),
        ],
        Some("Pair<Int, Boolean>(1, true)"),
    );
}

#[test]
fn repeated_parameter_respects_one_nullable_promotion() {
    for (actual, raw, final_diagnostics) in [
        (
            "1",
            Some(("L0084", "1")),
            &[("L0165", "Unbound"), ("L0084", "1")][..],
        ),
        ("value", None, &[("L0165", "Unbound")][..]),
    ] {
        let call = format!("take<Unbound>(Carrier<Int?>(value), {actual})");
        assert_call_recovery_in_entry(
            "fun <T> take(fixed: Carrier<T>, value: T?): Int = 0",
            &format!("{{ val value: Int? = null; return {call} }}"),
            &call,
            raw,
            final_diagnostics,
            None,
        );
    }
}

fn assert_call_recovery(
    declarations: &str,
    call: &str,
    raw: Option<(&str, &str)>,
    final_diagnostics: &[(&str, &str)],
    retained_call: Option<&str>,
) {
    assert_call_recovery_in_entry(
        declarations,
        &format!("= {call}"),
        call,
        raw,
        final_diagnostics,
        retained_call,
    );
}

fn assert_call_recovery_in_entry(
    declarations: &str,
    entry: &str,
    call: &str,
    raw: Option<(&str, &str)>,
    final_diagnostics: &[(&str, &str)],
    retained_call: Option<&str>,
) {
    assert_call_recovery_with_raw(
        declarations,
        entry,
        &[call],
        raw.as_slice(),
        final_diagnostics,
        retained_call,
    );
}

fn assert_call_recovery_with_raw(
    declarations: &str,
    entry: &str,
    rejected_calls: &[&str],
    raw: &[(&str, &str)],
    final_diagnostics: &[(&str, &str)],
    retained_call: Option<&str>,
) {
    let call = rejected_calls[0];
    let mut environment = NameEnvironment::new();
    let builtins = BuiltinType::ALL
        .map(|builtin| (environment.declare_type(builtin.name()).unwrap(), builtin));
    let list = environment.declare_type("List").unwrap();
    let array = environment.declare_type("Array").unwrap();
    let rc = environment.declare_type("Rc").unwrap();
    let map = environment.declare_type("Map").unwrap();
    let mutable_map = environment.declare_type("MutableMap").unwrap();
    let boxed = environment.declare_type("Box").unwrap();
    let list_of = environment.declare_function("listOf").unwrap();
    let map_of = environment.declare_function("mapOf").unwrap();
    let mutable_map_of = environment.declare_function("mutableMapOf").unwrap();
    let array_of = environment.declare_function("arrayOf").unwrap();
    let replace = environment.declare_function("replace").unwrap();
    let swap = environment.declare_function("swap").unwrap();
    let externals = ["Unbound", "Other"].map(|name| environment.declare_type(name).unwrap());
    environment.declare_function("outside").unwrap();
    environment.declare_function("inside").unwrap();
    let external_routes = [BuiltinType::Int, BuiltinType::Boolean]
        .map(|ty| (environment.declare_function("externalRoute").unwrap(), ty));
    let mut types = TypeEnvironment::new(&environment);
    types
        .bind_intrinsic(map, IntrinsicTypeConstructor::Map)
        .unwrap();
    types
        .bind_intrinsic(mutable_map, IntrinsicTypeConstructor::MutableMap)
        .unwrap();
    types
        .bind_intrinsic(boxed, IntrinsicTypeConstructor::Box)
        .unwrap();
    for (symbol, builtin) in builtins {
        types.bind_builtin(symbol, builtin).unwrap();
    }
    for (symbol, ty) in external_routes {
        let ty = EnvironmentType::Builtin(ty);
        types
            .bind_function(
                symbol,
                EnvironmentFunction {
                    parameters: vec![
                        EnvironmentParameter {
                            mode: ParameterMode::Value,
                            ty: EnvironmentType::Builtin(BuiltinType::Int),
                        },
                        EnvironmentParameter {
                            mode: ParameterMode::Borrow,
                            ty: EnvironmentType::Function {
                                move_only: false,
                                parameters: vec![EnvironmentParameter {
                                    mode: ParameterMode::Borrow,
                                    ty: ty.clone(),
                                }],
                                return_type: Box::new(ty),
                            },
                        },
                    ],
                    return_type: EnvironmentType::Builtin(BuiltinType::Int),
                    effects: Vec::new(),
                },
            )
            .unwrap();
    }
    types
        .bind_intrinsic(list, IntrinsicTypeConstructor::List)
        .unwrap();
    types
        .bind_intrinsic(array, IntrinsicTypeConstructor::Array)
        .unwrap();
    types
        .bind_intrinsic(rc, IntrinsicTypeConstructor::Rc)
        .unwrap();
    types
        .bind_intrinsic_callable(
            list_of,
            lang_frontend::type_checking::IntrinsicCallable::ListOf,
        )
        .unwrap();
    types
        .bind_intrinsic_callable(
            map_of,
            lang_frontend::type_checking::IntrinsicCallable::MapOf,
        )
        .unwrap();
    types
        .bind_intrinsic_callable(
            mutable_map_of,
            lang_frontend::type_checking::IntrinsicCallable::MutableMapOf,
        )
        .unwrap();
    for (symbol, intrinsic) in [
        (
            array_of,
            lang_frontend::type_checking::IntrinsicCallable::ArrayOf,
        ),
        (
            replace,
            lang_frontend::type_checking::IntrinsicCallable::Replace,
        ),
        (swap, lang_frontend::type_checking::IntrinsicCallable::Swap),
    ] {
        types.bind_intrinsic_callable(symbol, intrinsic).unwrap();
    }
    let text = format!(
        "class Carrier<T>(val item: T)\nclass Pair<A, B>(val first: A, val second: B)\n\
         {declarations}\nfun entry(): Int {entry}"
    );
    let mut sources = SourceMap::new();
    let (source, file) = parsed(&mut sources, "rejected-call.ko", &text);
    for input in [
        SourceUnitInput::memory_document(source, &file),
        SourceUnitInput::new("root", "rejected-call.ko", source, &file),
    ] {
        let inputs = [input];
        let names = validated_names(&sources, &inputs, &environment);
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        for external in externals {
            assert!(
                typed
                    .types()
                    .find(&UnitTypeKind::Deferred(DeferredReason::ExternalConstraint(
                        external
                    )))
                    .is_none(),
                "scratch external constraint escaped"
            );
        }
        let unit = source_unit(&names, source);
        let call_id =
            |text| UnitExpressionId::new(unit, expression_with_text(&sources, &file, text));
        for rejected in rejected_calls {
            let id = call_id(rejected);
            assert!(typed.call(id).is_none(), "{rejected}");
            assert!(typed.construction(id).is_none(), "{rejected}");
            assert!(typed.container_construction(id).is_none(), "{rejected}");
            assert!(typed.map_construction(id).is_none(), "{rejected}");
            assert!(typed.container_size(id).is_none(), "{rejected}");
            assert!(typed.map_size(id).is_none(), "{rejected}");
            assert!(typed.string_operation(id).is_none(), "{rejected}");
            assert!(typed.element_place(id).is_none(), "{rejected}");
            assert!(typed.map_get(id).is_none(), "{rejected}");
            assert!(typed.map_put(id).is_none(), "{rejected}");
            assert!(typed.integer_operation(id).is_none(), "{rejected}");
            assert!(typed.ownership_primitive(id).is_none(), "{rejected}");
            assert!(typed.assignment(id).is_none(), "{rejected}");
        }
        if call == "take<Unbound>(1, true)" {
            let int = typed.types().builtin(BuiltinType::Int).unwrap();
            assert!(typed.types().find(&UnitTypeKind::Nullable(int)).is_none());
        }
        if call.contains("inside()") {
            assert!(
                typed.expression_type(call_id("inside()")).is_none(),
                "{call}"
            );
        }
        assert_diagnostics(&sources, typed.diagnostics(), raw);

        let result = complete_compilation_unit_types(&sources, &inputs, &names, &types, &typed);
        let Err(FinalTypeAdmissionError::Diagnostics(diagnostics)) = result else {
            panic!("{call}: {result:?}");
        };
        assert_diagnostics(&sources, &diagnostics, final_diagnostics);
        if let Some(retained) = retained_call {
            let retained_id = call_id(retained);
            if typed.call(retained_id).is_some() {
                assert_eq!(
                    typed
                        .calls()
                        .iter()
                        .filter(|call| call.expression() == retained_id)
                        .count(),
                    1,
                    "{retained}"
                );
            }
            assert!(
                typed.call(call_id(retained)).is_some()
                    || typed.container_construction(call_id(retained)).is_some()
                    || typed.element_place(call_id(retained)).is_some()
                    || typed.map_get(call_id(retained)).is_some()
                    || typed.map_put(call_id(retained)).is_some()
                    || typed.integer_operation(call_id(retained)).is_some()
                    || typed
                        .constructions()
                        .iter()
                        .any(|fact| fact.expression() == call_id(retained)),
                "{retained}"
            );
        }
    }
}

fn assert_diagnostics(sources: &SourceMap, diagnostics: &[Diagnostic], expected: &[(&str, &str)]) {
    let actual = diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.code().to_string(),
                sources.slice(diagnostic.primary_span()).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let expected = expected
        .iter()
        .map(|(code, primary)| (code.to_string(), *primary))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}
