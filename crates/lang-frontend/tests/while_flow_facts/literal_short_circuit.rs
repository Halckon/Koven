use super::*;

#[test]
fn known_bypassed_logical_rhs_does_not_kill_nullable_flow() {
    assert_cases(&[
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
    assert_cases(&[
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

#[test]
fn composite_boolean_proof_bypasses_only_the_unreachable_rhs() {
    assert_cases(&[
        Case {
            name: "composite_boolean_lefts_bypass_rhs",
            text: "fun mutate(inout value: Int?): Boolean = true\nfun test(initial: Int?, flag: Boolean): Unit { var x = initial; if (x != null) { val a = (false && flag) && mutate(&x); val first: Int = /*first*/x; val b = (flag || true) || mutate(&x); val second: Int = /*second*/x; val c = !(flag || true) && mutate(&x); val third: Int = /*third*/x } }",
            marks: vec![
                read("first", Shape::Int),
                read("second", Shape::Int),
                read("third", Shape::Int),
            ],
            rejected: None,
        },
        Case {
            name: "compound_bypass_in_nested_conditions_and_loop_prescan",
            text: "fun mutate(inout value: Int?): Boolean = true\nfun test(initial: Int?, flag: Boolean): Unit { var x = initial; if (x != null && ((flag || true) || mutate(&x))) { while (flag) { val a: Int = /*a*/x; val ignore = flag && ((flag && false) && mutate(&x)); break }; val b: Int = /*b*/x } }",
            marks: vec![read("a", Shape::Int), read("b", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "compound_bypass_keeps_when_source_version",
            text: "fun mutate(inout value: Int?): Boolean = true
            fun inspect(input: Int?, flag: Boolean): Int {
                var x = input
                return when (x) {
                    (if ((flag && false) && mutate(&x)) { 1 } else { 2 }) -> 0
                    null -> 0
                    else -> /*use*/x
                }
            }",
            marks: vec![read("use", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "compound_proof_does_not_drop_evaluated_left_effects",
            text: "fun mutate(inout value: Int?): Boolean = true\nfun test(initial: Int?, flag: Boolean): Unit { var x = initial; if (x != null) { val ignore = flag && ((mutate(&x) && false) && flag); val bad: Int = /*reject*/x } }",
            marks: vec![read("reject", Shape::Error)],
            rejected: Some("reject"),
        },
    ]);
}

#[test]
fn rooted_compound_proofs_are_qualified_by_source_unit() {
    const BODY: &str = "fun mutate(inout value: Int?): Boolean = true
    fun test(initial: Int?, flag: Boolean): Unit {
        var x = initial
        if (x != null) {
            val ignored = (LITERAL && flag) && mutate(&x)
            val after: Int = /*use*/x
        }
    }";

    let mut sources = SourceMap::new();
    let dead = format!("package one\n{}", BODY.replace("LITERAL", "false"));
    let live = format!("package two\n{}", BODY.replace("LITERAL", "true "));
    let first = sources.add_source("one/main.ko", &dead).unwrap();
    let second = sources.add_source("two/main.ko", &live).unwrap();
    let parse = |source| {
        let parsed = parse_file(&sources, &lex(&sources, source).unwrap()).unwrap();
        assert!(parsed.diagnostics().is_empty());
        parsed
    };
    let first_file = parse(first);
    let second_file = parse(second);
    let read_at = dead.find("/*use*/x").unwrap() + "/*use*/".len();
    assert_eq!(read_at, live.find("/*use*/x").unwrap() + "/*use*/".len());
    let read_id = |file: &ParsedFile| {
        file.ast()
            .expressions()
            .iter()
            .find_map(|(id, node)| {
                (node.span().start() == read_at && matches!(node.payload(), Expression::Name))
                    .then_some(id)
            })
            .unwrap()
    };
    let read = read_id(&first_file);
    assert_eq!(read, read_id(&second_file), "same local AST identity");
    let (environment, types) = standard_environments();
    for reversed in [false, true] {
        let mut inputs = [
            SourceUnitInput::new("root", "one/main.ko", first, &first_file),
            SourceUnitInput::new("root", "two/main.ko", second, &second_file),
        ];
        if reversed {
            inputs.reverse();
        }
        let index = index_compilation_unit(&sources, &inputs).unwrap();
        let names = resolve_compilation_unit_names(&sources, &inputs, &index, &environment)
            .unwrap()
            .validate()
            .unwrap();
        let typed = check_compilation_unit_types(&sources, &inputs, &names, &types).unwrap();
        let [diagnostic] = typed.diagnostics() else {
            panic!(
                "only the possibly evaluated write kills a fact: {:?}",
                typed.diagnostics()
            );
        };
        assert_eq!(diagnostic.code().to_string(), "L0084");
        assert_eq!(
            diagnostic.primary_span(),
            sources.span(second, read_at, read_at + 1).unwrap()
        );
        let dead_unit = index
            .source_units()
            .iter()
            .find(|unit| unit.source_id() == first)
            .unwrap()
            .id();
        let ty = typed
            .expression_type(UnitExpressionId::new(dead_unit, read))
            .unwrap();
        assert_eq!(
            typed.types().get(ty),
            Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        );
    }
}

#[test]
fn loop_prescan_keeps_the_writes_before_an_interpolation_jump() {
    assert_cases(&[
        Case {
            name: "interpolation_dead_suffix_preserves_non_null_proof",
            text: r#"fun test(initial: Int?): Unit {
                var x = initial
                if (x != null) {
                    loop { val text = "${((break))}${if (true) { x = null; 1 } else { 0 }}" }
                    val after: Int = /*after*/x
                }
            }"#,
            marks: vec![read("after", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "interpolation_nested_string_jump_stops_outer_suffix_writes",
            text: r#"fun test(initial: Int?): Unit {
                var x = initial
                if (x != null) {
                    loop { val text = "${"inner ${break}"}${if (true) { x = null; 1 } else { 0 }}" }
                    val after: Int = /*after*/x
                }
            }"#,
            marks: vec![read("after", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "interpolation_call_argument_jump_stops_suffix_writes",
            text: r#"fun word(value: Int): String = "ok"
            fun test(initial: Int?): Unit {
                var x = initial
                if (x != null) {
                    loop { val text = "${word(break)}${if (true) { x = null; 1 } else { 0 }}" }
                    val after: Int = /*after*/x
                }
            }"#,
            marks: vec![read("after", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "interpolation_call_argument_jump_stops_later_arguments",
            text: r#"fun word(first: Int, second: Int): String = "ok"
            fun test(initial: Int?): Unit {
                var x = initial
                if (x != null) {
                    loop { val text = "${word(break, if (true) { x = null; 1 } else { 0 })}" }
                    val after: Int = /*after*/x
                }
            }"#,
            marks: vec![read("after", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "interpolation_interrupted_call_does_not_write_back",
            text: r#"fun word(inout value: Int?, next: Int): String { value = null; return "ok" }
            fun test(initial: Int?): Unit {
                var x = initial
                if (x != null) {
                    loop { val text = "${word(&x, break)}" }
                    val after: Int = /*after*/x
                }
            }"#,
            marks: vec![read("after", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "interpolation_completed_call_can_write_back",
            text: r#"fun word(inout value: Int?, next: Int): String { value = null; return "ok" }
            fun test(initial: Int?): Unit {
                var x = initial
                if (x != null) {
                    loop { val text = "${word(&x, 1)}${break}" }
                    val after: Int = /*reject*/x
                }
            }"#,
            marks: vec![read("reject", Shape::Error)],
            rejected: Some("reject"),
        },
        Case {
            name: "interpolation_return_payload_retains_a_write_before_break",
            text: r#"fun test(initial: Int?, flag: Boolean): Int {
                var x = initial
                if (x != null) {
                    loop { val text = "${return if (flag) { x = null; break } else { 7 }}" }
                    val after: Int = /*reject*/x
                }
                return 0
            }"#,
            marks: vec![read("reject", Shape::Error)],
            rejected: Some("reject"),
        },
        Case {
            name: "interpolation_write_before_jump_invalidates_non_null_proof",
            text: r#"fun test(initial: Int?): Unit {
                var x = initial
                if (x != null) {
                    loop { val text = "${if (true) { x = null; 1 } else { 0 }}${break}" }
                    val after: Int = /*reject*/x
                }
            }"#,
            marks: vec![read("reject", Shape::Error)],
            rejected: Some("reject"),
        },
    ]);
}

#[test]
fn loop_prescan_stops_after_all_if_arms_jump() {
    macro_rules! source {
        ($first_part:literal) => {
            concat!(
                r#"fun test(initial: Int?, flag: Boolean): Unit {
                    var x = initial
                    if (x != null) {
                        loop { val text = "${"#,
                $first_part,
                r#"}${if (true) { x = null; 1 } else { 0 }}${break}" }
                        val after: Int = /*after*/x
                    }
                }"#,
            )
        };
    }
    for (name, text, rejects) in [
        (
            "both_arms_jump",
            source!("if (flag) { break } else { break }"),
            false,
        ),
        (
            "neither_arm_writes_after_its_jump",
            source!("if (flag) { break; x = null } else { break; x = null }"),
            false,
        ),
        (
            "one_arm_completes",
            source!("if (flag) { break } else { 0 }"),
            true,
        ),
        (
            "missing_else_completes",
            source!("if (flag) { if (initial == null) { break }; 0 } else { break }"),
            true,
        ),
        (
            "condition_jumps_before_arms",
            source!("if (if (flag) { break } else { break }) { x = null; 1 } else { x = null; 0 }"),
            false,
        ),
        (
            "reachable_write_before_jump",
            source!("if (flag) { x = null; break } else { break }"),
            true,
        ),
        (
            "while_condition_break_exits_outer_loop",
            source!("if (flag) { while (break) { x = null }; x = null; 0 } else { break }"),
            false,
        ),
        (
            "for_source_break_exits_outer_loop",
            source!(
                "if (flag) { for (ignored in listOf<Int>(break)) { x = null }; x = null; 0 } else { break }"
            ),
            false,
        ),
        (
            "inner_loop_break_is_not_a_jump_out_of_the_interpolation",
            source!("if (flag) { loop { break }; 0 } else { break }"),
            true,
        ),
    ] {
        assert_cases(&[Case {
            name,
            text,
            marks: vec![read(
                "after",
                if rejects { Shape::Error } else { Shape::Int },
            )],
            rejected: rejects.then_some("after"),
        }]);
    }
}

#[test]
fn while_condition_without_backedge_preserves_the_entry_proof() {
    assert_cases(&[
        Case {
            name: "body_write_is_unreachable_from_the_condition",
            text: "fun cond(value: Int, stop: Boolean): Boolean = stop\n\
                   fun test(initial: Int?): Unit { var x = initial; if (x != null) {\
                       loop { while (cond(/*condition*/x, break)) { x = null } };\
                       val after: Int = /*after*/x\
                   } }",
            marks: vec![read("condition", Shape::Int), read("after", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "body_write_is_unreachable_from_the_negated_condition",
            text: "fun cond(value: Int, stop: Boolean): Boolean = stop\n\
                   fun test(initial: Int?): Unit { var x = initial; if (x != null) {\
                       loop { while (!cond(/*condition*/x, break)) { x = null } };\
                       val after: Int = /*after*/x\
                   } }",
            marks: vec![read("condition", Shape::Int), read("after", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "body_write_can_reach_the_next_condition",
            text: "fun cond(value: Int, stop: Boolean): Boolean = stop\n\
                   fun test(initial: Int?, flag: Boolean): Unit { var x = initial; if (x != null) {\
                       while (cond(/*condition*/x, flag)) { x = null }\
                   } }",
            marks: vec![read("condition", Shape::Error)],
            rejected: Some("condition"),
        },
        Case {
            name: "body_write_can_reach_the_next_negated_condition",
            text: "fun cond(value: Int, stop: Boolean): Boolean = stop\n\
                   fun test(initial: Int?, flag: Boolean): Unit { var x = initial; if (x != null) {\
                       while (!cond(/*condition*/x, flag)) { x = null }\
                   } }",
            marks: vec![read("condition", Shape::Error)],
            rejected: Some("condition"),
        },
        Case {
            name: "condition_write_invalidates_later_but_not_earlier_read",
            text: "fun cond(first: Int, effect: Boolean, later: Int, stop: Boolean): Boolean = stop\n\
                   fun test(initial: Int?, flag: Boolean): Unit { var x = initial;\
                       loop { if (x != null) {\
                           while (cond(/*condition*/x, if (flag) { /*write*/x = null; true } else { true }, /*later*/x, break)) {}\
                       } else { break } }\
                   }",
            marks: vec![
                read("condition", Shape::Int),
                write(),
                read("later", Shape::Error),
            ],
            rejected: Some("later"),
        },
    ]);
}

#[test]
fn loop_prescan_follows_the_evaluated_when_prefix() {
    for (name, body, rejects) in [
        (
            "subject_stops_before_arms",
            "when (break) { else -> { x = null } }",
            false,
        ),
        (
            "subject_stops_before_tail",
            "when ((break)) { else -> {} }; x = null",
            false,
        ),
        (
            "subject_continue_bypasses_writes",
            "when (continue) { else -> { x = null } }; x = null",
            false,
        ),
        (
            "selector_stops_before_arms",
            "when { keep(quiet(), break) -> { x = null }; else -> { x = null } }",
            false,
        ),
        (
            "known_miss_reaches_jump",
            "when { false, keep(quiet(), break) -> { x = null }; else -> { x = null } }",
            false,
        ),
        (
            "subjectful_selector_stops",
            "when (flag) { keep(quiet(), break) -> { x = null }; else -> { x = null } }",
            false,
        ),
        (
            "all_arms_stop_before_tail",
            "when { flag -> { break }; else -> { break } }; x = null",
            false,
        ),
        (
            "earlier_alternative_can_match",
            "when { flag, keep(quiet(), break) -> { x = null }; else -> {} }",
            true,
        ),
        (
            "known_match_bypasses_jump",
            "when { true, keep(quiet(), break) -> { x = null }; else -> {} }",
            true,
        ),
        (
            "one_arm_reaches_tail",
            "when { flag -> { break }; else -> {} }; x = null",
            true,
        ),
        (
            "nested_body_break_keeps_the_outer_tail_reachable",
            "when (flag) { else -> { loop { break } } }; x = null",
            true,
        ),
        (
            "nested_header_break_leaves_the_outer_loop",
            "when (flag) { else -> { while (break) {}; x = null } }; x = null",
            false,
        ),
        (
            "subject_writes_before_jump",
            "when (keep(clear(&x), break)) { else -> {} }",
            true,
        ),
        (
            "selector_writes_before_jump",
            "when { keep(clear(&x), break) -> {}; else -> {} }",
            true,
        ),
    ] {
        let text = format!(
            "fun cond(value: Int, flag: Boolean): Boolean = flag
             fun quiet(): Unit {{}}
             fun clear(inout target: Int?): Unit {{ target = null }}
             fun keep(value: Unit, flag: Boolean): Boolean = flag
             fun inspect(input: Int?, flag: Boolean): Int {{
                 var x = input
                 if (x == null) {{ return 0 }}
                 while (cond(/*head*/x, flag)) {{ {body} }}
                 return 7
             }}"
        );
        assert_cases(&[Case {
            name,
            text: &text,
            marks: vec![read(
                "head",
                if rejects { Shape::Error } else { Shape::Int },
            )],
            rejected: rejects.then_some("head"),
        }]);
    }

    assert_cases(&[Case {
        name: "subject_break_does_not_kill_the_existing_exit_proof",
        text: "fun inspect(input: Int?): Int {
            var x = input
            if (x == null) { return 0 }
            loop { when (break) { else -> { x = null } } }
            return /*after*/x
        }",
        marks: vec![read("after", Shape::Int)],
        rejected: None,
    }]);
}

#[test]
fn ordinary_member_conditions_keep_entry_proofs_only_when_there_is_no_backedge() {
    assert_cases(&[
        Case {
            name: "object_call_argument_breaks_before_the_loop_body",
            text: "object Gate { fun cond(value: Int, stop: Boolean): Boolean = stop }\n\
                   fun test(initial: Int?): Unit { var x = initial; if (x != null) {\n\
                       loop { while (Gate.cond(/*condition*/x, break)) { x = null } };\n\
                       val after: Int = /*after*/x\n\
                   } }",
            marks: vec![read("condition", Shape::Int), read("after", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "instance_call_argument_breaks_before_the_loop_body",
            text: "class Gate { fun cond(value: Int, stop: Boolean): Boolean = stop }\n\
                   fun test(initial: Int?): Unit { val gate = Gate(); var x = initial; if (x != null) {\n\
                       loop { while (gate.cond(/*condition*/x, break)) { x = null } };\n\
                       val after: Int = /*after*/x\n\
                   } }",
            marks: vec![read("condition", Shape::Int), read("after", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "instance_call_receiver_breaks_before_the_arguments",
            text: "class Gate { fun cond(value: Int, stop: Boolean): Boolean = stop }\n\
                   fun gate(stop: Int): Gate = Gate()\n\
                   fun test(initial: Int?, flag: Boolean): Unit { var x = initial; if (x != null) {\n\
                       loop { while (gate(break).cond(/*condition*/x, flag)) { x = null } };\n\
                       val after: Int = /*after*/x\n\
                   } }",
            marks: vec![read("condition", Shape::Int), read("after", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "member_chain_receiver_breaks_before_the_arguments",
            text: "class Gate { fun cond(value: Int, stop: Boolean): Boolean = stop }\n\
                   class Holder(val gate: Gate)\n\
                   fun holder(stop: Int): Holder = Holder(Gate())\n\
                   fun test(initial: Int?, flag: Boolean): Unit { var x = initial; if (x != null) {\n\
                       loop { while (holder(break).gate.cond(/*condition*/x, flag)) { x = null } };\n\
                       val after: Int = /*after*/x\n\
                   } }",
            marks: vec![read("condition", Shape::Int), read("after", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "receiver_continue_skips_argument_write_on_the_backedge",
            text: "class Gate { fun cond(value: Int, stop: Boolean): Boolean = stop }\n\
                   fun gate(stop: Int): Gate = Gate()\n\
                   fun test(initial: Int?, flag: Boolean): Unit { var x = initial; if (x != null) {\n\
                       while (flag) {\n\
                           val before: Int = /*before*/x;\n\
                           gate(continue).cond(if (flag) { x = null; 7 } else { 7 }, flag)\n\
                       };\n\
                       val after: Int = /*after*/x\n\
                   } }",
            marks: vec![read("before", Shape::Int), read("after", Shape::Int)],
            rejected: None,
        },
        Case {
            name: "completed_member_call_still_allows_body_write_to_reach_the_next_condition",
            text: "class Gate { fun cond(value: Int, stop: Boolean): Boolean = stop }\n\
                   fun test(initial: Int?, flag: Boolean): Unit { val gate = Gate(); var x = initial; if (x != null) {\n\
                       while (gate.cond(/*condition*/x, flag)) { x = null }\n\
                   } }",
            marks: vec![read("condition", Shape::Error)],
            rejected: Some("condition"),
        },
    ]);
}
