//! Ordinary one-level logical conditions keep two real reads of one Boolean parameter.
use super::replace_break_grouped_condition::{Consumer, check_transport, source};
use super::*;
use crate::ast::ExpressionId;
use crate::name_resolution::UnitReferenceTarget;
use crate::parser::{BinaryOperator, Expression, PrefixOperator};

fn grouped_parameter(f: &Fixture, mut expression: ExpressionId, groups: usize) -> ExpressionId {
    for _ in 0..groups {
        let Expression::Group { expression: inner } = f.parsed[0]
            .ast()
            .expressions()
            .get(expression)
            .unwrap()
            .payload()
        else {
            panic!("actual parameter Group wrapper")
        };
        expression = *inner;
    }
    let node = f.parsed[0].ast().expressions().get(expression).unwrap();
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
fn check_logical(consumer: Consumer) {
    for (operator, spelling) in [
        (BinaryOperator::LogicalAnd, "&&"),
        (BinaryOperator::LogicalOr, "||"),
    ] {
        for (left_groups, right_groups, prefix, suffix, groups, negations) in [
            (0, 0, "", "", 0, 0),
            (1, 2, "!((", "))", 2, 1),
            (2, 1, "((!(", ")))", 3, 1),
            (3, 0, "!(!(", "))", 2, 2),
        ] {
            let left = format!("{}flag{}", "(".repeat(left_groups), ")".repeat(left_groups));
            let right = format!(
                "{}flag{}",
                "(".repeat(right_groups),
                ")".repeat(right_groups)
            );
            let condition = format!("{prefix}{left} {spelling} {right}{suffix}");
            check_transport(consumer, &condition, |f, outer| {
                let ast = f.parsed[0].ast();
                let mut logical = outer.expression();
                let (mut actual_groups, mut actual_negations) = (0, 0);
                loop {
                    match ast.expressions().get(logical).unwrap().payload() {
                        Expression::Group { expression } => {
                            actual_groups += 1;
                            logical = *expression;
                        }
                        Expression::Prefix {
                            operator: PrefixOperator::Not,
                            operand,
                            ..
                        } => {
                            actual_negations += 1;
                            logical = *operand;
                        }
                        _ => break,
                    }
                }
                assert_eq!((actual_groups, actual_negations), (groups, negations));
                let Expression::Binary {
                    left,
                    operator: actual,
                    right,
                    ..
                } = ast.expressions().get(logical).unwrap().payload()
                else {
                    panic!("one actual logical expression")
                };
                assert_eq!(*actual, operator);
                let left = grouped_parameter(f, *left, left_groups);
                let right = grouped_parameter(f, *right, right_groups);
                assert_ne!(
                    left, right,
                    "two real AST reads, not one reconstructed leaf"
                );
                assert_ne!(
                    ast.expressions().get(left).unwrap().span(),
                    ast.expressions().get(right).unwrap().span()
                );
                assert_ne!(outer, UnitExpressionId::new(f.source(0), left));
                assert_ne!(outer, UnitExpressionId::new(f.source(0), right));
            });
        }
    }
}
#[test]
fn short_circuit_preserves_bind_consumer_and_actual_edges() {
    check_logical(Consumer::Bind);
}
#[test]
fn short_circuit_preserves_call_consumer_and_actual_edges() {
    check_logical(Consumer::Call);
}
#[test]
fn short_circuit_preserves_return_consumer_and_actual_edges() {
    check_logical(Consumer::Return);
}

fn check_absence(text: &str) {
    let f = Fixture::new(&[("logical-neighbor.ko", text)]);
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
fn short_circuit_declines_neighbor_and_other_loop_shapes() {
    for consumer in [Consumer::Bind, Consumer::Call, Consumer::Return] {
        for condition in [
            "!(flag == true) && flag",
            "flag || !(flag != false)",
            "(flag) && ((!(flag == true)))",
            "flag && true",
            "false || flag",
            "true && false",
            "(flag == true) && flag",
            "flag || (flag != false)",
            "(flag && flag) || flag",
            "flag && (flag || flag)",
            "flag && decision()",
            "decision() || flag",
        ] {
            let text = format!(
                "{}\nfun decision(): Boolean = true",
                source(consumer, condition)
            );
            check_absence(&text);
        }
        for condition in ["flag && other", "other || flag", "flag && flag"] {
            let text = source(consumer, condition)
                .replace("flag: Boolean)", "flag: Boolean, other: Boolean)");
            check_absence(&text);
        }
    }
    // Comparison-free ordinary loop families retain their old condition contract.
    for text in [
        r#"fun run(flag: Boolean): Unit { while(flag && flag) { val local = "inner"; break } }"#,
        r#"fun run(flag: Boolean): Unit { var held = "outer"; while(flag || flag) { val local = "inner"; break }; val retired = held }"#,
        r#"fun run(flag: Boolean, other: Boolean): Unit { while(flag) { val local = "inner"; if(other && other) { break } else { break } } }"#,
    ] {
        check_absence(text);
    }
}
