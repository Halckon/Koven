//! Field/Group 临时父 Borrow 的真实结束事件与完整父 owner 清理。
use super::*;
use lang_frontend::ast::ExpressionId;

pub(super) fn expression(observed: &Observed, text: &str) -> ExpressionId {
    let matches = observed
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(id, node)| (observed.sources.slice(node.span()) == Ok(text)).then_some(id))
        .collect::<Vec<_>>();
    let [expression] = matches.as_slice() else {
        panic!("one independently selected AST expression for {text}: {matches:?}")
    };
    *expression
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Cleanup<'a> {
    Returned(&'a str),
    Transfer(&'a str),
}

pub(super) fn assert_field_cleanup(
    observed: &Observed,
    call_text: &str,
    fields: &[(&str, &str)],
    expected_drops: &[(Cleanup<'_>, &str)],
) {
    assert_units_admitted(observed);
    // 期望的 parent、call、point 均先由独立源码/AST 选取，不能从 loan/drop 产物反推。
    let call_id = expression(observed, call_text);
    let call_node = observed.parsed.ast().expressions().get(call_id).unwrap();
    let Expression::Call { arguments, .. } = call_node.payload() else {
        panic!("actual source call")
    };
    for (unit, names) in observed.units.iter().zip(&observed.unit_names) {
        let [source] = names.names().index().source_units() else {
            panic!("one source in each real Unit input")
        };
        assert_eq!(source.source_id(), observed.parsed.source_id());
        let qualify = |id| UnitExpressionId::new(source.id(), id);
        let call = qualify(call_id);
        let loans = unit
            .loans()
            .iter()
            .filter(|loan| loan.call() == call)
            .collect::<Vec<_>>();
        assert_eq!(loans.len(), fields.len());
        for (field_text, parent_text) in fields {
            let field_id = expression(observed, field_text);
            let parent_id = expression(observed, parent_text);
            let field = observed.parsed.ast().expressions().get(field_id).unwrap();
            let parent = observed.parsed.ast().expressions().get(parent_id).unwrap();
            let mut receiver = field_id;
            let mut fields = 0;
            while receiver != parent_id {
                receiver = match observed
                    .parsed
                    .ast()
                    .expressions()
                    .get(receiver)
                    .unwrap()
                    .payload()
                {
                    Expression::Group { expression } => *expression,
                    Expression::Member { receiver, .. } => {
                        fields += 1;
                        *receiver
                    }
                    other => {
                        panic!("only source field/group steps before the expected owner: {other:?}")
                    }
                };
            }
            assert!(fields > 0);
            assert!(matches!(
                parent.payload(),
                Expression::Call { .. } | Expression::Group { .. }
            ));
            assert!(arguments.iter().any(|argument| argument.value == field_id));
            let matching = loans
                .iter()
                .filter(|loan| loan.argument() == qualify(field_id))
                .collect::<Vec<_>>();
            let [loan] = matching.as_slice() else {
                panic!("one actual loan for {field_text}")
            };
            assert_eq!(loan.kind(), LoanKind::Shared);
            assert_eq!(
                loan.target(),
                &UnitLoanTarget::Temporary(qualify(parent_id))
            );
            assert_eq!(loan.begin_span(), field.span());
            assert_eq!(loan.end_span(), call_node.span());
            assert_eq!(parent.span().source_id(), source.source_id());
        }
        let expected = expected_drops
            .iter()
            .map(|(point, parent)| {
                let point = match point {
                    Cleanup::Returned(text) => {
                        let id = expression(observed, text);
                        assert!(matches!(
                            observed
                                .parsed
                                .ast()
                                .expressions()
                                .get(id)
                                .unwrap()
                                .payload(),
                            Expression::Call { .. }
                        ));
                        UnitDropPoint::CallReturn(qualify(id))
                    }
                    Cleanup::Transfer(text) => {
                        let id = expression(observed, text);
                        let node = observed.parsed.ast().expressions().get(id).unwrap();
                        assert!(matches!(
                            (node.payload(), *text),
                            (Expression::Return { .. }, "return")
                                | (Expression::Break { .. }, "break")
                                | (Expression::Continue { .. }, "continue")
                        ));
                        UnitDropPoint::ControlTransfer(qualify(id))
                    }
                };
                let parent = expression(observed, parent);
                (
                    point,
                    UnitDropTarget::Temporary(qualify(parent)),
                    observed
                        .parsed
                        .ast()
                        .expressions()
                        .get(parent)
                        .unwrap()
                        .span(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            unit.drops()
                .iter()
                .map(|drop| (drop.point(), drop.target(), drop.value_origin()))
                .collect::<Vec<_>>(),
            expected,
            "complete drop table, including value_origin and traversal order"
        );
        assert!(unit.conditional_receiver_drops().is_empty());
        // 每个已完成的字段 parent 对应同一真实 token，结束先于同点全部父析构。
        // 预期只取上方源码指定的 parent/point 顺序，不从实际事件或 drop 表生成。
        let mut previous_point = None;
        let mut order = 0;
        let expected_ends = expected_drops
            .iter()
            .filter_map(|(point, parent)| {
                let (field, _) = fields.iter().find(|(_, owner)| owner == parent)?;
                if previous_point != Some(*point) {
                    previous_point = Some(*point);
                    order = 0;
                }
                let point = match point {
                    Cleanup::Returned(text) => {
                        UnitDropPoint::CallReturn(qualify(expression(observed, text)))
                    }
                    Cleanup::Transfer(text) => {
                        UnitDropPoint::ControlTransfer(qualify(expression(observed, text)))
                    }
                };
                let result = (
                    qualify(expression(observed, field)),
                    UnitLoanTarget::Temporary(qualify(expression(observed, parent))),
                    point,
                    0,
                    order,
                );
                order += 1;
                Some(result)
            })
            .collect::<Vec<_>>();
        let actual_ends = unit
            .direct_call_loan_ends()
            .iter()
            .map(|end| {
                let loan = end.loan();
                assert!(unit.loans().contains(loan));
                assert_eq!(loan.call(), call);
                assert_eq!(loan.kind(), LoanKind::Shared);
                (
                    loan.argument(),
                    loan.target().clone(),
                    end.point(),
                    end.preceding_drops(),
                    end.preceding_call_loan_ends(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            actual_ends, expected_ends,
            "complete ordered field-parent end vector"
        );
        assert!(unit.exclusive_call_prefix_ends().is_empty());
    }
}

#[test]
fn temporary_field_parent_drops_once_after_normal_call_return() {
    for parent in ["Holder(\"field\")", "(Holder(\"field\"))"] {
        let field = format!("{parent}.text");
        let call = format!("read({field}, 0)");
        let observed = observe(&format!(
            "class Holder(val text: String)\nfun read(text: String, own n: Int): Unit {{}}\nfun run(): Unit {{ {call} }}"
        ));
        assert_field_cleanup(
            &observed,
            &call,
            &[(&field, parent)],
            &[(Cleanup::Returned(&call), parent)],
        );
    }
    let call = "read(Holder(1).number, 0)";
    let observed = observe(&format!(
        "class Holder(val number: Int)\nfun read(number: Int, own n: Int): Unit {{}}\nfun run(): Unit {{ {call} }}"
    ));
    assert_field_cleanup(
        &observed,
        call,
        &[("Holder(1).number", "Holder(1)")],
        &[(Cleanup::Returned(call), "Holder(1)")],
    );
}

#[test]
fn distinct_temporary_field_parents_drop_in_reverse_completion_order() {
    let call = "read(Holder(\"first\").text, Holder(\"second\").text, 0)";
    let observed = observe(&format!(
        "class Holder(val text: String)\nfun read(first: String, second: String, own n: Int): Unit {{}}\nfun run(): Unit {{ {call} }}"
    ));
    assert_field_cleanup(
        &observed,
        call,
        &[
            ("Holder(\"first\").text", "Holder(\"first\")"),
            ("Holder(\"second\").text", "Holder(\"second\")"),
        ],
        &[
            (Cleanup::Returned(call), "Holder(\"second\")"),
            (Cleanup::Returned(call), "Holder(\"first\")"),
        ],
    );
}

#[test]
fn later_argument_exit_releases_completed_field_parents_in_reverse_order() {
    for transfer in ["return", "break", "continue"] {
        let call = format!(
            "read(Holder(\"first\").text, Holder(\"second\").text, if (flag) {{ {transfer} }} else {{ 0 }})"
        );
        let body = if transfer == "return" {
            call.clone()
        } else {
            format!("while (flag) {{ {call} }}")
        };
        let observed = observe(&format!(
            "class Holder(val text: String)\nfun read(first: String, second: String, own n: Int): Unit {{}}\nfun run(flag: Boolean): Unit {{ {body} }}"
        ));
        assert_field_cleanup(
            &observed,
            &call,
            &[
                ("Holder(\"first\").text", "Holder(\"first\")"),
                ("Holder(\"second\").text", "Holder(\"second\")"),
            ],
            &[
                (Cleanup::Transfer(transfer), "Holder(\"second\")"),
                (Cleanup::Transfer(transfer), "Holder(\"first\")"),
                (Cleanup::Returned(&call), "Holder(\"second\")"),
                (Cleanup::Returned(&call), "Holder(\"first\")"),
            ],
        );
    }
}

#[test]
fn inner_loop_exit_keeps_the_outer_temporary_field_parent() {
    let call = "read(Holder(\"field\").text, if (flag) { loop { break }; 0 } else { 0 })";
    let observed = observe(&format!(
        "class Holder(val text: String)\nfun read(text: String, own n: Int): Unit {{}}\nfun run(flag: Boolean): Unit {{ {call} }}"
    ));
    assert_field_cleanup(
        &observed,
        call,
        &[("Holder(\"field\").text", "Holder(\"field\")")],
        &[(Cleanup::Returned(call), "Holder(\"field\")")],
    );
}

#[test]
fn abort_does_not_unwind_the_temporary_field_parent() {
    let call = "read(Holder(\"field\").text, error(\"stop\"))";
    let observed = observe(&format!(
        "class Holder(val text: String)\nfun read(text: String, own n: Int): Unit {{}}\nfun run(): Unit {{ {call} }}"
    ));
    assert_field_cleanup(
        &observed,
        call,
        &[("Holder(\"field\").text", "Holder(\"field\")")],
        &[],
    );
    let incomplete = "read(Holder(error(\"stop\")).text, 0)";
    let observed = observe(&format!(
        "class Holder(val text: String)\nfun read(text: String, own n: Int): Unit {{}}\nfun run(): Unit {{ {incomplete} }}"
    ));
    assert_field_cleanup(&observed, incomplete, &[], &[]);
}

#[test]
fn copyable_value_class_parent_has_no_unique_drop_obligation() {
    let call = "read(Holder(1).number, 0)";
    let observed = observe(&format!(
        "value class Holder(val number: Int)\nfun read(number: Int, own n: Int): Unit {{}}\nfun run(): Unit {{ {call} }}"
    ));
    assert_field_cleanup(&observed, call, &[("Holder(1).number", "Holder(1)")], &[]);
}

#[test]
fn grouped_temporary_field_keeps_the_original_parent_owner() {
    for (field, parent) in [
        ("(Holder(\"field\").text)", "Holder(\"field\")"),
        ("((Holder(\"field\").text))", "Holder(\"field\")"),
        ("((Holder(\"field\")).text)", "(Holder(\"field\"))"),
    ] {
        let call = format!("read({field}, 0)");
        let observed = observe(&format!(
            "class Holder(val text: String)\nfun read(text: String, own n: Int): Unit {{}}\nfun run(): Unit {{ {call} }}"
        ));
        assert_field_cleanup(
            &observed,
            &call,
            &[(field, parent)],
            &[(Cleanup::Returned(&call), parent)],
        );
    }
}

#[test]
fn nested_temporary_field_drops_the_complete_outer_owner() {
    for field in [
        "Outer(Holder(\"field\")).inner.text",
        "(Outer(Holder(\"field\")).inner.text)",
        "((Outer(Holder(\"field\")).inner).text)",
    ] {
        let call = format!("read({field}, 0)");
        let observed = observe(&format!(
            "class Holder(val text: String)\nclass Outer(val inner: Holder)\nfun read(text: String, own n: Int): Unit {{}}\nfun run(): Unit {{ {call} }}"
        ));
        assert_field_cleanup(
            &observed,
            &call,
            &[(field, "Outer(Holder(\"field\"))")],
            &[(Cleanup::Returned(&call), "Outer(Holder(\"field\"))")],
        );
    }
}

#[test]
fn nested_resource_field_prefix_survives_or_releases_on_the_actual_exit() {
    for transfer in ["return", "break", "continue", "loop { break }; 0"] {
        let first = "(Outer(Holder(\"first\")).inner.text)";
        let second = "((Outer(Holder(\"second\")).inner).text)";
        let call = format!("read({first}, {second}, if (flag) {{ {transfer} }} else {{ 0 }})");
        let body = if matches!(transfer, "break" | "continue") {
            format!("while (flag) {{ {call} }}")
        } else {
            call.clone()
        };
        let observed = observe(&format!(
            "class Holder(val text: String) {{ deinit() {{}} }}\nclass Outer(val inner: Holder)\nfun read(first: String, second: String, own n: Int): Unit {{}}\nfun run(flag: Boolean): Unit {{ {body} }}"
        ));
        let mut drops = Vec::new();
        if !transfer.starts_with("loop") {
            drops.extend([
                (Cleanup::Transfer(transfer), "Outer(Holder(\"second\"))"),
                (Cleanup::Transfer(transfer), "Outer(Holder(\"first\"))"),
            ]);
        }
        drops.extend([
            (Cleanup::Returned(&call), "Outer(Holder(\"second\"))"),
            (Cleanup::Returned(&call), "Outer(Holder(\"first\"))"),
        ]);
        assert_field_cleanup(
            &observed,
            &call,
            &[
                (first, "Outer(Holder(\"first\"))"),
                (second, "Outer(Holder(\"second\"))"),
            ],
            &drops,
        );
    }
}

#[test]
fn incomplete_outer_construction_releases_only_its_completed_resource_operand() {
    let call = "read((Outer(Holder(\"field\"), return).inner.text), 0)";
    let observed = observe(&format!(
        "class Holder(val text: String) {{ deinit() {{}} }}\nclass Outer(val inner: Holder, val number: Int)\nfun read(text: String, own n: Int): Unit {{}}\nfun run(): Unit {{ {call} }}"
    ));
    assert_field_cleanup(
        &observed,
        call,
        &[],
        &[(Cleanup::Transfer("return"), "Holder(\"field\")")],
    );
}

#[test]
fn grouped_field_inout_and_owned_move_do_not_gain_borrow_admission() {
    for (body, code, primary) in [
        (
            "change(&(Outer(Holder(\"field\")).inner.text))",
            "L0134",
            "&",
        ),
        (
            "val outer = Outer(Holder(\"field\")); consume((outer.inner.text))",
            "L0132",
            "text",
        ),
    ] {
        let observed = observe(&format!(
            "class Holder(var text: String) {{ deinit() {{}} }}\nclass Outer(val inner: Holder)\nfun change(inout text: String): Unit {{}}\nfun consume(own text: String): Unit {{}}\nfun run(): Unit {{ {body} }}"
        ));
        for unit in &observed.units {
            assert_eq!(
                unit.diagnostics()
                    .iter()
                    .map(|diagnostic| diagnostic.code().to_string())
                    .collect::<Vec<_>>(),
                [code]
            );
            assert_eq!(
                observed.sources.slice(unit.diagnostics()[0].primary_span()),
                Ok(primary)
            );
            assert!(unit.loans().is_empty());
            assert!(unit.value_deliveries().is_empty());
            assert!(unit.drops().is_empty());
            assert!(unit.clone().validate().is_err());
        }
    }
}

#[test]
fn later_invalid_argument_clears_the_completed_nested_field_prefix() {
    let observed = observe(
        "class Holder(val text: String) { deinit() {} }\nclass Outer(val inner: Holder)\nfun read(text: String, own n: Int): Unit {}\nfun consume(own text: String): Int = 0\nfun run(): Unit { val outer = Outer(Holder(\"named\")); read((Outer(Holder(\"temporary\")).inner.text), consume((outer.inner.text))) }",
    );
    for unit in &observed.units {
        let [diagnostic] = unit.diagnostics() else {
            panic!("one forbidden partial move: {:?}", unit.diagnostics())
        };
        assert_eq!(diagnostic.code().to_string(), "L0132");
        assert_eq!(
            observed.sources.slice(diagnostic.primary_span()),
            Ok("text")
        );
        assert!(unit.loans().is_empty());
        assert!(unit.value_deliveries().is_empty());
        assert!(unit.drops().is_empty());
        assert!(unit.direct_call_loan_ends().is_empty());
        assert!(unit.clone().validate().is_err());
    }
}

#[test]
fn nested_copyable_field_parent_has_no_unique_drop_obligation() {
    let field = "((Outer(Holder(1)).inner).number)";
    let call = format!("read({field}, 0)");
    let observed = observe(&format!(
        "value class Holder(val number: Int)\nvalue class Outer(val inner: Holder)\nfun read(number: Int, own n: Int): Unit {{}}\nfun run(): Unit {{ {call} }}"
    ));
    assert_field_cleanup(&observed, &call, &[(field, "Outer(Holder(1))")], &[]);
}
