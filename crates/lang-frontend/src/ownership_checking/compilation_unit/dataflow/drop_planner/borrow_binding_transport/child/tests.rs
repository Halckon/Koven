//! Constructed metadata ambiguity is CI-only. Do not run locally.
use super::super::super::{action_source::ActionSource, model::merge_value_states};
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;

const SOURCE: &str = r#"fun view(source: String): borrow String from source = source
fun consume(item: String): Unit {}
fun run(): Unit { val owner = "kept"; borrow val parent = view(owner); borrow val child = parent; consume(child) }"#;

#[test]
fn ci_only_child_metadata_never_changes_semantic_eq_or_recovers_lost_commit() {
    let fixture = Fixture::new(&[("a.ko", SOURCE)]);
    let shared = fixture.shared();
    let transport = &shared.borrow_binding_transports()[0];
    let child = shared
        .borrow_results()
        .bindings()
        .iter()
        .find(|fact| fact.parent() == Some(transport.binding()))
        .unwrap()
        .binding();
    let mut actions = ActionSource::default();
    let source_begin = actions.begin(transport.source_loan().clone());
    let handoff = actions.commit_borrow_handoff(shared.borrow_call_handoffs()[0].clone());
    let mut first = PendingBindingTransport {
        binding: transport.binding(),
        source: transport.source_loan().clone(),
        source_begin: source_begin.clone(),
        handoff: Some(handoff),
        use_return: UseReturn::Unused,
        observable: true,
        child: ChildState::Never,
    };
    // State reconciliation compares record identity, not the role or equal descriptors.
    first.child.activate(child, source_begin.clone());
    let mut other = first.clone();
    other.child = ChildState::Never;
    other
        .child
        .activate(child, actions.begin(transport.source_loan().clone()));
    assert_eq!(
        first, other,
        "child observation metadata is not semantic state"
    );
    let mut merged = merge_value_states(vec![
        ValueState {
            borrow_binding_transports: vec![first.clone()],
            ..ValueState::default()
        },
        ValueState {
            borrow_binding_transports: vec![other],
            ..ValueState::default()
        },
    ]);
    let carrier = &mut merged.borrow_binding_transports[0];
    assert_eq!(carrier, &first);
    assert!(matches!(carrier.child, ChildState::Lost));
    carrier.child.activate(child, source_begin.clone());
    assert!(matches!(carrier.child, ChildState::Lost));
    carrier.reconcile(std::slice::from_ref(&first));
    assert!(matches!(carrier.child, ChildState::Lost));
    let mut missing_parent = first.clone();
    missing_parent.handoff = None;
    missing_parent.reconcile(std::slice::from_ref(&first));
    assert!(matches!(missing_parent.child, ChildState::Lost));
    let mut used = first.child.clone();
    let ChildState::Live { used: history, .. } = &mut used else {
        unreachable!()
    };
    *history = UseReturn::Ambiguous;
    used.reconcile(&first.child);
    assert!(matches!(
        used,
        ChildState::Live {
            used: UseReturn::Ambiguous,
            ..
        }
    ));
    let mut retired = ChildState::Retired;
    retired.activate(child, source_begin);
    assert!(matches!(retired, ChildState::Lost));
}
