use super::*;

#[test]
fn grouped_right_operand_keeps_writes_in_call_arguments() {
    assert_cases(&[Case {
        name: "inout-write-in-nested-call-argument",
        text: "fun clear(inout value: Int?): Boolean { value = null; return true }
        fun forward(value: Boolean): Boolean = value
        fun probe(input: Int?, flag: Boolean): Int {
            var x = input
            if (x != null && (flag && (flag && forward(clear(&x))))) {
                return /*reject*/x
            }
            return 0
        }",
        marks: vec![read("reject", Shape::Error)],
        rejected: Some("reject"),
    }]);
}

#[test]
fn grouped_logical_writes_invalidate_the_baseline_on_normal_paths() {
    for (name, condition, marker, rejected) in [
        ("and-reaches", "flag && (flag && clear(&x))", "result", true),
        ("or-reaches", "flag || (flag || clear(&x))", "result", true),
        (
            "and-bypasses",
            "flag && (false && clear(&x))",
            "result",
            false,
        ),
        (
            "or-bypasses",
            "flag || (true || clear(&x))",
            "result",
            false,
        ),
        (
            "nested-left-writes",
            "flag && (clear(&x) && flag)",
            "result",
            true,
        ),
        (
            "write-before-return",
            "flag && (flag && (if (flag) { clear(&x); return 0 } else { true }))",
            "result",
            false,
        ),
        (
            "write-before-local-break",
            "flag && (flag && (if (flag) { loop { clear(&x); break }; true } else { false }))",
            "result",
            true,
        ),
    ] {
        let text = format!(
            "fun clear(inout value: Int?): Boolean {{ value = null; return true }}
            fun probe(input: Int?, flag: Boolean): Int {{
                var x = input
                if (x == null) {{ return 0 }}
                if ({condition}) {{ println(\"hit\") }}
                return /*{marker}*/x
            }}"
        );
        assert_cases(&[Case {
            name,
            text: &text,
            marks: vec![read(
                marker,
                if rejected { Shape::Error } else { Shape::Int },
            )],
            rejected: rejected.then_some(marker),
        }]);
    }
}

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

#[test]
fn long_short_circuit_chain_preserves_selected_write_edges() {
    for (name, seed, operator, marker, rejected) in [
        (
            "and-bypasses",
            "(flag || (clear(&x) && false))",
            "&&",
            "use",
            None,
        ),
        (
            "and-writes",
            "(flag || (clear(&x) && true))",
            "&&",
            "reject",
            Some("reject"),
        ),
        (
            "or-bypasses",
            "(flag && (clear(&x) || true))",
            "||",
            "use",
            None,
        ),
        (
            "or-writes",
            "(flag && (clear(&x) && false))",
            "||",
            "reject",
            Some("reject"),
        ),
    ] {
        let flags = format!(" {operator} flag").repeat(127);
        let source = format!(
            "fun clear(inout value: Int?): Boolean {{ value = null; return true }}
             fun accept(value: Int): Boolean = true
             fun probe(input: Int?, flag: Boolean): Boolean {{
                 var x = input
                 if (x == null) {{ return false }}
                 return {seed}{flags} {operator} accept(/*{marker}*/x)
             }}"
        );
        assert_cases(&[Case {
            name,
            text: &source,
            marks: vec![read(
                marker,
                if rejected.is_some() {
                    Shape::Error
                } else {
                    Shape::Int
                },
            )],
            rejected,
        }]);
    }
}

#[test]
fn long_short_circuit_chain_carries_condition_facts_to_the_rhs() {
    for (name, seed, operator, middle, marker, rejected) in [
        ("and-fact", "x != null", "&&", "", "use", None),
        ("or-fact", "!(x != null)", "||", "", "use", None),
        (
            "and-write",
            "x != null",
            "&&",
            " && clear(&x)",
            "reject",
            Some("reject"),
        ),
        (
            "or-write",
            "!(x != null)",
            "||",
            " || clear(&x)",
            "reject",
            Some("reject"),
        ),
    ] {
        let flags = format!(" {operator} flag").repeat(127);
        let source = format!(
            "fun clear(inout value: Int?): Boolean {{ value = null; return true }}
             fun accept(value: Int): Boolean = true
             fun probe(input: Int?, flag: Boolean): Boolean {{
                 var x = input
                 return {seed}{flags}{middle} {operator} accept(/*{marker}*/x)
             }}"
        );
        assert_cases(&[Case {
            name,
            text: &source,
            marks: vec![read(
                marker,
                if rejected.is_some() {
                    Shape::Error
                } else {
                    Shape::Int
                },
            )],
            rejected,
        }]);
    }
}

#[test]
fn long_grouped_condition_narrows_the_selected_branch() {
    let source = format!(
        "fun probe(x: Int?, flag: Boolean): Int {{
             if (!(x == null{})) {{ return /*use*/x }}
             return 0
         }}",
        " || flag".repeat(128)
    );
    assert_cases(&[Case {
        name: "negated-or-condition-narrows-its-branch",
        text: &source,
        marks: vec![read("use", Shape::Int)],
        rejected: None,
    }]);
}
