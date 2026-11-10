//! Four switches over the same admitted checker, not a new mixed-region grammar.
use super::*;
use crate::ownership_checking::{UnitLoanAction as Action, UnitLoanActionCoverage as Coverage};

const LOANS: &[(&str, &str)] = &[
    (
        "temporary",
        "fun inspect(value: String): Int = 1\nfun loans(): Unit { inspect(\"temporary\") }",
    ),
    (
        "named",
        "fun inspect(value: String): Int = 1\nfun loans(own source: String): Unit { inspect(source); inspect(source) }",
    ),
    (
        "receiver",
        "class Node { fun touch(): Int = 1 }\nfun loans(own node: Node): Unit { node.touch(); node.touch() }",
    ),
    (
        "result",
        "fun view(source: String): borrow String from source = source\nfun loans(): Unit { val owner = \"kept\"; borrow val parent = view(owner); borrow val child = view(parent); println(child) }",
    ),
];

#[test]
fn four_switches_preserve_b_consumer_and_all_legacy_products() {
    // Keep B byte-for-byte intact, including its bounded suffix. Loan functions
    // are siblings; this does not claim flat loan regions admit B's loop shape.
    for (kind, loan) in LOANS {
        let text = format!("{}\n{loan}", loop_replace_break::SOURCE);
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok(), "{kind}");
        let b = Fixture::new(&[("b.ko", loop_replace_break::SOURCE)]);
        loop_replace_break::check(&b, 0, &b.shared());
        compare_and_inspect(&fixture, &fixture.parts(&owned), |checker| {
            let baseline =
                super::super::super::plan_with_observations(checker, false, false).unwrap();
            assert!(baseline.owner_observations.is_empty());
            assert!(baseline.loan_action_observations.is_empty());
            for (owner, loan) in [(false, false), (true, false), (false, true), (true, true)] {
                let mut actual =
                    super::super::super::plan_with_observations(checker, owner, loan).unwrap();
                assert_eq!(actual.owner_observations.is_empty(), !owner);
                assert_eq!(actual.loan_action_observations.is_empty(), !loan);
                if owner {
                    let boundary = actual.owner_observations[0]
                        .replace_break_observation_v1()
                        .expect("original B");
                    let consumer = boundary
                        .suffix_consumer_v1()
                        .expect("actual take/bind/drop");
                    assert_ne!(
                        boundary.condition_false_exit().owner(),
                        boundary.break_boundary().owner()
                    );
                    assert_eq!(
                        consumer.incoming(),
                        &[
                            boundary.condition_false_exit().clone(),
                            boundary.break_boundary().clone()
                        ]
                    );
                    use crate::ownership_checking::UnitOwnerReplaceBreakConsumerActionV1 as A;
                    let (statement, initializer) =
                        loop_replace_break::declaration(&fixture, 0, "retired");
                    let retired = fixture.local(0, "retired");
                    assert_eq!(
                        consumer.actions(),
                        [
                            A::Take(initializer, fixture.local(0, "held")),
                            A::Bind(initializer, retired),
                            A::Drop(
                                crate::ownership_checking::UnitDropPoint::AfterStatement(statement),
                                retired
                            ),
                        ]
                    );
                    // The independent B oracle above remains unchanged. Here the
                    // combined fixture checks exact actions and every legacy field.
                }
                if loan {
                    let observations = &actual.loan_action_observations;
                    if *kind != "result" {
                        assert!(
                            observations.iter().any(|o| o
                                .regions()
                                .iter()
                                .any(|r| r.coverage() == Coverage::Complete)),
                            "{kind}"
                        );
                    }
                    // Result handoff/binding evidence does not promote Partial to Complete.
                    let actions: Vec<_> = observations.iter().flat_map(|o| o.actions()).collect();
                    match *kind {
                        "temporary" | "named" => {
                            assert!(actions.iter().any(|a| matches!(a, Action::Begin(_))))
                        }
                        "receiver" => {
                            assert!(actions.iter().any(|a| matches!(a, Action::ReceiverEnd(_))))
                        }
                        "result" => {
                            assert!(
                                actions
                                    .iter()
                                    .any(|a| matches!(a, Action::BorrowHandoffCommitted(_)))
                            );
                            assert!(
                                actions
                                    .iter()
                                    .any(|a| matches!(a, Action::BorrowBindingEnd(_)))
                            );
                        }
                        _ => unreachable!(),
                    }
                }
                actual.owner_observations.clear();
                actual.loan_action_observations.clear();
                assert_eq!(
                    actual, baseline,
                    "full old Analysis: {kind}, owner={owner}, loan={loan}"
                );
            }
        });
    }
}
