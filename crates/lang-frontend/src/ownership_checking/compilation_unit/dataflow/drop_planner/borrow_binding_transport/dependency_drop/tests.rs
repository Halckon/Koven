//! Constructed identity/merge/batch loss is CI-only. Do not run locally.
use super::super::{PendingBindingTransport, UseReturn, child::ChildState};
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;

#[test]
fn ci_only_dependency_batch_and_retired_identity_loss_are_sticky() {
    let fixture = Fixture::new(&[(
        "a.ko",
        r#"fun view(source: String): borrow String from source = source
fun consume(item: String): Unit {}
fun run(): Unit { val owner = "kept"; borrow val parent = view(owner); borrow val child = parent; consume(child) }"#,
    )]);
    let shared = fixture.shared();
    let tx = &shared.borrow_binding_transports()[0];
    let mut actions = ActionSource::default();
    let handle = actions.begin(tx.source_loan().clone()).unwrap();
    let different = actions.begin(tx.source_loan().clone()).unwrap();
    let mut retired = ChildState::Retired {
        binding: tx.binding(),
        retirement: handle.clone(),
    };
    retired.reconcile(&ChildState::Retired {
        binding: tx.binding(),
        retirement: handle.clone(),
    });
    assert!(matches!(retired, ChildState::Retired { .. }));
    retired.reconcile(&ChildState::Retired {
        binding: tx.binding(),
        retirement: different.clone(),
    });
    retired.reconcile(&ChildState::Retired {
        binding: tx.binding(),
        retirement: handle.clone(),
    });
    assert!(matches!(retired, ChildState::Lost));
    let mut batch = Batch::first(Some(handle.clone()), None);
    batch.observe(None, None, &actions);
    batch.observe(None, Some(different), &actions);
    assert!(batch.cause().is_none());
    let mut first = PendingBindingTransport {
        binding: tx.binding(),
        source: tx.source_loan().clone(),
        source_begin: Some(handle.clone()),
        handoff: Some(handle),
        use_return: UseReturn::Unused,
        observable: true,
        child: ChildState::Never,
        dependency_owner: None,
        auxiliary: None,
        auxiliary_seen: false,
    };
    let mut other = first.clone();
    let drop = shared.drops()[0];
    let crate::ownership_checking::UnitDropTarget::Named(root) = drop.target() else {
        unreachable!()
    };
    other.dependency_owner = Some(OwnedValue {
        owner: None,
        symbol: root,
        origin: drop.value_origin(),
        declaration: drop.value_origin(),
        scope_depth: 1,
    });
    assert_eq!(
        first, other,
        "owner evidence is excluded from semantic equality"
    );
    first.reconcile(std::slice::from_ref(&other));
    first.reconcile(std::slice::from_ref(&other));
    assert!(
        first.dependency_owner.is_none(),
        "lost capture cannot be supplied by a later branch"
    );
}
