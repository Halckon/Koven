//! Ordinary grouped sources and shared checks retain actual condition and consumer identities.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitDropTarget, UnitOwnerLoopEdgeV1 as Edge, UnitOwnerOrigin,
    UnitOwnerReplaceBreakCallActionV1 as Call, UnitOwnerReplaceBreakConsumerActionV1 as Bind,
    UnitOwnerReplaceBreakReturnActionV1 as Return,
};
use crate::parser::{Expression, PrefixOperator, Statement};
use crate::type_checking::UnitStatementId;

#[derive(Clone, Copy)]
pub(super) enum Consumer {
    Bind,
    Call,
    Return,
}
pub(super) fn source(consumer: Consumer, condition: &str) -> String {
    let run = loop_replace_break::SOURCE;
    let text = match consumer {
        Consumer::Bind => run.to_owned(),
        Consumer::Call => format!(
            "fun sink(own item: String): Unit {{}}\n{}",
            run.replace("val retired = held", "sink(held)")
        ),
        Consumer::Return => run
            .replace("flag: Boolean)", "flag: Boolean): String")
            .replace("val retired = held", "return held"),
    };
    text.replace("while(flag)", &format!("while({condition})"))
}
pub(super) fn check(consumer: Consumer, condition: &str, groups: usize, negations: usize) {
    check_transport(consumer, condition, |f, outer| {
        let ast = f.parsed[0].ast();
        let mut leaf = outer.expression();
        let (mut actual_groups, mut actual_negations) = (0, 0);
        loop {
            match ast.expressions().get(leaf).unwrap().payload() {
                Expression::Group { expression } => {
                    actual_groups += 1;
                    leaf = *expression;
                }
                Expression::Prefix {
                    operator: PrefixOperator::Not,
                    operand,
                    ..
                } => {
                    actual_negations += 1;
                    leaf = *operand;
                }
                _ => break,
            }
        }
        assert_eq!((actual_groups, actual_negations), (groups, negations));
        assert!(matches!(
            ast.expressions().get(leaf).unwrap().payload(),
            Expression::Name
        ));
        assert_eq!(
            f.sources
                .slice(ast.expressions().get(leaf).unwrap().span())
                .unwrap(),
            "flag"
        );
        assert_ne!(outer, UnitExpressionId::new(f.source(0), leaf));
    });
}

pub(super) fn check_transport(
    consumer: Consumer,
    condition: &str,
    inspect: impl FnOnce(&Fixture, UnitExpressionId),
) {
    let text = source(consumer, condition);
    let f = Fixture::new(&[("grouped.ko", &text)]);
    let owned = f.shared();
    assert!(
        owned.clone().validate().is_ok(),
        "{:?}",
        owned.diagnostics()
    );
    let found: Vec<_> = owned
        .owner_observations()
        .iter()
        .filter_map(|o| o.replace_break_observation_v1())
        .collect();
    assert_eq!(
        found.len(),
        1,
        "missing real wrapped-condition ReplaceBreak product: {text}"
    );
    let boundary = found[0];
    assert!(boundary.is_consistent());
    let ast = f.parsed[0].ast();
    let loops: Vec<_> = ast
        .statements()
        .iter()
        .filter_map(|(id, node)| {
            let Statement::While {
                condition, body, ..
            } = node.payload()
            else {
                return None;
            };
            Some((
                UnitStatementId::new(f.source(0), id),
                UnitExpressionId::new(f.source(0), *condition),
                UnitStatementId::new(f.source(0), *body),
            ))
        })
        .collect();
    let [(statement, outer, body)] = loops.as_slice() else {
        panic!("one real While")
    };
    assert_eq!(
        (boundary.statement(), boundary.condition(), boundary.body()),
        (*statement, *outer, *body)
    );
    assert_eq!(
        f.sources
            .slice(ast.expressions().get(outer.expression()).unwrap().span())
            .unwrap(),
        condition
    );
    inspect(&f, *outer);
    let jump = UnitExpressionId::new(f.source(0), f.expression(0, "break"));
    assert_eq!(
        boundary.edges(),
        &[
            Edge::EnterBody(*outer, *body),
            Edge::BreakExit(
                UnitDropPoint::ControlTransfer(jump),
                UnitDropPoint::LoopExit(*statement)
            ),
            Edge::ConditionFalseExit(*outer, UnitDropPoint::LoopExit(*statement))
        ]
    );
    let p = boundary.condition_false_exit().owner();
    let q = boundary.break_boundary().owner();
    assert_ne!(p, q);
    let (_, initial) = loop_replace_break::declaration(&f, 0, "held");
    let (_, call) = loop_replace_break::declaration(&f, 0, "previous");
    let Expression::Call { arguments, .. } =
        ast.expressions().get(call.expression()).unwrap().payload()
    else {
        panic!("real replacement call")
    };
    let replacement = UnitExpressionId::new(f.source(0), arguments[1].value);
    assert_eq!(
        boundary.input_producers().definition(p).unwrap().origin(),
        UnitOwnerOrigin::String(initial)
    );
    assert_eq!(
        boundary.body_producers().definition(q).unwrap().origin(),
        UnitOwnerOrigin::String(replacement)
    );
    assert_eq!(boundary.entry(), boundary.condition_false_exit());
    assert_eq!(boundary.break_boundary(), boundary.body_completion());
    let incoming = [
        boundary.condition_false_exit().clone(),
        boundary.break_boundary().clone(),
    ];
    let held = f.local(0, "held");
    let expected_drops = match consumer {
        Consumer::Bind => {
            let selected = boundary.suffix_consumer_v1().expect("actual Bind consumer");
            let (statement, initializer) = loop_replace_break::declaration(&f, 0, "retired");
            let retired = f.local(0, "retired");
            assert_eq!(selected.incoming(), &incoming);
            assert_eq!(
                selected.actions(),
                &[
                    Bind::Take(initializer, held),
                    Bind::Bind(initializer, retired),
                    Bind::Drop(UnitDropPoint::AfterStatement(statement), retired)
                ]
            );
            assert!(
                boundary.suffix_call_consumer_v1().is_none()
                    && boundary.suffix_return_consumer_v1().is_none()
            );
            2
        }
        Consumer::Call => {
            let selected = boundary
                .suffix_call_consumer_v1()
                .expect("actual Call consumer");
            let call = UnitExpressionId::new(f.source(0), f.expression(0, "sink(held)"));
            let Expression::Call { arguments, .. } =
                ast.expressions().get(call.expression()).unwrap().payload()
            else {
                panic!("actual call")
            };
            let argument = UnitExpressionId::new(f.source(0), arguments[0].value);
            assert_eq!(selected.incoming(), &incoming);
            assert_eq!(
                selected.actions(),
                &[
                    Call::Take(argument, held),
                    Call::Call {
                        call,
                        argument,
                        target: f.symbol(0, "sink")
                    }
                ]
            );
            assert!(
                boundary.suffix_consumer_v1().is_none()
                    && boundary.suffix_return_consumer_v1().is_none()
            );
            2
        }
        Consumer::Return => {
            let selected = boundary
                .suffix_return_consumer_v1()
                .expect("actual Return consumer");
            let expression = UnitExpressionId::new(f.source(0), f.expression(0, "return held"));
            let Expression::Return {
                value: Some(payload),
                ..
            } = ast
                .expressions()
                .get(expression.expression())
                .unwrap()
                .payload()
            else {
                panic!("actual return")
            };
            let payload = UnitExpressionId::new(f.source(0), *payload);
            assert_eq!(selected.incoming(), &incoming);
            assert_eq!(
                selected.actions(),
                &[
                    Return::Take(payload, held),
                    Return::Return {
                        expression,
                        payload
                    }
                ]
            );
            assert!(
                boundary.suffix_consumer_v1().is_none()
                    && boundary.suffix_call_consumer_v1().is_none()
            );
            1
        }
    };
    assert_eq!(owned.drops().len(), expected_drops);
    assert!(
        owned
            .drops()
            .iter()
            .all(|drop| drop.target() != UnitDropTarget::Named(held))
    );
    compare(&f, &f.parts(&owned));
}
#[test]
fn grouped_replace_break_preserves_bind_consumer_and_outer_condition() {
    for depth in [1, 2, 4] {
        let condition = format!("{}flag{}", "(".repeat(depth), ")".repeat(depth));
        check(Consumer::Bind, &condition, depth, 0);
    }
}
#[test]
fn grouped_replace_break_preserves_call_consumer_and_outer_condition() {
    for depth in [1, 2, 4] {
        let condition = format!("{}flag{}", "(".repeat(depth), ")".repeat(depth));
        check(Consumer::Call, &condition, depth, 0);
    }
}
#[test]
fn grouped_replace_break_preserves_return_consumer_and_outer_condition() {
    for depth in [1, 2, 4] {
        let condition = format!("{}flag{}", "(".repeat(depth), ")".repeat(depth));
        check(Consumer::Return, &condition, depth, 0);
    }
}
#[test]
fn grouped_replace_break_keeps_general_conditions_outside_transport() {
    for consumer in [Consumer::Bind, Consumer::Call, Consumer::Return] {
        for condition in [
            "flag && !flag",
            "(flag || !flag)",
            "!((flag && (flag == true)))",
            "((flag == flag))",
            "!(!((flag != false) == true))",
        ] {
            let text = source(consumer, condition);
            let f = Fixture::new(&[("condition-neighbor.ko", &text)]);
            let owned = f.shared();
            assert!(
                owned.clone().validate().is_ok(),
                "{text}: {:?}",
                owned.diagnostics()
            );
            assert!(
                owned
                    .owner_observations()
                    .iter()
                    .all(|o| o.replace_break_observation_v1().is_none()),
                "{text}"
            );
            compare(&f, &f.parts(&owned));
        }
    }
}
