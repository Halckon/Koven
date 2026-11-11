//! Ordinary leaf negation keeps actual logical evaluation and the same P/Q owner roles.
use super::replace_break_grouped_condition::{Consumer, check_transport, source};
use super::*;
use crate::ast::ExpressionId;
use crate::name_resolution::UnitReferenceTarget;
use crate::parser::{BinaryOperator, Expression, PrefixOperator};

fn peel(
    f: &Fixture,
    mut expression: ExpressionId,
    groups: usize,
    negations: usize,
) -> ExpressionId {
    let (mut actual_groups, mut actual_negations) = (0, 0);
    loop {
        match f.parsed[0]
            .ast()
            .expressions()
            .get(expression)
            .unwrap()
            .payload()
        {
            Expression::Group { expression: inner } => {
                actual_groups += 1;
                expression = *inner;
            }
            Expression::Prefix {
                operator: PrefixOperator::Not,
                operand,
                ..
            } => {
                actual_negations += 1;
                expression = *operand;
            }
            _ => break,
        }
    }
    assert_eq!((actual_groups, actual_negations), (groups, negations));
    expression
}
fn parameter(
    f: &Fixture,
    expression: ExpressionId,
    groups: usize,
    negations: usize,
    spelling: &str,
) -> ExpressionId {
    let ast = f.parsed[0].ast();
    assert_eq!(
        f.sources
            .slice(ast.expressions().get(expression).unwrap().span())
            .unwrap(),
        spelling
    );
    let expression = peel(f, expression, groups, negations);
    let node = ast.expressions().get(expression).unwrap();
    assert!(matches!(node.payload(), Expression::Name));
    assert_eq!(f.sources.slice(node.span()).unwrap(), "flag");
    let references: Vec<_> = f
        .names
        .names()
        .references()
        .iter()
        .filter(|reference| {
            reference.source_unit() == f.source(0) && reference.span() == node.span()
        })
        .collect();
    let [reference] = references.as_slice() else {
        panic!("one real parameter reference")
    };
    assert_eq!(
        reference.target(),
        &UnitReferenceTarget::Symbol(f.symbol(0, "flag"))
    );
    expression
}
fn check_leaf_not(consumer: Consumer) {
    for (operator, spelling) in [
        (BinaryOperator::LogicalAnd, "&&"),
        (BinaryOperator::LogicalOr, "||"),
    ] {
        for (left, right, left_groups, left_nots, right_groups, right_nots) in [
            ("!flag", "flag", 0, 1, 0, 0),
            ("flag", "!(flag)", 0, 0, 1, 1),
            ("(!flag)", "!((flag))", 1, 1, 2, 1),
            ("!(!flag)", "((flag))", 1, 2, 2, 0),
            ("!((!flag))", "!(!(!flag))", 2, 2, 2, 3),
        ] {
            for (prefix, suffix, groups, negations) in [("", "", 0, 0), ("!((", "))", 2, 1)] {
                let condition = format!("{prefix}{left} {spelling} {right}{suffix}");
                check_transport(consumer, &condition, |f, outer| {
                    assert_eq!(
                        f.typed.types().ordinary_boolean_outcomes(outer),
                        (true, true)
                    );
                    let logical = peel(f, outer.expression(), groups, negations);
                    let ast = f.parsed[0].ast();
                    let Expression::Binary {
                        left: left_operand,
                        operator: actual,
                        right: right_operand,
                        ..
                    } = ast.expressions().get(logical).unwrap().payload()
                    else {
                        panic!("one actual logical expression")
                    };
                    assert_eq!(*actual, operator);
                    let left_name = parameter(f, *left_operand, left_groups, left_nots, left);
                    let right_name = parameter(f, *right_operand, right_groups, right_nots, right);
                    assert_ne!(left_name, right_name);
                    assert_ne!(
                        ast.expressions().get(left_name).unwrap().span(),
                        ast.expressions().get(right_name).unwrap().span()
                    );
                    assert_ne!(outer, UnitExpressionId::new(f.source(0), left_name));
                    assert_ne!(outer, UnitExpressionId::new(f.source(0), right_name));
                });
            }
        }
    }
}
#[test]
fn leaf_not_preserves_bind_consumer_and_actual_edges() {
    check_leaf_not(Consumer::Bind);
}
#[test]
fn leaf_not_preserves_call_consumer_and_actual_edges() {
    check_leaf_not(Consumer::Call);
}
#[test]
fn leaf_not_preserves_return_consumer_and_actual_edges() {
    check_leaf_not(Consumer::Return);
}

fn check_absence(text: &str) {
    let f = Fixture::new(&[("leaf-not-neighbor.ko", text)]);
    let owned = f.shared();
    assert!(
        owned.clone().validate().is_ok(),
        "{text}: {:?}",
        owned.diagnostics()
    );
    assert!(
        owned.owner_observations().iter().all(|observation| {
            observation.replace_break_observation_v1().is_none()
                && observation.loop_observation_v1().is_none()
                && observation.invariant_loop_observation_v1().is_none()
        }),
        "{text}"
    );
    compare(&f, &f.parts(&owned));
}
#[test]
fn leaf_not_declines_neighbor_and_other_loop_shapes() {
    for consumer in [Consumer::Bind, Consumer::Call, Consumer::Return] {
        for condition in [
            "!flag && true",
            "false || !flag",
            "!(flag == true) && flag",
            "flag || !(flag != false)",
            "(!flag && flag) || flag",
            "!flag && (flag || !flag)",
            "!flag && decision()",
            "decision() || !flag",
            "(if(flag) !flag else flag) && flag",
        ] {
            let text = format!(
                "{}\nfun decision(): Boolean = true",
                source(consumer, condition)
            );
            check_absence(&text);
        }
        for condition in ["!flag && other", "other || !flag", "!flag && flag"] {
            let text = source(consumer, condition)
                .replace("flag: Boolean)", "flag: Boolean, other: Boolean)");
            check_absence(&text);
        }
    }
    for text in [
        r#"fun run(flag: Boolean): Unit { while(!flag && !flag) { val local = "inner"; break } }"#,
        r#"fun run(flag: Boolean): Unit { var held = "outer"; while(!flag || !flag) { val local = "inner"; break }; val retired = held }"#,
        r#"fun run(flag: Boolean, other: Boolean): Unit { while(flag) { val local = "inner"; if(!other && !other) { break } else { break } } }"#,
    ] {
        check_absence(text);
    }
}
