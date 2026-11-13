//! Positive, normal-source probes for SPEC-0298's bounded local control flow.
use super::*;

pub(in crate::ssa) use self::short_circuit::boundary::local_control_support as support;

pub(in crate::ssa) use super::scalar_counts::local_control_assertions as assertions;

mod ownership {
    //! Same-root parent/child/sibling source fixtures with normal native execution.
    use super::*;

    #[derive(Clone, Copy, Debug)]
    enum Payload {
        String,
        MoveOnly,
        Resource,
    }

    fn ownership_case(pipeline: Pipeline, payload: Payload) {
        let (declaration, element, values, read, drops) = match payload {
            Payload::String => (
                "",
                "String",
                "\"a\".clone(),\"b\".clone(),\"c\".clone()",
                "println(item)",
                "",
            ),
            Payload::MoveOnly => (
                "class Item(val text:String){}",
                "Item",
                "Item(\"a\"),Item(\"b\"),Item(\"c\")",
                "println(item.text)",
                "",
            ),
            Payload::Resource => (
                "class Item(val text:String){deinit(){println(this.text)}}",
                "Item",
                "Item(\"a\"),Item(\"b\"),Item(\"c\")",
                "println(item.text)",
                "c\nb\na\n",
            ),
        };
        let branch = "if(flag){read(parent);read(child);read(sibling)}else{read(parent);read(child);read(sibling)}";
        let consumer = format!(
            r#"
    {declaration}
    fun read(view:View<{element}>):Unit{{for(item in view){{{read}}}}}
    fun consume(own root:List<{element}>):Unit{{println("consume")}}
    fun scan(flag:Boolean):Unit{{
        val root=listOf({values})
        {{
            borrow val parent=take(root,2)
            borrow val child=take(parent,1)
            borrow val sibling=take(parent,2)
            {branch}
            read(parent)
            read(child)
            read(sibling)
        }}
        consume(root)
    }}
    fun main():Unit{{scan(true);scan(false)}}
    "#
        );
        let label = format!("{pipeline:?}-{payload:?}-parent-child-sibling");
        let lowered = lower_positive(&consumer, branch, pipeline);
        let branch_span = lowered.expected_span;
        let (program, entry) = lowered.require_success(&label);
        assertions::if_transport(&program, branch_span, 3);
        let one_scan = format!("a\nb\na\na\nb\na\nb\na\na\nb\nconsume\n{drops}");
        run_unmodified(&program, entry, &label, one_scan.repeat(2).as_bytes());
    }

    macro_rules! ownership_tests {
        ($single:ident, $unit:ident, $reversed:ident, $payload:ident) => {
            #[test]
            fn $single() {
                ownership_case(Pipeline::Single, Payload::$payload);
            }
            #[test]
            fn $unit() {
                ownership_case(Pipeline::Unit, Payload::$payload);
            }
            #[test]
            fn $reversed() {
                ownership_case(Pipeline::UnitReversed, Payload::$payload);
            }
        };
    }

    ownership_tests!(
        range_local_control_string_siblings_single,
        range_local_control_string_siblings_unit,
        range_local_control_string_siblings_unit_reversed,
        String
    );
    ownership_tests!(
        range_local_control_move_only_siblings_single,
        range_local_control_move_only_siblings_unit,
        range_local_control_move_only_siblings_unit_reversed,
        MoveOnly
    );
    ownership_tests!(
        range_local_control_resource_siblings_single,
        range_local_control_resource_siblings_unit,
        range_local_control_resource_siblings_unit_reversed,
        Resource
    );

    fn carrier_scope_layout(case: &str, binding: &str, body: &str, expected: &[u8]) {
        let consumer = format!(
            r#"
    class Item(val text:String){{deinit(){{println("drop");println(this.text)}}}}
    fun read(view:View<Item>):Unit{{for(item in view){{println(item.text)}}}}
    fun scan(own flag:Boolean):Unit{{
        val root=listOf(Item("first"),Item("second"))
        {body}
    }}
    fun main():Unit{{scan(true);scan(false)}}
    "#
        );
        run_carrier_layouts(case, &consumer, binding, expected);
    }

    fn run_carrier_layouts(case: &str, consumer: &str, binding: &str, expected: &[u8]) {
        for (variant, suffix) in [
            ("plain", ""),
            (
                "sibling",
                "\nfun unrelated(): Unit { var a = 1; var b = 2; swap(&a, &b) }",
            ),
        ] {
            let consumer = format!("{consumer}{suffix}");
            for pipeline in [Pipeline::Single, Pipeline::Unit, Pipeline::UnitReversed] {
                let label = format!("carrier-{case}-{variant}-{pipeline:?}");
                let lowered = lower_positive(&consumer, binding, pipeline);
                let (program, entry) = lowered.require_success(&label);
                run_unmodified(&program, entry, &label, expected);
            }
        }
    }

    #[test]
    fn range_carrier_gate_descriptor_ends_before_outer_if() {
        carrier_scope_layout(
            "descriptor-ended",
            "if(flag){println(\"after\")}",
            "{borrow val parent=take(root,1);read(parent)}\nif(flag){println(\"after\")}",
            b"first\nafter\ndrop\nsecond\ndrop\nfirst\nfirst\ndrop\nsecond\ndrop\nfirst\n",
        );
    }

    #[test]
    fn range_carrier_gate_child_ends_before_outer_if() {
        carrier_scope_layout(
            "child-ended",
            "if(flag){println(\"after\")}",
            "{borrow val parent=take(root,1);borrow val child=(parent);read(child)}\nif(flag){println(\"after\")}",
            b"first\nafter\ndrop\nsecond\ndrop\nfirst\nfirst\ndrop\nsecond\ndrop\nfirst\n",
        );
    }

    #[test]
    fn range_carrier_gate_descriptor_begins_after_outer_if() {
        carrier_scope_layout(
            "descriptor-before",
            "if(flag){println(\"left\")}else{println(\"right\")}",
            "if(flag){println(\"left\")}else{println(\"right\")}\n{borrow val parent=take(root,1);read(parent)}",
            b"left\nfirst\ndrop\nsecond\ndrop\nfirst\nright\nfirst\ndrop\nsecond\ndrop\nfirst\n",
        );
    }

    #[test]
    fn range_carrier_gate_child_is_live_across_return() {
        carrier_scope_layout(
            "child-return",
            "if(flag){read(child);return}",
            "borrow val parent=take(root,1)\nborrow val child=(parent)\nif(flag){read(child);return}\nread(child);println(\"continued\")",
            b"first\ndrop\nsecond\ndrop\nfirst\nfirst\ncontinued\ndrop\nsecond\ndrop\nfirst\n",
        );
    }

    #[test]
    fn range_carrier_gate_descriptor_ends_on_explicit_return_arm() {
        carrier_scope_layout(
            "descriptor-return",
            "if(flag){read(part);return}else{read(part)}",
            "{borrow val part=take(root,1);if(flag){read(part);return}else{read(part)};read(part)}",
            b"first\ndrop\nsecond\ndrop\nfirst\nfirst\nfirst\ndrop\nsecond\ndrop\nfirst\n",
        );
    }

    #[test]
    fn range_carrier_gate_composes_return_and_normal_scope_end() {
        carrier_scope_layout(
            "return-and-scope-end",
            "if(flag){read(child);return}",
            "{borrow val parent=take(root,1);borrow val child=(parent);if(flag){read(child);return};read(child)}\nif(!flag){println(\"after\")}",
            b"first\ndrop\nsecond\ndrop\nfirst\nfirst\nafter\ndrop\nsecond\ndrop\nfirst\n",
        );
    }

    #[test]
    fn range_carrier_gate_return_precedes_child_acquisition() {
        carrier_scope_layout(
            "return-before-child",
            "if(flag){read(parent);return}",
            "borrow val parent=take(root,1)\nif(flag){read(parent);return}\nborrow val child=(parent);read(child);println(\"continued\")",
            b"first\ndrop\nsecond\ndrop\nfirst\nfirst\ncontinued\ndrop\nsecond\ndrop\nfirst\n",
        );
    }

    #[test]
    fn range_carrier_gate_descriptor_ends_on_unbraced_return() {
        carrier_scope_layout(
            "unbraced-return",
            "if(flag) return",
            "borrow val part=take(root,1)\nif(flag) return\nread(part)",
            b"drop\nsecond\ndrop\nfirst\nfirst\ndrop\nsecond\ndrop\nfirst\n",
        );
    }

    #[test]
    fn range_carrier_direct_return_after_scope() {
        carrier_scope_layout(
            "direct-return-after-scope",
            "return",
            "{borrow val parent=take(root,1);borrow val child=(parent);read(child)}\nreturn",
            b"first\ndrop\nsecond\ndrop\nfirst\nfirst\ndrop\nsecond\ndrop\nfirst\n",
        );
    }

    #[test]
    fn range_carrier_direct_return_in_binding_scope() {
        carrier_scope_layout(
            "direct-return-same-scope",
            "return",
            "borrow val parent=take(root,1)\nborrow val child=(parent)\nread(child)\nreturn",
            b"first\ndrop\nsecond\ndrop\nfirst\nfirst\ndrop\nsecond\ndrop\nfirst\n",
        );
    }

    #[test]
    fn range_carrier_direct_return_skips_unreachable_use() {
        carrier_scope_layout(
            "direct-return-unreachable-use",
            "return",
            "borrow val part=take(root,1)\nreturn\nread(part)",
            b"drop\nsecond\ndrop\nfirst\ndrop\nsecond\ndrop\nfirst\n",
        );
    }

    #[test]
    fn range_carrier_direct_value_return_reads_live_child() {
        let consumer = r#"
    class Item(val text:String){deinit(){println("drop");println(this.text)}}
    fun count(view:View<Item>):Int{for(item in view){println(item.text)};return 7}
    fun scan():Int{
        val root=listOf(Item("first"),Item("second"))
        borrow val parent=take(root,1)
        borrow val child=(parent)
        return count(child)
    }
    fun main():Unit{val result=scan();if(result==7){println("seven")}}
    "#;
        run_carrier_layouts(
            "direct-return-value",
            consumer,
            "return count(child)",
            b"first\ndrop\nsecond\ndrop\nfirst\nseven\n",
        );
    }
}
use support::{Pipeline, lower, lower_positive, run_unmodified};

mod boundary {
    //! Ordinary, frontend-valid source probes for the frozen capability boundary.
    use super::support::{UnitExpectation, observe_original_unit};
    use super::*;
    use crate::ssa::LoweringErrorKind;
    use crate::test_support::diagnostics::{assert_eq_diagnostic, diagnostic};

    fn reject(label: &str, consumer: &str, fragment: &str) {
        for pipeline in [Pipeline::Single, Pipeline::Unit, Pipeline::UnitReversed] {
            // A frontend diagnostic is a fixture failure, not a backend rejection.
            // Print the current source case before the shared frontend setup asserts.
            diagnostic!("boundary={label}; pipeline={pipeline:?}; expected={fragment:?}");
            if matches!(pipeline, Pipeline::Single) {
                observe_original_unit(
                    consumer,
                    fragment,
                    pipeline,
                    UnitExpectation::Reject(fragment),
                    label,
                );
                continue;
            }
            let lowered = lower(consumer, fragment, pipeline);
            let error = lowered
                .result
                .err()
                .expect("this source remains outside the bounded capability");
            let actual = error.span.and_then(|span| lowered.sources.slice(span).ok());
            assert_eq_diagnostic!(
                error.kind,
                LoweringErrorKind::UnsupportedNode,
                "{label}/{pipeline:?}: {error:?}; source={actual:?}"
            );
            assert_eq_diagnostic!(
                error.span,
                Some(lowered.expected_span),
                "{label}/{pipeline:?}: {error:?}; source={actual:?}"
            );
        }
    }

    fn scoped(statement: &str) -> String {
        format!(
            r#"
    fun check(flag:Boolean):Unit{{if(flag){{println("true")}}else{{println("false")}}}}
    fun read(view:View<String>):Unit{{}}
    fun consume(own root:List<String>):Unit{{}}
    fun scan(flag:Boolean,n:Int):Unit{{
        val root=listOf("kept")
        {{
            borrow val part=take(root,1)
            {statement}
            read(part)
        }}
        consume(root)
    }}
    fun main():Unit{{scan(true,2)}}
    "#
        )
    }

    #[test]
    fn range_local_control_rejects_if_without_else() {
        let expression = "if(flag){read(part)}";
        reject("missing-else", &scoped(expression), expression);
    }

    #[test]
    fn range_local_control_rejects_value_context_if() {
        let expression = "if(flag){read(part)}else{read(part)}";
        let consumer = scoped(&format!("sink({expression})"));
        reject(
            "unit-argument",
            &format!("fun sink(value:Unit):Unit{{}}\n{consumer}"),
            expression,
        );
    }

    #[test]
    fn range_local_control_rejects_nested_if_at_inner_expression() {
        let inner = "if(flag){read(part)}else{read(part)}";
        let outer = format!("if(flag){{{inner}}}else{{read(part)}}");
        reject("nested-if", &scoped(&outer), inner);
    }

    #[test]
    fn range_local_control_rejects_abort_arm() {
        let expression = "if(flag){error(\"stop\")}else{read(part)}";
        reject("abort-arm", &scoped(expression), expression);
    }

    #[test]
    fn range_local_control_rejects_new_binding_inside_an_arm() {
        let expression = "if(flag){borrow val child=take(part,1);read(child)}else{read(part)}";
        reject("arm-binding", &scoped(expression), expression);
    }

    #[test]
    fn range_local_control_rejects_if_nested_in_a_loop() {
        let expression = "if(flag){read(part)}else{read(part)}";
        for (label, statement) in [
            ("while-if", format!("while(flag){{{expression}}}")),
            ("for-if", format!("for(item in part){{{expression}}}")),
        ] {
            reject(label, &scoped(&statement), expression);
        }
    }

    #[test]
    fn range_local_control_rejects_different_roots_in_the_same_scope() {
        let expression = "if(flag){read(part)}else{read(part)}";
        let consumer = format!(
            r#"
    fun read(view:View<String>):Unit{{}}
    fun consume(own root:List<String>):Unit{{}}
    fun scan(flag:Boolean):Unit{{
        val root=listOf("first")
        val other=listOf("second")
        {{
            borrow val part=take(root,1)
            borrow val sibling=take(other,1)
            {expression}
            read(part)
            read(sibling)
        }}
        consume(root)
        consume(other)
    }}
    fun main():Unit{{scan(true)}}
    "#
        );
        reject("different-roots", &consumer, expression);
    }

    #[test]
    fn range_local_control_keeps_unproved_alias_control_gates() {
        let branch = "if(flag){read(part)}else{read(part)}";
        let ordinary = "val text=\"kept\";borrow val alias=text;println(alias)";
        reject(
            "ordinary-if",
            &scoped(&format!("{ordinary};{branch}")),
            branch,
        );

        let metadata = "borrow val alias=part;read(alias)";
        for (kind, statement, fragment) in [
            ("eq", "check(n==2)", "n==2"),
            ("lt", "check(n<3)", "n<3"),
            ("if", branch, branch),
        ] {
            reject(
                &format!("metadata-{kind}"),
                &scoped(&format!("{metadata};{statement}")),
                fragment,
            );
        }
    }

    #[test]
    fn range_local_control_accepts_int_comparisons_with_ordinary_root_alias() {
        let binding = "val text=\"kept\";borrow val alias=text;println(alias)";
        for (label, expression, expected) in [
            ("eq", "n==2", "true"),
            ("lt", "n<3", "true"),
            ("ne", "n!=0", "true"),
            ("le", "n<=0", "false"),
            ("gt", "n>0", "true"),
            ("ge", "n>=0", "true"),
        ] {
            let source = scoped(&format!("{binding};check({expression})"));
            let expected = format!("kept\n{expected}\n");
            for pipeline in [
                Pipeline::Single,
                Pipeline::Unit,
                Pipeline::UnitReversed,
                Pipeline::SingleConstant,
                Pipeline::UnitConstant,
                Pipeline::UnitConstantReversed,
            ] {
                let label = format!("ordinary-alias-{label}-{pipeline:?}");
                let lowered = lower_positive(&source, expression, pipeline);
                let (program, entry) = lowered.require_success(&label);
                run_unmodified(&program, entry, &label, expected.as_bytes());
            }
        }
    }

    #[test]
    fn range_local_control_keeps_unselected_binary_gates() {
        for (label, statement, fragment) in [
            ("string-concat", "println(\"a\"+\"b\")", "\"a\"+\"b\""),
            ("long-equality", "check(2L==4L)", "2L==4L"),
        ] {
            reject(label, &scoped(statement), fragment);
        }
    }

    #[test]
    fn range_local_control_rejects_hidden_cfg_in_condition_or_argument() {
        let nullable_if = "if(n!!==2){read(part)}else{read(part)}";
        let nullable = format!(
            r#"
    fun read(view:View<String>):Unit{{}}
    fun consume(own root:List<String>):Unit{{}}
    fun scan(n:Int?):Unit{{
        val root=listOf("kept")
        {{
            borrow val part=take(root,1)
            {nullable_if}
            read(part)
        }}
        consume(root)
    }}
    fun main():Unit{{scan(2)}}
    "#
        );
        reject("nullable-condition", &nullable, nullable_if);

        let indexed_if = "if(values[0]==2){read(part)}else{read(part)}";
        reject(
            "indexed-condition",
            &scoped(&format!("val values=listOf(2);{indexed_if}")),
            indexed_if,
        );

        let interpolation_if = "if(flag){println(\"${n}\")}else{read(part)}";
        reject(
            "interpolated-argument",
            &scoped(interpolation_if),
            interpolation_if,
        );
    }

    #[test]
    fn range_local_control_rejects_old_call_prefix_exit_inside_a_new_if() {
        let expression =
            "if(flag){readCount(take(part,1),if(flag){return}else{0})}else{read(part)}";
        let consumer = format!(
            "fun readCount(view:View<String>,n:Int):Unit{{}}\n{}",
            scoped(expression)
        );
        reject("call-prefix-return-in-arm", &consumer, expression);
    }

    #[test]
    fn range_local_control_rejects_carrier_call_results() {
        let carrier = "if(flag){read(take(part,1))}else{read(part)}";
        reject("new-carrier-in-arm", &scoped(carrier), carrier);
    }
}

mod selection {
    //! Observable condition/arm selection without instrumentation or rewritten IR.
    use super::*;

    const SOURCE: &str = r#"
    fun mark(flag:Boolean):Boolean{println("condition");return flag}
    fun read(view:View<String>):Unit{for(item in view){println(item)}}
    fun consume(own root:List<String>):Unit{println("consume")}
    fun scan(flag:Boolean):Unit{
        val root=listOf("kept","excluded")
        {
            borrow val part=take(root,1)
            if(mark(flag)){println("then");read(part)}else{println("else");read(part)}
            read(part)
        }
        consume(root)
    }
    fun main():Unit{scan(true);scan(false)}
    "#;

    fn selection_case(pipeline: Pipeline) {
        let branch =
            "if(mark(flag)){println(\"then\");read(part)}else{println(\"else\");read(part)}";
        run_selection(pipeline, SOURCE, branch, None);
    }

    fn comparison_selection_case(pipeline: Pipeline, operator: ComparisonOperator) {
        let condition = match operator {
            ComparisonOperator::Equal => "mark(n)==2",
            ComparisonOperator::LessThan => "mark(n)<3",
            _ => unreachable!("only the two frozen Int comparisons are selected"),
        };
        let branch = format!(
            "if({condition}){{println(\"then\");read(part)}}else{{println(\"else\");read(part)}}"
        );
        let source = SOURCE
            .replace(
                "fun mark(flag:Boolean):Boolean{println(\"condition\");return flag}",
                "fun mark(n:Int):Int{println(\"condition\");return n}",
            )
            .replace("fun scan(flag:Boolean)", "fun scan(n:Int)")
            .replace("if(mark(flag))", &format!("if({condition})"))
            .replace("scan(true);scan(false)", "scan(2);scan(4)");
        run_selection(pipeline, &source, &branch, Some(operator));
    }

    fn run_selection(
        pipeline: Pipeline,
        source: &str,
        branch: &str,
        operator: Option<ComparisonOperator>,
    ) {
        let label = format!("{pipeline:?}-{operator:?}-observable-selection");
        let lowered = lower_positive(source, branch, pipeline);
        let branch_span = lowered.expected_span;
        let condition_start = branch_span.start() + "if(".len();
        let call_expression = if operator.is_some() {
            "mark(n)"
        } else {
            "mark(flag)"
        };
        let condition_expression = match operator {
            Some(ComparisonOperator::Equal) => "mark(n)==2",
            Some(ComparisonOperator::LessThan) => "mark(n)<3",
            None => "mark(flag)",
            _ => unreachable!(),
        };
        let condition_span = lowered
            .sources()
            .span(
                branch_span.source_id(),
                condition_start,
                condition_start + condition_expression.len(),
            )
            .unwrap();
        let call_span = lowered
            .sources()
            .span(
                branch_span.source_id(),
                condition_start,
                condition_start + call_expression.len(),
            )
            .unwrap();
        let (program, entry) = lowered.require_success(&label);
        let function = program.modules[0]
            .functions
            .iter()
            .find(|function| {
                function.blocks.iter().any(|block| {
                    block.terminator.as_ref().is_some_and(|terminator| {
                        terminator.origin.span() == branch_span
                            && matches!(terminator.kind, TerminatorKind::Conditional { .. })
                    })
                })
            })
            .expect("the selected source if must retain its exact conditional span");
        let terminator = function
            .blocks
            .iter()
            .filter_map(|block| block.terminator.as_ref())
            .find(|terminator| {
                terminator.origin.span() == branch_span
                    && matches!(terminator.kind, TerminatorKind::Conditional { .. })
            })
            .unwrap();
        let TerminatorKind::Conditional { condition, .. } = terminator.kind else {
            unreachable!()
        };
        let calls = function
            .instructions
            .iter()
            .filter(|instruction| {
                instruction.origin.span() == call_span
                    && matches!(instruction.operation, Operation::DirectCall { .. })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            calls.len(),
            1,
            "the condition must be evaluated exactly once"
        );
        if let Some(expected) = operator {
            let compare = function
                .instructions
                .iter()
                .find(|instruction| {
                    instruction.origin.span() == condition_span
                        && matches!(instruction.operation, Operation::Compare { .. })
                })
                .expect("the exact Int comparison expression must drive the if");
            let Operation::Compare {
                operator: actual,
                left,
                right,
            } = compare.operation
            else {
                unreachable!()
            };
            assert_eq!(actual, expected);
            assert_eq!(calls[0].results, [EntityId::Value(left)]);
            assert_eq!(compare.results, [EntityId::Value(condition)]);
            for operand in [left, right] {
                let ty = function
                    .entity(EntityId::Value(operand))
                    .unwrap()
                    .ty
                    .semantic_type();
                assert_eq!(
                    program.modules[0].type_kind(ty),
                    Some(&SsaTypeKind::Integer {
                        bits: 32,
                        signed: true
                    })
                );
            }
        } else {
            assert_eq!(calls[0].results, [EntityId::Value(condition)]);
        }
        let condition_type = function
            .entity(EntityId::Value(condition))
            .unwrap()
            .ty
            .semantic_type();
        assert_eq!(
            program.modules[0].type_kind(condition_type),
            Some(&SsaTypeKind::Boolean)
        );
        // println creates ordinary temporary String owners in each arm. The general
        // verifier checks them; this fixture does not forbid their legitimate drops.
        run_unmodified(
            &program,
            entry,
            &label,
            b"condition\nthen\nkept\nkept\nconsume\ncondition\nelse\nkept\nkept\nconsume\n",
        );
    }

    #[test]
    fn range_local_control_observable_selection_single() {
        selection_case(Pipeline::Single);
    }
    #[test]
    fn range_local_control_observable_selection_unit() {
        selection_case(Pipeline::Unit);
    }
    #[test]
    fn range_local_control_observable_selection_unit_reversed() {
        selection_case(Pipeline::UnitReversed);
    }

    macro_rules! comparison_selection_tests {
        ($single:ident, $unit:ident, $reversed:ident, $operator:ident) => {
            #[test]
            fn $single() {
                comparison_selection_case(Pipeline::Single, ComparisonOperator::$operator);
            }
            #[test]
            fn $unit() {
                comparison_selection_case(Pipeline::Unit, ComparisonOperator::$operator);
            }
            #[test]
            fn $reversed() {
                comparison_selection_case(Pipeline::UnitReversed, ComparisonOperator::$operator);
            }
        };
    }

    comparison_selection_tests!(
        range_local_control_eq_if_selection_single,
        range_local_control_eq_if_selection_unit,
        range_local_control_eq_if_selection_unit_reversed,
        Equal
    );
    comparison_selection_tests!(
        range_local_control_lt_if_selection_single,
        range_local_control_lt_if_selection_unit,
        range_local_control_lt_if_selection_unit_reversed,
        LessThan
    );
}

#[path = "local_comparisons.rs"]
pub(in crate::ssa) mod comparisons;

#[derive(Clone, Copy, Debug)]
pub(in crate::ssa) enum Position {
    BeforeBinding,
    WhileLive,
    AfterEnds,
}

pub(in crate::ssa) fn comparison_case(
    pipeline: Pipeline,
    position: Position,
    operator: ComparisonOperator,
) {
    let expression = match operator {
        ComparisonOperator::Equal => "n==2",
        ComparisonOperator::LessThan => "n<3",
        _ => unreachable!("the frozen slice has only Int equality and less-than"),
    };
    let comparison = format!("check({expression})");
    let (body, expected): (String, &[u8]) = match position {
        Position::BeforeBinding => (
            format!("{comparison}\n{{borrow val part=take(root,1);read(part)}}"),
            b"true\nkept\nconsume\nfalse\nkept\nconsume\n",
        ),
        Position::WhileLive => (
            format!("{{borrow val part=take(root,1);read(part);{comparison};read(part)}}"),
            b"kept\ntrue\nkept\nconsume\nkept\nfalse\nkept\nconsume\n",
        ),
        Position::AfterEnds => (
            format!("{{borrow val part=take(root,1);read(part)}}\n{comparison}"),
            b"kept\ntrue\nconsume\nkept\nfalse\nconsume\n",
        ),
    };
    // check has no borrow binding: its if cannot trigger scan's binding gate.
    let consumer = format!(
        r#"
fun check(flag:Boolean):Unit{{if(flag){{println("true")}}else{{println("false")}}}}
fun read(view:View<String>):Unit{{for(item in view){{println(item)}}}}
fun consume(own root:List<String>):Unit{{println("consume")}}
fun scan(n:Int):Unit{{
    val root=listOf("kept","excluded")
    {body}
    consume(root)
}}
fun main():Unit{{scan(2);scan(4)}}
"#
    );
    let label = format!("{pipeline:?}-{position:?}-{operator:?}");
    let lowered = lower_positive(&consumer, expression, pipeline);
    let comparison_span = lowered.expected_span;
    let (program, entry) = lowered.require_success(&label);
    let module = &program.modules[0];
    let instruction = module
        .functions
        .iter()
        .flat_map(|function| &function.instructions)
        .find(|instruction| {
            instruction.origin.span() == comparison_span
                && matches!(instruction.operation, Operation::Compare { .. })
        })
        .expect("the Int comparison must produce a Compare at its exact expression span");
    let function = &module.functions[instruction.id.function().index()];
    let Operation::Compare {
        operator: actual,
        left,
        right,
    } = instruction.operation
    else {
        unreachable!()
    };
    assert_eq!(actual, operator);
    for operand in [left, right] {
        let EntityType::Value(ty) = function.entity(EntityId::Value(operand)).unwrap().ty else {
            panic!("comparison operand must be a value")
        };
        assert_eq!(
            module.type_kind(ty),
            Some(&SsaTypeKind::Integer {
                bits: 32,
                signed: true
            })
        );
    }
    let [result] = instruction.results.as_slice() else {
        panic!("one Boolean comparison result")
    };
    let EntityType::Value(ty) = function.entity(*result).unwrap().ty else {
        panic!("comparison result must be a value")
    };
    assert_eq!(module.type_kind(ty), Some(&SsaTypeKind::Boolean));
    assertions::comparison_lifetime(&program, comparison_span, position);
    run_unmodified(&program, entry, &label, expected);
}

// Each position/operator/pipeline is independently selectable and reaches its
// own failure, so libtest collects all three positions even on the initial gate.
macro_rules! comparison_tests {
    ($single:ident, $unit:ident, $reversed:ident, $position:ident, $operator:ident) => {
        #[test]
        fn $single() {
            comparison_case(
                Pipeline::Single,
                Position::$position,
                ComparisonOperator::$operator,
            );
        }
        #[test]
        fn $unit() {
            comparison_case(
                Pipeline::Unit,
                Position::$position,
                ComparisonOperator::$operator,
            );
        }
        #[test]
        fn $reversed() {
            comparison_case(
                Pipeline::UnitReversed,
                Position::$position,
                ComparisonOperator::$operator,
            );
        }
    };
}

comparison_tests!(
    range_local_control_eq_before_single,
    range_local_control_eq_before_unit,
    range_local_control_eq_before_unit_reversed,
    BeforeBinding,
    Equal
);
comparison_tests!(
    range_local_control_eq_live_single,
    range_local_control_eq_live_unit,
    range_local_control_eq_live_unit_reversed,
    WhileLive,
    Equal
);
comparison_tests!(
    range_local_control_eq_after_single,
    range_local_control_eq_after_unit,
    range_local_control_eq_after_unit_reversed,
    AfterEnds,
    Equal
);
comparison_tests!(
    range_local_control_lt_before_single,
    range_local_control_lt_before_unit,
    range_local_control_lt_before_unit_reversed,
    BeforeBinding,
    LessThan
);
comparison_tests!(
    range_local_control_lt_live_single,
    range_local_control_lt_live_unit,
    range_local_control_lt_live_unit_reversed,
    WhileLive,
    LessThan
);
comparison_tests!(
    range_local_control_lt_after_single,
    range_local_control_lt_after_unit,
    range_local_control_lt_after_unit_reversed,
    AfterEnds,
    LessThan
);

pub(in crate::ssa) const IF_SOURCE: &str = r#"
fun read(view:View<String>):Unit{for(item in view){println(item)}}
fun consume(own root:List<String>):Unit{println("consume")}
fun scan(flag:Boolean):Unit{
    val root=listOf("kept","excluded")
    {
        borrow val part=take(root,1)
        if(flag){read(part)}else{read(part)}
        read(part)
    }
    consume(root)
}
fun main():Unit{scan(true);scan(false)}
"#;

pub(in crate::ssa) fn if_case(pipeline: Pipeline) {
    let label = format!("{pipeline:?}-if-both-arms");
    let lowered = lower_positive(IF_SOURCE, "if(flag){read(part)}else{read(part)}", pipeline);
    let branch_span = lowered.expected_span;
    let (program, entry) = lowered.require_success(&label);
    assertions::if_transport(&program, branch_span, 1);
    run_unmodified(
        &program,
        entry,
        &label,
        b"kept\nkept\nconsume\nkept\nkept\nconsume\n",
    );
}

#[test]
pub(in crate::ssa) fn range_local_control_if_both_arms_single() {
    if_case(Pipeline::Single);
}
#[test]
pub(in crate::ssa) fn range_local_control_if_both_arms_unit() {
    if_case(Pipeline::Unit);
}
#[test]
pub(in crate::ssa) fn range_local_control_if_both_arms_unit_reversed() {
    if_case(Pipeline::UnitReversed);
}

#[path = "local_short_circuit.rs"]
pub(in crate::ssa) mod short_circuit;
