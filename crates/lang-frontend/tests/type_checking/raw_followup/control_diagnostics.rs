//! 原赋值/capture smart-cast 与 when 诊断消费者，不消费 ownership 事件。
use super::{checked_entries, codes};

#[test]
fn assignment_and_capture_invalidate_mutable_smart_casts_and_invalid_tests_are_precise() {
    let text = "enum class Shape { Circle(radius: Int), Point }\n\
                interface Marker\n\
                fun assigned(initial: Shape, replacement: Shape): Int {\n\
                    var current = initial\n\
                    if (current is Shape.Circle) {\n\
                        current = replacement\n\
                        return current.radius\n\
                    }\n\
                    return 0\n\
                }\n\
                fun captured(initial: Shape): Int {\n\
                    var current = initial\n\
                    val callback = { current }\n\
                    if (current is Shape.Circle) { return current.radius }\n\
                    return 0\n\
                }\n\
                fun shadowed(initial: Shape): Int {\n\
                    var current = initial\n\
                    {\n\
                        var current = initial\n\
                        val callback = { current }\n\
                    }\n\
                    if (current is Shape.Circle) { return current.radius }\n\
                    return 0\n\
                }\n\
                fun invalid(shape: Shape): Boolean = shape is Marker";
    let (sources, file, entries) = checked_entries(text);
    // Only the assigned and captured occurrences are invalid; the shadowed outer var is stable.
    let invalid_members = text
        .match_indices("current.radius")
        .take(2)
        .map(|(start, member)| {
            sources
                .span(
                    file.source_id(),
                    start + "current.".len(),
                    start + member.len(),
                )
                .expect("member span")
        })
        .collect::<Vec<_>>();
    for (_, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0113", "L0113", "L0106"]);
        assert_eq!(
            typed.diagnostics()[..2]
                .iter()
                .map(|diagnostic| diagnostic.primary_span())
                .collect::<Vec<_>>(),
            invalid_members
        );
        assert_eq!(
            typed
                .diagnostics()
                .iter()
                .map(|diagnostic| sources.slice(diagnostic.primary_span()).expect("span"))
                .collect::<Vec<_>>(),
            ["radius", "radius", "is"]
        );
    }
}

#[test]
fn when_negative_and_comma_coverage_keep_only_shared_flow_facts() {
    let valid = "enum class Shape { Circle(radius: Int), Point }
                 fun classify(shape: Shape): Int = when (shape) {
                     !is Shape.Circle -> 0
                     is Shape.Circle -> shape.radius
                 }";
    let (_, _, entries) = checked_entries(valid);
    for (_, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
    }
    let invalid = "enum class Shape { Circle(radius: Int), Point }
                   fun merged(shape: Shape): Int = when (shape) {
                       is Shape.Circle, is Shape.Point -> shape.radius
                   }";
    let (sources, _, entries) = checked_entries(invalid);
    for (_, typed) in entries {
        assert_eq!(codes(typed.diagnostics()), ["L0113"]);
        assert_eq!(
            sources
                .slice(typed.diagnostics()[0].primary_span())
                .expect("payload span"),
            "radius"
        );
    }
}

#[test]
fn when_context_and_coverage_diagnostics_are_deterministic() {
    let text = "enum class Shape { Circle(radius: Int), Point }
                fun statement(shape: Shape): Int {
                    when (shape) { is Shape.Circle -> shape.radius }
                    return 0
                }
                fun nestedStatement(flag: Boolean, shape: Shape): Int {
                    if (flag) {
                        when (shape) { is Shape.Circle -> shape.radius }
                    }
                    return 0
                }
                fun missing(shape: Shape): Int = when (shape) {
                    is Shape.Circle -> shape.radius
                }
                fun repeated(flag: Boolean): Int = when (flag) {
                    true -> 1
                    true -> 2
                }
                fun misplaced(flag: Boolean): Int = when (flag) {
                    else -> 0
                    true -> 1
                    else -> 2
                }";
    let (sources, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert_eq!(
            codes(typed.diagnostics()),
            ["L0111", "L0111", "L0110", "L0109", "L0108"]
        );
        assert_eq!(
            typed
                .diagnostics()
                .iter()
                .map(|diagnostic| sources.slice(diagnostic.primary_span()).expect("span"))
                .collect::<Vec<_>>(),
            ["when", "when", "true", "else", "else"]
        );
    }
}

#[test]
fn unfinished_inout_call_keeps_the_other_paths_narrowed() {
    let overloads = "
        fun <T> choose(seed: T, inout x: Int?, stop: Int, cb: (Int) -> Int): Unit { x = null }
        fun <T> choose(seed: T, inout x: Int?, stop: Int, cb: (Boolean) -> Boolean): Unit { x = null }
    ";
    for (argument, expected) in [
        ("(return 1)", [].as_slice()),
        ("(if (return 1) 2 else 3)", [].as_slice()),
        ("(when (return 1) { else -> 2 })", [].as_slice()),
        ("(when (0) { else -> 2 })", ["L0084"].as_slice()),
        ("1", ["L0084"].as_slice()),
    ] {
        let text = format!(
            "{overloads}
            fun test(seed: Int?, flag: Boolean): Int {{
                var x = seed
                if (x != null) {{
                    if (flag) {{ choose(0, &x, {argument}, {{ it - 1 }}) }}
                    return x
                }}
                return 0
            }}"
        );
        let (sources, _, entries) = checked_entries(&text);
        for (_, typed) in entries {
            assert_eq!(codes(typed.diagnostics()), expected, "{argument}");
            for diagnostic in typed.diagnostics() {
                assert_eq!(sources.slice(diagnostic.primary_span()).unwrap(), "x");
            }
        }
    }
}

#[test]
fn unfinished_call_argument_is_bottom_in_a_value_branch() {
    let text = "
        fun <T> choose(seed: T, inout x: Int?, stop: Int, cb: (Int) -> Int): Unit { x = null }
        fun <T> choose(seed: T, inout x: Int?, stop: Int, cb: (Boolean) -> Boolean): Unit { x = null }
        fun test(flag: Boolean): Int {
            var x: Int? = 0
            return if (flag) { choose(0, &x, (return 1), { it - 1 }) } else { 2 }
        }
    ";
    let (_, _, entries) = checked_entries(text);
    for (_, typed) in entries {
        assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
        typed.validate().expect("valid unit types");
    }
}

#[test]
fn unfinished_call_retains_the_static_shape_for_a_chained_call() {
    for (declaration, use_site) in [
        (
            "fun factory(value: Int): () -> Int = { 7 }",
            "factory(return 1)()",
        ),
        (
            "class C { fun method(): Unit {} }\nfun factory(value: Int): C = C()",
            "factory(return 1).method()",
        ),
        (
            "fun build(value: Int): Int = value
             fun sink(value: Int): Unit {}
             fun sink(value: Boolean): Unit {}",
            "sink(build(return 1))",
        ),
    ] {
        let text = format!(
            "{declaration}
             fun test(): Int {{
                 {use_site}
                 return 2
             }}"
        );
        let (_, _, entries) = checked_entries(&text);
        for (_, typed) in entries {
            assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
            assert_eq!(typed.calls().len(), 2, "{use_site}: {:?}", typed.calls());
            typed.validate().expect("valid unit types");
        }
    }
}

#[test]
fn rejected_inout_call_does_not_kill_non_null_facts() {
    let overloads = "
        fun <T> choose(seed: T, inout x: Int?, cb: (Int) -> Int): Boolean { x = null; return true }
        fun <T> choose(seed: T, inout x: Int?, cb: (Boolean) -> Boolean): Boolean { x = null; return true }
    ";
    for body in [
        "if (x != null) { CALL; return x }",
        "if (x != null && CALL) { return x }",
        "if (x == null || CALL) {} else { return x }",
    ] {
        for (callback, code) in [("it + \"\"", "L0123"), ("it - 1", "L0084")] {
            let call = format!("choose(0, &x, {{ {callback} }})");
            let body = body.replace("CALL", &call);
            let text = format!(
                "{overloads}
                 fun test(seed: Int?): Int {{ var x = seed; {body}; return 0 }}"
            );
            let (sources, _, entries) = checked_entries(&text);
            for (_, typed) in entries {
                assert_eq!(codes(typed.diagnostics()), [code], "{body}");
                if code == "L0084" {
                    assert_eq!(
                        sources
                            .slice(typed.diagnostics()[0].primary_span())
                            .unwrap(),
                        "x"
                    );
                }
            }
        }
    }
}

#[test]
fn inout_effects_keep_nested_calls_and_contextually_rejected_results() {
    let declarations = "
        fun update(inout x: Int?): Int { x = null; return 0 }
        fun predicate(value: Boolean): Boolean = value
        fun choose(seed: Int, inout x: Int?, cb: (Int) -> Int): Boolean = true
        fun choose(seed: Int, inout x: Int?, cb: (Boolean) -> Boolean): Boolean = true
    ";
    for (condition, tail, error_code, error_text, changed) in [
        (
            "x != null && predicate(update(&x))",
            "return x",
            "L0084",
            "update(&x)",
            "x",
        ),
        (
            "x != null && y != null && choose(update(&y), &x, { it + \"\" })",
            "val retained: Int = x; return y",
            "L0123",
            "choose",
            "y",
        ),
    ] {
        let text = format!(
            "{declarations}
             fun test(seed: Int?): Int {{
                 var x = seed
                 var y = seed
                 if ({condition}) {{ {tail} }}
                 return 0
             }}"
        );
        let (sources, _, entries) = checked_entries(&text);
        for (_, typed) in entries {
            assert_eq!(
                codes(typed.diagnostics()),
                [error_code, "L0084"],
                "{condition}"
            );
            assert_eq!(
                typed
                    .diagnostics()
                    .iter()
                    .map(|diag| sources.slice(diag.primary_span()).unwrap())
                    .collect::<Vec<_>>(),
                [error_text, changed],
                "{condition}"
            );
        }
    }
}

#[test]
fn rejected_calls_keep_non_null_facts_with_deferred_recovery_roots() {
    let declarations = "
        fun choose(seed: Int, inout x: Int?, cb: (Int) -> Int): Boolean = true
        fun choose(seed: Int, inout x: Int?, cb: (Boolean) -> Boolean): Boolean = true
    ";
    for (scope, parameters, call) in [
        (
            format!("class Picker {{ {declarations} }}"),
            "seed: Int?, picker: Picker",
            r#"picker.choose(0, &x, { it + "" })"#,
        ),
        (
            declarations.to_owned(),
            "seed: Int?",
            r#"choose("wrong", &x, { it })"#,
        ),
    ] {
        let text = format!(
            "{scope}
             fun test({parameters}): Int {{
                 var x = seed
                 if (x != null && {call}) {{ return x }}
                 return 0
             }}"
        );
        let (_, _, entries) = checked_entries(&text);
        for (_, typed) in entries {
            assert_eq!(codes(typed.diagnostics()), ["L0123"], "{call}");
        }
    }
}

#[test]
fn rejected_callee_shapes_do_not_kill_inout_narrowing() {
    let declarations = "
        fun choose(seed: Int, cb: (Int) -> Int): (inout Int?) -> Boolean = { _ -> true }
        fun choose(seed: Int, cb: (Boolean) -> Boolean): (inout Int?) -> Boolean = { _ -> true }
        fun badIndex(seed: Int, cb: (Int) -> Int): Int = 0
        fun badIndex(seed: Int, cb: (Boolean) -> Boolean): Int = 0
    ";
    for (parameters, call, expected) in [
        (
            "seed: Int?, callback: (inout Int?) -> Boolean",
            "callback<Int>(&x)",
            [].as_slice(),
        ),
        (
            "seed: Int?",
            r#"(choose("wrong", { it }))(&x)"#,
            ["L0123"].as_slice(),
        ),
        (
            "seed: Int?",
            r#"choose("wrong", { it })[0](&x)"#,
            ["L0123"].as_slice(),
        ),
        (
            "seed: Int?, items: List<(inout Int?) -> Boolean>",
            r#"items[badIndex("wrong", { it })](&x)"#,
            ["L0123"].as_slice(),
        ),
    ] {
        let text = format!(
            "{declarations}
             fun test({parameters}): Int {{
                 var x = seed
                 if (x != null && {call}) {{ return x }}
                 return 0
             }}"
        );
        let (_, _, entries) = checked_entries(&text);
        for (_, typed) in entries {
            assert_eq!(codes(typed.diagnostics()), expected, "{call}");
            assert!(typed.calls().is_empty(), "{:?}", typed.calls());
        }
    }
}
