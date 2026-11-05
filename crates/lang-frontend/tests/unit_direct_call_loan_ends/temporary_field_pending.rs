//! 已准入的一层临时父字段 Borrow 清理；不扩大 direct-call loan-end 观测片。
use super::*;
use lang_frontend::ast::ExpressionId;

fn expression(observed: &Observed, text: &str) -> ExpressionId {
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

#[derive(Clone, Copy)]
enum Cleanup<'a> {
    Returned(&'a str),
    Transfer(&'a str),
}

fn assert_field_cleanup(
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
            let Expression::Member { receiver, .. } = field.payload() else {
                panic!("actual one-level field operand")
            };
            assert_eq!(*receiver, parent_id);
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
        assert!(unit.direct_call_loan_ends().is_empty());
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
