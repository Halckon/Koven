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

#[test]
fn composite_boolean_proof_bypasses_only_the_unreachable_rhs() {
    assert_repeating_cases(&[
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
