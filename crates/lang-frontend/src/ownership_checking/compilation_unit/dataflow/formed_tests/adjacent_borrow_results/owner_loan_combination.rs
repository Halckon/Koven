//! Genuine multi-source rejection after both owner B and loan production.
use super::*;
use crate::ownership_checking::{CompilationUnitOwnership, UnitLoanAction as Action};

// Byte-for-byte B from owner_transport/tests/loop_replace_break.rs.
const B: &str = r#"fun run(flag: Boolean) { var held = "outer"; while(flag) { val previous = replace(&held, "next"); break }; val retired = held }"#;
const LOAN: &str = r#"fun inspect(value: String): Int = 1
class Node { fun touch(): Int = 1 }
fun view(source: String): borrow String from source = source
fun consume(item: String): Unit {}
fun loans(own node: Node): Unit {
inspect("temporary")
val owner = "kept"
inspect(owner)
node.touch()
consume(view(owner))
borrow val item = view(owner)
println(item)
}
fun complete_consumer(): Unit {
val region_owner = "region"
consume(view(region_owner))
}"#;

// Only resolve carried public handles. Descriptor equality is checked afterward,
// never used to discover a dependency or to claim a dynamic activation identity.
fn assert_consumer_dependencies(
    observations: &[crate::ownership_checking::UnitLoanActionObservation],
    transports: &[crate::ownership_checking::UnitBorrowCallTransportFact],
) {
    assert_eq!(
        transports.len(),
        2,
        "mixed Partial and separate Complete sibling transports"
    );
    let mut count = 0;
    let mut seen = Vec::new();
    for observation in observations {
        for (retired_index, action) in observation.actions().iter().enumerate() {
            let Action::BorrowConsumerRetired(retirement) = action else {
                continue;
            };
            count += 1;
            assert!(transports.contains(retirement.transaction()));
            assert!(
                !seen.contains(&retirement.transaction()),
                "distinct real consumer transactions"
            );
            seen.push(retirement.transaction());
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
    assert_eq!(
        count, 2,
        "nonempty mixed and sibling acquisition-to-retirement evidence"
    );
}

// Protocol 4 certifies the entire unfiltered local interval. Its retirement
// transaction carries consumer End then source End; neither is a new action.
fn assert_complete_consumer_region(
    fixture: &Fixture,
    file: usize,
    root_name: &str,
    observations: &[crate::ownership_checking::UnitLoanActionObservation],
) {
    use crate::ownership_checking::{
        UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction, UnitLoanActionCoverage,
        UnitLoanRegionFamily, UnitLoanTarget,
    };
    let complete: Vec<_> = observations
        .iter()
        .flat_map(|observation| {
            observation
                .regions()
                .iter()
                .filter(|region| {
                    region.protocol_version() == 4
                        && region.coverage() == UnitLoanActionCoverage::Complete
                })
                .map(move |region| (observation, region))
        })
        .collect();
    let [(observation, region)] = complete.as_slice() else {
        panic!("one real Complete protocol 4 sibling")
    };
    assert!(observation.owns_region(region));
    assert_eq!(observation.coverage(), UnitLoanActionCoverage::Partial);
    assert_eq!(
        region.families(),
        &[
            UnitLoanRegionFamily::SynchronousResultContinuation,
            UnitLoanRegionFamily::NamedSourceDrop
        ]
    );
    let source_call = UnitExpressionId::new(
        fixture.source(file),
        fixture.expression(file, &format!("view({root_name})")),
    );
    let consumer_call = UnitExpressionId::new(
        fixture.source(file),
        fixture.expression(file, &format!("consume(view({root_name}))")),
    );
    assert_eq!(region.call(), consumer_call);
    assert_eq!(
        region.callable().unwrap().source_unit(),
        fixture.source(file)
    );
    let ids: Vec<_> = region.action_ids().collect();
    let records: Vec<_> = ids
        .iter()
        .map(|id| {
            observation
                .action(id)
                .expect("public region handle resolves")
        })
        .collect();
    let [
        Action::Begin(source),
        Action::BorrowConsumerTransfer {
            source: moved,
            consumer: call,
            argument,
        },
        Action::Begin(consumer),
        Action::BorrowConsumerRetired(retired),
        Action::Cleanup {
            point,
            action: UnitIterationCleanupAction::Drop(drop),
        },
    ] = records.as_slice()
    else {
        panic!("complete unfiltered acquisition/retirement/destruction interval")
    };
    assert_eq!(source.call(), source_call);
    assert_eq!(consumer.call(), consumer_call);
    assert_eq!(consumer.argument(), source_call);
    assert_eq!(*call, consumer_call);
    assert_eq!(*argument, source_call);
    assert_eq!(moved, source);
    assert_eq!(source.target(), consumer.target());
    let root = fixture.local(file, root_name);
    assert!(
        matches!(source.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == root)
    );
    assert_eq!(retired.source_begin().as_ref(), Some(&ids[0]));
    assert_eq!(retired.transfer().as_ref(), Some(&ids[1]));
    assert_eq!(retired.consumer_begin().as_ref(), Some(&ids[2]));
    assert_eq!(retired.transaction().ended_loans(), [consumer, source]);
    assert_eq!(*point, UnitDropPoint::CallReturn(consumer_call));
    assert_eq!(drop.point(), *point);
    assert_eq!(drop.target(), UnitDropTarget::Named(root));
    let declaration = fixture.names.names().source_units()[root.source_unit().index()]
        .resolution()
        .symbols()[root.symbol().index()]
    .span();
    assert_eq!(drop.value_origin(), declaration);
    let start = (0..observation.actions().len())
        .find(|&i| observation.action_id(i).as_ref() == Some(&ids[0]))
        .unwrap();
    for (offset, id) in ids.iter().enumerate() {
        assert_eq!(
            observation.action_id(start + offset).as_ref(),
            Some(id),
            "full interval retains exact order"
        );
    }
}

fn assert_public_cleared(shared: &CompilationUnitOwnership) {
    assert!(shared.loan_action_observations().is_empty());
    assert!(shared.string_clone_loan_ends().is_empty());
    assert!(shared.formed_owned_locals().is_empty());
    assert!(shared.nullable_whens().is_empty());
    assert!(shared.borrow_return_origins().is_empty());
    assert!(shared.direct_call_loan_ends().is_empty());
    assert!(shared.borrow_call_handoffs().is_empty());
    assert!(shared.borrow_call_prefix_ends().is_empty());
    assert!(shared.borrow_results().bindings().is_empty());
    assert!(shared.borrow_results().ends().is_empty());
    assert!(shared.borrow_results().forwarded_source_loans().is_empty());
    assert!(shared.borrow_results().range_return_origins().is_empty());
    assert!(shared.borrow_results().range_uses().is_empty());
    assert!(shared.borrow_results().call_uses().is_empty());
    assert!(shared.borrow_forward_transports().is_empty());
    assert!(shared.receiver_call_prefix_ends().is_empty());
    assert!(shared.exclusive_call_prefix_ends().is_empty());
    assert!(shared.receiver_call_return_ends().is_empty());
    assert!(shared.borrow_call_transports().is_empty());
    assert!(shared.borrow_binding_transports().is_empty());
}

fn reject(suffix: &str, deferred: bool) {
    let accepted = Fixture::new(&[("a.ko", B), ("b.ko", LOAN)]);
    let shared = accepted.shared();
    shared.clone().validate().unwrap();
    let raw = accepted.dataflow(&accepted.parts(&shared));
    assert!(raw.diagnostics.is_empty() && raw.deferred.is_empty());
    let boundary = raw.owner_observations[0]
        .replace_break_observation_v1()
        .expect("actual B before failure");
    let consumer = boundary
        .suffix_consumer_v1()
        .expect("actual Q suffix consumer");
    assert_eq!(
        consumer.incoming(),
        &[
            boundary.condition_false_exit().clone(),
            boundary.break_boundary().clone()
        ]
    );
    assert_ne!(
        consumer.incoming()[0].owner(),
        consumer.incoming()[1].owner()
    );
    assert_eq!(consumer.actions().len(), 3);
    assert_eq!(raw.owner_observations, shared.owner_observations());
    assert!(!raw.loan_action_observations.is_empty());
    let actions: Vec<_> = raw
        .loan_action_observations
        .iter()
        .flat_map(|o| o.actions())
        .collect();
    assert!(actions.iter().any(|a| matches!(a, Action::Begin(_))));
    assert!(actions.iter().any(|a| matches!(a, Action::ReceiverEnd(_))));
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
    assert!(!raw.direct_call_loan_ends.is_empty());
    assert!(!raw.receiver_call_return_ends.is_empty());
    assert!(!raw.borrow_call_handoffs.is_empty());
    assert!(!raw.borrow_results.bindings.is_empty());
    assert!(!raw.borrow_results.ends.is_empty());
    assert_eq!(
        raw.loan_action_observations,
        shared.loan_action_observations()
    );

    assert_consumer_dependencies(&raw.loan_action_observations, &raw.borrow_call_transports);
    assert_consumer_dependencies(
        shared.loan_action_observations(),
        shared.borrow_call_transports(),
    );

    assert_complete_consumer_region(&accepted, 1, "region_owner", &raw.loan_action_observations);
    assert_complete_consumer_region(
        &accepted,
        1,
        "region_owner",
        shared.loan_action_observations(),
    );

    // Both positive sources remain unchanged and precede an independent source.
    let rejected = Fixture::new(&[("a.ko", B), ("b.ko", LOAN), ("z.ko", suffix)]);
    assert_eq!(
        rejected
            .names
            .names()
            .index()
            .source_units()
            .iter()
            .map(|s| s.id())
            .collect::<Vec<_>>(),
        [rejected.source(0), rejected.source(1), rejected.source(2)]
    );
    let shared = rejected.shared();
    let raw = rejected.dataflow(&rejected.parts(&shared));
    if deferred {
        assert!(raw.diagnostics.is_empty());
        assert_eq!(
            raw.deferred,
            [UnitOwnershipDeferredFact::new(
                UnitExpressionId::new(
                    rejected.source(2),
                    rejected.expression(2, "replace(&callback, { 2 })")
                ),
                OwnershipDeferredReason::OwnershipPrimitiveClosureTransport,
            )]
        );
        // Existing deferred publication preserves the old diagnostic projection,
        // but must revoke these three capabilities. Do not invent all-empty semantics.
        assert!(!raw.owner_observations.is_empty());
        assert!(!shared.owner_observations().is_empty());
        for observation in raw
            .owner_observations
            .iter()
            .chain(shared.owner_observations())
        {
            assert!(observation.replace_break_observation_v1().is_none());
            assert!(observation.loop_observation_v1().is_none());
            assert!(observation.invariant_loop_observation_v1().is_none());
        }
        assert_eq!(raw.owner_observations, shared.owner_observations());
    } else {
        assert!(raw.deferred.is_empty());
        let target = rejected.expression(2, "target");
        let span = rejected.parsed[2]
            .ast()
            .expressions()
            .get(target)
            .unwrap()
            .span();
        let symbol = rejected.symbol(2, "target");
        let declaration = rejected.names.names().source_units()[symbol.source_unit().index()]
            .resolution()
            .symbols()[symbol.symbol().index()]
        .span();
        let mut expected = Diagnostic::new(
            &rejected.sources,
            Severity::Error,
            codes::catalog()
                .unwrap()
                .resolve(codes::IMMUTABLE_INOUT_PLACE)
                .unwrap(),
            "assignment target is not a mutable place",
            span,
        )
        .unwrap();
        expected
            .add_label(
                &rejected.sources,
                declaration,
                "immutable binding declared here",
            )
            .unwrap();
        assert_eq!(raw.diagnostics, [expected]);
        assert!(
            raw.owner_observations.is_empty(),
            "raw owner publication must not retain an accepted B after global failure"
        );
        assert!(shared.owner_observations().is_empty());
    }
    assert_eq!(raw.diagnostics, shared.diagnostics());
    assert_eq!(raw.deferred, shared.deferred());
    assert_cleared(&raw);
    assert_public_cleared(&shared);
}

#[test]
fn real_error_revokes_owner_and_loan_after_independent_positive_sources() {
    reject(DIAGNOSTIC_SUFFIX, false);
}

#[test]
fn real_deferred_revokes_owner_capabilities_and_loan_after_independent_positive_sources() {
    reject(DEFERRED_SUFFIX, true);
}
