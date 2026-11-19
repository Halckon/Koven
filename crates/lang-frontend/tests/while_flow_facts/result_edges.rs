use super::*;

#[test]
fn runtime_boolean_condition_checks_writes_on_its_own_result_edge() {
    assert_cases(&[
        Case {
            name: "when-match-bypasses-write-on-false-edge",
            text: "fun clear(inout value: Int?): Boolean { value = null; return true }
            fun probe(input: Int?, flag: Boolean): Int {
                var x = input
                if (x == null) { return 0 }
                when { flag || (clear(&x) && false) -> { return /*use*/x }; else -> { return 0 } }
            }",
            marks: vec![read("use", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "when-miss-always-has-the-write",
            text: "fun clear(inout value: Int?): Boolean { value = null; return true }
            fun probe(input: Int?, flag: Boolean): Int {
                var x = input
                if (x == null) { return 0 }
                when { flag || (clear(&x) && false) -> { return 0 }; else -> { return /*reject*/x } }
            }",
            marks: vec![read("reject", Shape::Error)],
            rejected: Some("reject"),
        },
        Case {
            name: "when-match-includes-a-reachable-write",
            text: "fun clear(inout value: Int?): Boolean { value = null; return true }
            fun probe(input: Int?, flag: Boolean): Int {
                var x = input
                if (x == null) { return 0 }
                when { flag || (clear(&x) && true) -> { return /*reject*/x }; else -> { return 0 } }
            }",
            marks: vec![read("reject", Shape::Error)],
            rejected: Some("reject"),
        },
        Case {
            name: "if-false-edge-bypasses-write-on-true-edge",
            text: "fun clear(inout value: Int?): Boolean { value = null; return true }
            fun probe(input: Int?, flag: Boolean): Int {
                var x = input
                if (x == null) { return 0 }
                if (flag && (clear(&x) || true)) { return 0 } else { return /*use*/x }
            }",
            marks: vec![read("use", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "known-when-miss-keeps-facts-from-left-bypass",
            text: "fun clear(inout value: Int?): Boolean { value = null; return true }
            fun probe(input: Int?, flag: Boolean): Int {
                var x = input
                if (x == null) { return 0 }
                when { flag && (clear(&x) || true) -> { return 0 }; else -> { return /*use*/x } }
            }",
            marks: vec![read("use", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "if-without-else-keeps-facts-on-the-only-fallthrough",
            text: "fun clear(inout value: Int?): Boolean { value = null; return true }
            fun probe(input: Int?, flag: Boolean): Int {
                var x = input
                if (x == null) { return 0 }
                if (flag && (clear(&x) || true)) { return 0 }
                return /*use*/x
            }",
            marks: vec![read("use", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "same-root-write-on-the-selected-edge-still-kills",
            text: "fun clear(inout value: Int?): Boolean { value = null; return true }
            fun probe(input: Int?, flag: Boolean): Int {
                var x = input
                if (x == null) { return 0 }
                if (clear(&x) && (flag || (clear(&x) && false))) { return /*reject*/x }
                return 0
            }",
            marks: vec![read("reject", Shape::Error)],
            rejected: Some("reject"),
        },
        Case {
            name: "if-false-edge-includes-a-reachable-write",
            text: "fun clear(inout value: Int?): Boolean { value = null; return true }
            fun probe(input: Int?, flag: Boolean): Int {
                var x = input
                if (x == null) { return 0 }
                if (flag && (clear(&x) && false)) { return 0 } else { return /*reject*/x }
            }",
            marks: vec![read("reject", Shape::Error)],
            rejected: Some("reject"),
        },
    ]);
}

#[test]
fn a_logical_rhs_uses_only_the_left_edge_that_reaches_it() {
    assert_cases(&[
        Case {
            name: "and-rhs-skips-when-the-left-write-is-on-the-opposite-edge",
            text: "fun clear(inout value: Int?): Boolean { value = null; return true }
            fun probe(input: Int?, flag: Boolean): Int {
                var x = input
                if (x == null) { return 0 }
                if ((flag || (clear(&x) && false)) && /*use*/x > 0) { return 1 }
                return 0
            }",
            marks: vec![read("use", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "or-rhs-skips-when-the-left-write-is-on-the-opposite-edge",
            text: "fun clear(inout value: Int?): Boolean { value = null; return true }
            fun probe(input: Int?, flag: Boolean): Int {
                var x = input
                if (x == null) { return 0 }
                if ((flag && (clear(&x) || true)) || /*use*/x > 0) { return 1 }
                return 0
            }",
            marks: vec![read("use", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "and-rhs-rejects-a-write-on-the-left-edge-that-reaches-it",
            text: "fun clear(inout value: Int?): Boolean { value = null; return true }
            fun accept(value: Int): Boolean = true
            fun probe(input: Int?, flag: Boolean): Int {
                var x = input
                if (x == null) { return 0 }
                if ((flag || (clear(&x) && true)) && accept(/*reject*/x)) { return 1 }
                return 0
            }",
            marks: vec![read("reject", Shape::Error)],
            rejected: Some("reject"),
        },
    ]);
}
