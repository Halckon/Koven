//! Constructed merge states are CI-only; ordinary frontend neighbors live in codegen tests.
use super::super::{
    action_source::{ActionHandle, ActionSource},
    model::merge_value_states,
};
use super::*;
use crate::ownership_checking::compilation_unit::formed_tests::Fixture;

#[test]
fn ci_only_forward_merge_preserves_old_equality_and_only_loses_cause() {
    let fixture = Fixture::new(&[(
        "a.ko",
        "fun view(source: String): borrow String from source = source\nfun wrap(source: String): borrow String from source = view(source)",
    )]);
    let owned = fixture.shared();
    let fact = &owned.borrow_forward_transports()[0];
    let source = fact.source_loan();
    let wrap = fixture
        .names
        .names()
        .index()
        .declarations()
        .iter()
        .find(|d| d.name() == "wrap")
        .unwrap();
    let UnitLoanTarget::Place(place) = source.target() else {
        panic!("parameter")
    };
    let scope = ForwardScope {
        region: None,
        callable: UnitItemId::new(wrap.source_unit(), wrap.root()),
        body: source.call(),
        control: source.call(),
        branch: None,
        branch_entry: None,
        argument: source.argument(),
        root: place.root(),
    };
    let mut actions = ActionSource::default();
    let first = actions.begin(source.clone()).unwrap();
    let different = actions.begin(source.clone()).unwrap();
    let foreign = ActionSource::default().begin(source.clone()).unwrap();
    let pending = |handle: Option<ActionHandle>| PendingForwardTransport {
        bounded_cause: None,
        single_cause: None,
        multi_cause: None,
        nested_cause: None,
        returned: fact.returned_expression(),
        source: source.clone(),
        cause: handle.map(|acquisition| ForwardCause { acquisition, scope }),
    };
    let state = |scope, transports| ValueState {
        forward_scope: scope,
        borrow_forward_transports: transports,
        ..ValueState::default()
    };
    let same = merge_value_states(vec![
        state(Some(scope), vec![pending(Some(first.clone()))]),
        state(Some(scope), vec![pending(Some(first.clone()))]),
    ]);
    assert_eq!(same.borrow_forward_transports.len(), 1);
    assert!(
        same.borrow_forward_transports[0]
            .cause
            .as_ref()
            .unwrap()
            .acquisition
            .same_record(&first)
    );
    let mut wrong_scope = scope;
    wrong_scope.body = scope.argument;
    for alternative in [
        state(Some(scope), vec![pending(Some(different))]),
        state(Some(scope), vec![pending(Some(foreign))]),
        state(Some(scope), vec![pending(None)]),
        state(None, vec![pending(Some(first.clone()))]),
        state(Some(wrong_scope), vec![pending(Some(first.clone()))]),
        state(
            Some(scope),
            vec![pending(Some(first.clone())), pending(Some(first.clone()))],
        ),
    ] {
        assert_eq!(
            pending(Some(first.clone())),
            alternative.borrow_forward_transports[0]
        );
        let merged = merge_value_states(vec![
            alternative,
            state(Some(scope), vec![pending(Some(first.clone()))]),
        ]);
        assert_eq!(merged.borrow_forward_transports.len(), 1);
        assert!(merged.borrow_forward_transports[0].cause.is_none());
        let again = merge_value_states(vec![
            state(Some(scope), vec![pending(Some(first.clone()))]),
            merged,
        ]);
        assert_eq!(again.borrow_forward_transports.len(), 1);
        assert!(
            again.borrow_forward_transports[0].cause.is_none(),
            "lost evidence must never refill"
        );
    }
    let absent = merge_value_states(vec![
        state(Some(scope), vec![]),
        state(Some(scope), vec![pending(Some(first))]),
    ]);
    assert!(
        absent.borrow_forward_transports.is_empty(),
        "old retain qualification remains unchanged"
    );
}
