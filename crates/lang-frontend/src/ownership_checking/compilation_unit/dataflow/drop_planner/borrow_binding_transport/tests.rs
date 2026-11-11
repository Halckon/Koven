//! Constructed metadata loss is CI-only. Do not execute locally.
use super::super::{action_source::ActionSource, model::merge_value_states};
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;

const SOURCE: &str = r#"fun view(source: String): borrow String from source = source
fun consume(item: String): Unit {}
fun run(): Unit { val owner = "kept"; borrow val item = view(owner); consume(item) }"#;

#[test]
fn ci_only_binding_metadata_preserves_semantic_eq_and_sticky_merge_loss() {
    let fixture = Fixture::new(&[("a.ko", SOURCE)]);
    let shared = fixture.shared();
    let transaction = &shared.borrow_binding_transports()[0];
    let mut actions = ActionSource::default();
    let source_begin = actions.begin(transaction.source_loan().clone());
    let handoff = actions.commit_borrow_handoff(shared.borrow_call_handoffs()[0].clone());
    let first = PendingBindingTransport {
        binding: transaction.binding(),
        source: transaction.source_loan().clone(),
        source_begin: source_begin.clone(),
        handoff: Some(handoff),
        use_return: UseReturn::Unique(source_begin.clone().unwrap()),
        observable: true,
        dependency_owner: None,
        auxiliary: None,
        auxiliary_seen: false,
        child: child::ChildState::Never,
    };
    let mut second = first.clone();
    second.use_return =
        UseReturn::Unique(actions.begin(transaction.source_loan().clone()).unwrap());
    assert_eq!(
        first, second,
        "metadata cannot change semantic transport equality"
    );
    let left = ValueState {
        borrow_binding_transports: vec![first.clone()],
        ..ValueState::default()
    };
    let right = ValueState {
        borrow_binding_transports: vec![second],
        ..ValueState::default()
    };
    let mut merged = merge_value_states(vec![left, right]);
    assert_eq!(merged.borrow_binding_transports.len(), 1);
    let carrier = &mut merged.borrow_binding_transports[0];
    assert_eq!(carrier, &first);
    assert!(matches!(carrier.use_return, UseReturn::Ambiguous));
    carrier.record_use(source_begin.clone());
    assert!(matches!(carrier.use_return, UseReturn::Ambiguous));
    carrier.reconcile(std::slice::from_ref(&first));
    carrier.record_use(source_begin);
    assert!(matches!(carrier.use_return, UseReturn::Ambiguous));
    let mut unused = first.clone();
    unused.use_return = UseReturn::Unused;
    unused.record_use(actions.begin(transaction.source_loan().clone()));
    assert!(matches!(unused.use_return, UseReturn::Unique(_)));
    unused.record_use(actions.begin(transaction.source_loan().clone()));
    assert!(matches!(unused.use_return, UseReturn::Ambiguous));
}
