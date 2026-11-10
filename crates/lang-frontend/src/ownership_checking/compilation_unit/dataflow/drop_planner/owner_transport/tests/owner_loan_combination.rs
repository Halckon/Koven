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
    (
        "consumer",
        "fun view(source: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun loans(): Unit { val owner = \"kept\"; consume(view(owner)) }",
    ),
];

// Only resolve carried public handles. Descriptor equality is checked afterward,
// never used to discover a dependency or to claim a dynamic activation identity.
fn assert_consumer_dependencies(
    observations: &[crate::ownership_checking::UnitLoanActionObservation],
    transports: &[crate::ownership_checking::UnitBorrowCallTransportFact],
) {
    assert_eq!(
        transports.len(),
        1,
        "one real synchronous consumer transport"
    );
    let mut count = 0;
    for observation in observations {
        for (retired_index, action) in observation.actions().iter().enumerate() {
            let Action::BorrowConsumerRetired(retirement) = action else {
                continue;
            };
            count += 1;
            assert_eq!(retirement.transaction(), &transports[0]);
            let [consumer, source] = retirement.transaction().ended_loans();
            let source_id = retirement
                .source_begin()
                .expect("actual source acquisition");
            let transfer_id = retirement.transfer().expect("actual source transfer");
            let consumer_id = retirement
                .consumer_begin()
                .expect("actual consumer acquisition");
            assert_eq!(
                observation.action(&source_id),
                Some(&Action::Begin(source.clone()))
            );
            assert_eq!(
                observation.action(&consumer_id),
                Some(&Action::Begin(consumer.clone()))
            );
            assert_eq!(
                observation.action(&transfer_id),
                Some(&Action::BorrowConsumerTransfer {
                    source: source.clone(),
                    consumer: consumer.call(),
                    argument: consumer.argument(),
                })
            );
            let indices = [&source_id, &transfer_id, &consumer_id].map(|id| {
                (0..observation.actions().len())
                    .find(|&index| observation.action_id(index).as_ref() == Some(id))
                    .expect("same catalog public handle")
            });
            assert!(
                indices[0] < indices[1] && indices[1] < indices[2] && indices[2] < retired_index
            );
            assert_eq!(
                observation.coverage(),
                crate::ownership_checking::UnitLoanActionCoverage::Partial
            );
            assert_eq!(source.call().source_unit(), observation.source());
            assert_eq!(consumer.call().source_unit(), observation.source());
        }
    }
    assert_eq!(count, 1, "nonempty acquisition-to-retirement evidence");
}

#[test]
fn four_switches_preserve_b_consumer_and_all_legacy_products() {
    // Keep B byte-for-byte intact, including its bounded suffix. Loan functions
    // are siblings; this does not claim flat loan regions admit B's loop shape.
    for (kind, loan) in LOANS {
        let text = format!("{}\n{loan}", loop_replace_break::SOURCE);
        let fixture = Fixture::new(&[("root.ko", &text)]);
        let owned = fixture.shared();
        assert!(owned.clone().validate().is_ok(), "{kind}");
        if *kind == "consumer" {
            assert_consumer_dependencies(
                owned.loan_action_observations(),
                owned.borrow_call_transports(),
            );
        }
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
                    if !matches!(*kind, "result" | "consumer") {
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
                        "consumer" => assert_consumer_dependencies(
                            observations,
                            &actual.borrow_call_transports,
                        ),
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
