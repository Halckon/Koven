use super::*;

#[test]
fn known_bypassed_logical_rhs_does_not_kill_nullable_flow() {
    assert_repeating_cases(&[
        Case {
            name: "false_and_true_or_and_grouped_not",
            text: "fun mutate(inout value: Int?): Boolean = true\nfun test(initial: Int?): Unit { var x = initial; if (x != null) { val one = false && mutate(&x); val a: Int = /*a*/x; val two = true || mutate(&x); val b: Int = /*b*/x; val three = (!((true))) && mutate(&x); val c: Int = /*c*/x } }",
            marks: vec![
                read("a", Shape::Int),
                read("b", Shape::Int),
                read("c", Shape::Int),
            ],
            rejected: None,
        },
        Case {
            name: "nested_rhs_and_condition",
            text: "fun mutate(inout value: Int?): Boolean = true\nfun test(initial: Int?, flag: Boolean): Unit { var x = initial; if (x != null) { val ignore = flag && (false && mutate(&x)); val a: Int = /*a*/x }; if (x != null && (true || mutate(&x))) { val b: Int = /*b*/x } }",
            marks: vec![read("a", Shape::Int), read("b", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "while_prescan_omits_dead_write",
            text: "fun mutate(inout value: Int?): Boolean = true\nfun test(initial: Int?, flag: Boolean): Unit { var x = initial; if (x != null) { while (flag && (true || mutate(&x))) { val a: Int = /*a*/x; val ignore = false && mutate(&x); break }; val b: Int = /*b*/x } }",
            marks: vec![read("a", Shape::Int), read("b", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "dead_write_preserves_when_source_version",
            text: "fun mutate(inout value: Int?): Boolean = true
            fun inspect(input: Int?): Int {
                var x = input
                return when (x) {
                    (if (false && mutate(&x)) { 1 } else { 2 }) -> 0
                    null -> 0
                    else -> /*use*/x
                }
            }",
            marks: vec![read("use", Shape::Int)],
            rejected: None,
        },
    ]);
}

#[test]
fn potentially_evaluated_rhs_still_kills_facts_and_dead_rhs_is_checked() {
    assert_repeating_cases(&[
        Case {
            name: "required_rhs_still_kills",
            text: "fun mutate(inout value: Int?): Boolean = true\nfun test(initial: Int?): Unit { var x = initial; if (x != null) { val ignore = true && mutate(&x); val bad: Int = /*reject*/x } }",
            marks: vec![read("reject", Shape::Error)],
            rejected: Some("reject"),
        },
        Case {
            name: "unknown_rhs_still_kills",
            text: "fun mutate(inout value: Int?): Boolean = true\nfun test(initial: Int?, flag: Boolean): Unit { var x = initial; if (x != null) { val ignore = flag && mutate(&x); val bad: Int = /*reject*/x } }",
            marks: vec![read("reject", Shape::Error)],
            rejected: Some("reject"),
        },
        Case {
            name: "nested_dead_write_does_not_hide_an_adjacent_live_write",
            text: "fun mutate(inout value: Int?): Boolean = true\nfun test(initial: Int?, flag: Boolean): Unit { var x = initial; if (x != null) { val ignore = flag && ((false && mutate(&x)) || mutate(&x)); val bad: Int = /*reject*/x } }",
            marks: vec![read("reject", Shape::Error)],
            rejected: Some("reject"),
        },
        Case {
            name: "dead_rhs_still_reports_static_type_error",
            text: "fun mutate(inout value: Int?): Boolean = true\nfun test(initial: Int?, flag: Boolean): Unit { var x = initial; if (x != null) { val ignore = false && mutate(&/*reject*/flag); val after: Int = /*after*/x } }",
            marks: vec![
                Mark {
                    name: "reject",
                    text: "flag",
                    kind: NodeKind::Read,
                    shape: Shape::Error,
                },
                read("after", Shape::Int),
            ],
            rejected: Some("reject"),
        },
    ]);
}
