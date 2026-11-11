//! Synthetic lifecycle evidence is CI-only. Compile locally; never execute locally.
use super::super::super::OwnedValue;
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitLoanAction, UnitOwnerCallable, UnitOwnerId,
    compilation_unit::formed_tests::Fixture,
};
use std::sync::Arc;

const SOURCE: &str = r#"fun view(source: String, aux: String): borrow String from source = source
fun consume(item: String): Unit {}
fun run(): Unit { val owner = "kept"; borrow val item = view(owner, owner); consume(item) }"#;

fn setup() -> (ActionSource, Scope, Expected, OwnedValue, ActionHandle) {
    let fixture = Fixture::new(&[("aux-lifecycle.ko", SOURCE)]);
    let shared = fixture.shared();
    let observed = &shared.loan_action_observations()[0];
    let region = &observed.auxiliary_binding_regions()[0];
    let [
        UnitLoanAction::Begin(source),
        UnitLoanAction::Begin(auxiliary),
        _,
        UnitLoanAction::Begin(consumer),
        _,
        _,
        _,
        UnitLoanAction::Cleanup {
            point: UnitDropPoint::AfterStatement(statement),
            action: crate::ownership_checking::UnitIterationCleanupAction::Drop(drop),
        },
    ] = observed.actions()
    else {
        unreachable!()
    };
    let scope = Scope {
        callable: region.callable(),
        binding: region.binding(),
        root: region.root(),
        source_call: region.source_call(),
        consumer_call: region.consumer_call(),
        entry: region.entry_statement(),
        completion: region.completion_statement(),
        consumer_statement: *statement,
    };
    let owner = OwnedValue {
        owner: Some(UnitOwnerId {
            identity: Arc::new(()),
            callable: UnitOwnerCallable::Function(scope.callable),
            index: 0,
        }),
        symbol: scope.root,
        origin: drop.value_origin(),
        declaration: drop.value_origin(),
        scope_depth: 1,
    };
    let expected = Expected {
        source: source.clone(),
        auxiliary: auxiliary.clone(),
        consumer: consumer.clone(),
        owner: OwnerSnapshot {
            owner: owner.owner.clone(),
            symbol: owner.symbol,
            origin: owner.origin,
            declaration: owner.declaration,
            scope_depth: owner.scope_depth,
        },
    };
    let mut actions = ActionSource::default();
    actions.enter_auxiliary_binding_region(scope.clone(), Some(expected.clone()));
    let handle = actions.begin(source.clone()).unwrap();
    (actions, scope, expected, owner, handle)
}

fn cause(actions: &ActionSource, scope: &Scope, handle: &ActionHandle) -> AuxiliaryRegionDropCause {
    let Some(BindingDropCause::AuxiliaryRegion(cause)) =
        actions.auxiliary_binding_drop_cause(scope.binding, handle.clone())
    else {
        unreachable!()
    };
    cause
}

#[test]
fn ci_only_auxiliary_region_rejects_duplicate_completion_and_false_exit() {
    let (mut actions, scope, _, _, _) = setup();
    actions.finish_auxiliary_binding_region_statement(scope.completion, true, true);
    assert!(actions.auxiliary_binding_regions[0].end.is_some());
    assert!(!actions.auxiliary_binding_regions[0].ambiguous);
    actions.finish_auxiliary_binding_region_statement(scope.completion, true, true);
    assert!(actions.auxiliary_binding_regions[0].ambiguous);
    let (mut actions, scope, _, _, _) = setup();
    actions.finish_auxiliary_binding_region_statement(scope.completion, false, true);
    assert!(actions.auxiliary_binding_regions[0].end.is_none());
    assert!(actions.active_auxiliary_binding_region.is_none());
}

#[test]
fn ci_only_auxiliary_region_rejects_missing_wrong_and_repeated_removed_owner() {
    let (mut actions, scope, _, owner, handle) = setup();
    let unverified = cause(&actions, &scope, &handle);
    actions.record_auxiliary_region_drop(unverified, handle.clone());
    assert!(actions.auxiliary_binding_regions[0].ambiguous);
    assert!(actions.auxiliary_binding_regions[0].drop_link.is_none());
    for fault in 0..4 {
        let (mut actions, scope, _, mut wrong, handle) = setup();
        match fault {
            0 => wrong.owner = None,
            1 => wrong.owner.as_mut().unwrap().identity = Arc::new(()),
            2 => wrong.symbol = scope.binding,
            _ => wrong.scope_depth += 1,
        }
        let candidate = cause(&actions, &scope, &handle);
        assert!(
            actions
                .verify_removed_auxiliary_region_owner(candidate, &wrong)
                .is_none()
        );
        assert!(actions.auxiliary_binding_regions[0].ambiguous);
    }
    let (mut actions, scope, _, _, handle) = setup();
    // This different run's identity cannot satisfy the freshly captured owner.
    let candidate = cause(&actions, &scope, &handle);
    assert!(
        actions
            .verify_removed_auxiliary_region_owner(candidate, &owner)
            .is_none()
    );
    let (mut actions, scope, _, owner, handle) = setup();
    let candidate = cause(&actions, &scope, &handle);
    let verified = actions
        .verify_removed_auxiliary_region_owner(candidate, &owner)
        .unwrap();
    assert!(
        actions
            .verify_removed_auxiliary_region_owner(verified, &owner)
            .is_none()
    );
    assert!(actions.auxiliary_binding_regions[0].ambiguous);
}

#[test]
fn ci_only_auxiliary_region_keeps_legacy_cause_and_root_conflicts_sticky() {
    let (mut actions, scope, expected, _, handle) = setup();
    actions.enter_binding_region(
        scope.clone(),
        Some(
            crate::ownership_checking::compilation_unit::loan_binding_region_validation::Expected {
                source: expected.source,
                consumer: expected.consumer,
                owner: expected.owner,
            },
        ),
    );
    let legacy = actions.binding_drop_cause(scope.binding, handle.clone());
    let auxiliary = actions.auxiliary_binding_drop_cause(scope.binding, handle);
    let kept = actions.preserve_existing_drop_cause(scope.root, legacy, auxiliary);
    assert!(matches!(kept, Some(BindingDropCause::Region(_))));
    assert!(actions.auxiliary_binding_regions[0].ambiguous);
    assert!(!actions.binding_regions[0].ambiguous);
    let (mut actions, scope, expected, owner, handle) = setup();
    actions.invalidate_auxiliary_root(scope.binding);
    assert!(!actions.auxiliary_binding_regions[0].ambiguous);
    actions.invalidate_auxiliary_root(scope.root);
    let candidate = cause(&actions, &scope, &handle);
    let verified = actions
        .verify_removed_auxiliary_region_owner(candidate, &owner)
        .unwrap();
    actions.record_auxiliary_region_drop(verified, handle);
    actions.finish_auxiliary_binding_region_statement(scope.completion, true, true);
    assert!(actions.auxiliary_binding_regions[0].ambiguous);
    let (mut actions, scope, _, _, _) = setup();
    let mut other = scope;
    other.root = other.binding;
    actions.enter_auxiliary_binding_region(other, Some(expected));
    assert_eq!(actions.auxiliary_binding_regions.len(), 1);
    assert!(actions.auxiliary_binding_regions[0].ambiguous);
}
