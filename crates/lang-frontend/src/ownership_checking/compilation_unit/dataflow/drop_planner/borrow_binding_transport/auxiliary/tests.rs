//! CI-only auxiliary metadata corruption; compile locally, never run locally.
use super::super::{UseReturn, child};
use super::*;
use crate::ownership_checking::compilation_unit::dataflow::drop_planner::action_source::ActionSource;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;

const SOURCE: &str = "fun view(source: String, aux: String): borrow String from source = source\nfun consume(item: String): Unit {}\nfun run(): Unit { val owner = \"kept\"; borrow val item = view(owner, owner); consume(item) }";

fn activate(pending: &mut PendingBindingTransport, auxiliary: &ActionHandle) {
    let binding = pending.binding;
    let source = pending.source.clone();
    let begin = pending.source_begin.clone();
    let handoff = pending.handoff.clone().unwrap();
    pending.activate_auxiliary_observation(
        binding,
        &source,
        begin.as_ref(),
        Some(auxiliary),
        &handoff,
    );
}

#[test]
fn ci_only_auxiliary_activation_rejects_foreign_order_duplicates_and_merge_revival() {
    let fixture = Fixture::new(&[("aux.ko", SOURCE)]);
    let owned = fixture.shared();
    let [fact] = owned.borrow_call_handoffs() else {
        panic!("actual handoff")
    };
    let mut actions = ActionSource::default();
    let source = actions.begin(fact.source_loan().clone()).unwrap();
    let auxiliary = actions
        .begin(fact.auxiliary_loan_ends()[0].clone())
        .unwrap();
    let handoff = actions.commit_borrow_handoff(fact.clone());
    let initial = PendingBindingTransport {
        binding: fact.binding(),
        source: fact.source_loan().clone(),
        source_begin: Some(source.clone()),
        handoff: Some(handoff.clone()),
        use_return: UseReturn::Unused,
        use_set: None,
        observable: false,
        child: child::ChildState::Never,
        dependency_owner: None,
        auxiliary: None,
        auxiliary_seen: false,
        distinct: None,
        distinct_seen: false,
    };
    let mut valid = initial.clone();
    activate(&mut valid, &auxiliary);
    assert!(valid.auxiliary.is_some());
    assert_eq!(valid, initial, "metadata must not change semantic equality");
    assert!(!valid.observable);
    assert!(matches!(valid.use_return, UseReturn::Unused));
    assert!(valid.dependency_owner.is_none());
    assert!(matches!(valid.child, child::ChildState::Never));
    activate(&mut valid, &auxiliary);
    assert!(
        valid.auxiliary.is_none(),
        "duplicate activation poisons new capability"
    );
    activate(&mut valid, &auxiliary);
    assert!(valid.auxiliary.is_none(), "duplicate poison remains sticky");

    let foreign = ActionSource::default()
        .begin(fact.auxiliary_loan_ends()[0].clone())
        .unwrap();
    let later = actions
        .begin(fact.auxiliary_loan_ends()[0].clone())
        .unwrap();
    for bad in [&foreign, &source, &handoff, &later] {
        let mut pending = initial.clone();
        activate(&mut pending, bad);
        assert!(pending.auxiliary.is_none());
        activate(&mut pending, &auxiliary);
        assert!(
            pending.auxiliary.is_none(),
            "failed activation cannot be retried as valid"
        );
    }
    let mut reversed = initial.clone();
    reversed.source_begin = actions.begin(fact.source_loan().clone());
    activate(&mut reversed, &auxiliary);
    assert!(
        reversed.auxiliary.is_none(),
        "source must precede actual handoff"
    );

    for already_active in [false, true] {
        let mut merged = initial.clone();
        if already_active {
            activate(&mut merged, &auxiliary);
        }
        let alternative = merged.clone();
        merged.reconcile(&[alternative]);
        assert!(merged.auxiliary.is_none());
        activate(&mut merged, &auxiliary);
        assert!(
            merged.auxiliary.is_none(),
            "merge invalidation is sticky before or after activation"
        );
        assert!(!merged.observable);
        assert!(merged.dependency_owner.is_none());
    }
    let mut legacy = initial;
    legacy.observable = true;
    activate(&mut legacy, &auxiliary);
    assert!(legacy.auxiliary.is_none());
    assert!(
        legacy.observable,
        "auxiliary rejection must not revoke old authority"
    );
    assert!(matches!(legacy.use_return, UseReturn::Unused));
}
