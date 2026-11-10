//! Genuine multi-source rejection after both owner B and loan production.
use super::*;
use crate::ownership_checking::{CompilationUnitOwnership, UnitLoanAction as Action};

// Byte-for-byte B from owner_transport/tests/loop_replace_break.rs.
const B: &str = r#"fun run(flag: Boolean) { var held = "outer"; while(flag) { val previous = replace(&held, "next"); break }; val retired = held }"#;
const LOAN: &str = r#"fun inspect(value: String): Int = 1
class Node { fun touch(): Int = 1 }
fun view(source: String): borrow String from source = source
fun loans(own node: Node): Unit {
inspect("temporary")
val owner = "kept"
inspect(owner)
node.touch()
borrow val item = view(owner)
println(item)
}"#;

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
