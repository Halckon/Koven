//! Ordinary single Boolean comparisons retain the real outer condition and P/Q roles.
use super::replace_break_grouped_condition::{Consumer, check_transport, source};
use super::*;
use crate::ast::ExpressionId;
use crate::name_resolution::UnitReferenceTarget;
use crate::parser::{BinaryOperator, Expression, LiteralKind, PrefixOperator};

#[derive(Clone, Copy)]
enum Wrapper {
    Group,
    Not,
}
fn wrapped(text: &str, wrappers: &[Wrapper]) -> String {
    wrappers
        .iter()
        .rev()
        .fold(text.to_owned(), |text, wrapper| match wrapper {
            Wrapper::Group => format!("({text})"),
            Wrapper::Not => format!("! {text}"),
        })
}
fn unwrap_exact(f: &Fixture, mut expression: ExpressionId, wrappers: &[Wrapper]) -> ExpressionId {
    for wrapper in wrappers {
        expression = match (
            wrapper,
            f.parsed[0]
                .ast()
                .expressions()
                .get(expression)
                .unwrap()
                .payload(),
        ) {
            (Wrapper::Group, Expression::Group { expression }) => *expression,
            (
                Wrapper::Not,
                Expression::Prefix {
                    operator: PrefixOperator::Not,
                    operand,
                    ..
                },
            ) => *operand,
            _ => panic!("actual wrapper structure must match the source"),
        };
    }
    expression
}
fn check_comparisons(consumer: Consumer) {
    use Wrapper::{Group, Not};
    let variants: &[(&[Wrapper], &[Wrapper])] = &[
        (&[], &[]),
        (&[Group, Not, Group, Group], &[Not, Group, Group]),
        (&[Not, Group, Not], &[Group, Not, Group]),
    ];
    for operator in [BinaryOperator::Equal, BinaryOperator::NotEqual] {
        let spelling = if operator == BinaryOperator::Equal {
            "=="
        } else {
            "!="
        };
        for literal in [false, true] {
            for literal_first in [false, true] {
                for &(parameter_wrappers, outer_wrappers) in variants {
                    let parameter = wrapped("flag", parameter_wrappers);
                    let comparison = if literal_first {
                        format!("{literal} {spelling} {parameter}")
                    } else {
                        format!("{parameter} {spelling} {literal}")
                    };
                    let condition = wrapped(&comparison, outer_wrappers);
                    check_transport(consumer, &condition, |f, outer| {
                        let binary = unwrap_exact(f, outer.expression(), outer_wrappers);
                        let Expression::Binary {
                            left,
                            operator: actual,
                            right,
                            ..
                        } = f.parsed[0]
                            .ast()
                            .expressions()
                            .get(binary)
                            .unwrap()
                            .payload()
                        else {
                            panic!("one actual comparison")
                        };
                        assert_eq!(*actual, operator);
                        let (parameter, value) = if literal_first {
                            (*right, *left)
                        } else {
                            (*left, *right)
                        };
                        assert!(matches!(
                            f.parsed[0].ast().expressions().get(value).unwrap().payload(),
                            Expression::Literal(LiteralKind::Boolean(actual)) if *actual == literal
                        ));
                        let parameter = unwrap_exact(f, parameter, parameter_wrappers);
                        let node = f.parsed[0].ast().expressions().get(parameter).unwrap();
                        assert!(matches!(node.payload(), Expression::Name));
                        assert_eq!(f.sources.slice(node.span()).unwrap(), "flag");
                        let references: Vec<_> = f
                            .names
                            .names()
                            .references()
                            .iter()
                            .filter(|reference| {
                                reference.source_unit() == f.source(0)
                                    && reference.span() == node.span()
                            })
                            .collect();
                        let [reference] = references.as_slice() else {
                            panic!("one real parameter reference")
                        };
                        assert_eq!(
                            reference.target(),
                            &UnitReferenceTarget::Symbol(f.symbol(0, "flag"))
                        );
                        assert_ne!(outer, UnitExpressionId::new(f.source(0), parameter));
                    });
                }
            }
        }
    }
}
#[test]
fn boolean_comparison_preserves_bind_consumer_and_actual_edges() {
    check_comparisons(Consumer::Bind);
}
#[test]
fn boolean_comparison_preserves_call_consumer_and_actual_edges() {
    check_comparisons(Consumer::Call);
}
#[test]
fn boolean_comparison_preserves_return_consumer_and_actual_edges() {
    check_comparisons(Consumer::Return);
}

fn check_absence(text: &str) {
    let f = Fixture::new(&[("comparison-neighbor.ko", text)]);
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
fn boolean_comparison_declines_neighbor_and_other_loop_shapes() {
    for consumer in [Consumer::Bind, Consumer::Call, Consumer::Return] {
        for condition in [
            "flag == flag",
            "true != false",
            "(flag == true) == false",
            "true != (flag != false)",
            "flag == (true)",
            "flag != !false",
            "flag && !flag",
            "!(flag || (flag == false))",
            "flag == decision()",
            "decision() != false",
            "(if (flag) flag else false) == true",
        ] {
            let text = format!(
                "{}\nfun decision(): Boolean = true",
                source(consumer, condition)
            );
            check_absence(&text);
        }
    }
    // These real ordinary loops must keep the original shared condition gate.
    for text in [
        r#"fun run(flag: Boolean): Unit { while(flag == true) { val local = "inner"; break } }"#,
        r#"fun run(flag: Boolean, other: Boolean): Unit { while(flag) { val local = "inner"; if(other == true) { break } else { break } } }"#,
    ] {
        check_absence(text);
    }
}
