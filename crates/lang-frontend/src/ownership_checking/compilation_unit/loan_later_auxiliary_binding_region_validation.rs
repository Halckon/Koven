//! Independent eleven-record two-root later-use interval with an actual return sidecar.
use super::{
    UnitBorrowLaterAuxiliaryBindingRegion, UnitLoanAction, UnitLoanActionCoverage,
    UnitLoanActionId, UnitLoanDistinctAuxiliaryRetirement, UnitLoanFact,
};
use std::sync::Arc;

use super::loan_binding_region_validation::{OwnerSnapshot, Scope};

#[derive(Clone)]
pub(super) struct Expected {
    /// Verified positional parameter and chronological Begin offset, strictly 0 or 1.
    pub(super) later_consumer: UnitLoanFact,
    pub(super) later_statement: crate::type_checking::UnitStatementId,
    pub(super) source_index: usize,
    pub(super) source: UnitLoanFact,
    pub(super) auxiliary: UnitLoanFact,
    pub(super) consumer: UnitLoanFact,
    pub(super) source_owner: OwnerSnapshot,
    pub(super) auxiliary_owner: OwnerSnapshot,
}

#[derive(Clone)]
pub(super) struct LaterReturn {
    pub(super) loan: UnitLoanFact,
    pub(super) begin: UnitLoanActionId,
    pub(super) cursor: usize,
    pub(super) owner: OwnerSnapshot,
}

#[derive(Clone)]
pub(super) struct Candidate {
    pub(super) later_return: Option<LaterReturn>,
    pub(super) removed_auxiliary_owner: Option<OwnerSnapshot>,
    pub(super) auxiliary_drop_link: Option<(UnitLoanActionId, UnitLoanActionId)>,
    pub(super) return_drop_claimed: bool,
    pub(super) scope: Scope,
    pub(super) expected: Option<Expected>,
    pub(super) start: usize,
    pub(super) end: Option<usize>,
    pub(super) exit_empty: bool,
    pub(super) drop_link: Option<(UnitLoanActionId, UnitLoanActionId)>,
    pub(super) ambiguous: bool,
    pub(super) removed_source_owner: Option<OwnerSnapshot>,
    pub(super) owner_events: Vec<super::UnitOwnerTransportEvent>,
}

impl Candidate {
    pub(super) fn finish(
        self,
        identity: Arc<()>,
        actions: &[UnitLoanAction],
        auxiliaries: &[UnitLoanDistinctAuxiliaryRetirement],
    ) -> UnitBorrowLaterAuxiliaryBindingRegion {
        let coverage = self.coverage(&identity, actions, auxiliaries);
        UnitBorrowLaterAuxiliaryBindingRegion {
            identity,
            callable: self.scope.callable,
            binding: self.scope.binding,
            root: self.scope.root,
            source_call: self.scope.source_call,
            consumer_call: self.scope.consumer_call,
            entry_statement: self.scope.entry,
            completion_statement: self.scope.completion,
            coverage,
            auxiliary_root: self.expected.as_ref().map(|e| e.auxiliary_owner.symbol),
            source_parameter: self.expected.as_ref().map(|e| e.source_index),
            source_owner: self
                .expected
                .as_ref()
                .and_then(|e| e.source_owner.owner.clone()),
            auxiliary_owner: self
                .expected
                .as_ref()
                .and_then(|e| e.auxiliary_owner.owner.clone()),
            later_consumer_call: self.expected.as_ref().map(|e| e.later_consumer.call()),
            later_statement: self.expected.as_ref().map(|e| e.later_statement),
            later_begin: self.later_return.as_ref().map(|r| r.begin.clone()),
            later_return_cursor: self.later_return.as_ref().map(|r| r.cursor),
            records: self.start..self.end.unwrap_or(self.start),
        }
    }

    fn coverage(
        &self,
        identity: &Arc<()>,
        all: &[UnitLoanAction],
        auxiliaries: &[UnitLoanDistinctAuxiliaryRetirement],
    ) -> UnitLoanActionCoverage {
        use super::{UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction, UnitLoanTarget};
        use crate::ownership_checking::LoanKind;
        let Some(expected) = &self.expected else {
            return UnitLoanActionCoverage::Unsupported;
        };
        let Some(actions) = self.end.and_then(|end| all.get(self.start..end)) else {
            return UnitLoanActionCoverage::Partial;
        };
        let [
            UnitLoanAction::Begin(first),
            UnitLoanAction::Begin(second),
            UnitLoanAction::BorrowHandoffCommitted(handoff),
            UnitLoanAction::Begin(consumer),
            UnitLoanAction::BorrowBindingUseReturned(returned),
            UnitLoanAction::BorrowBindingEnd(binding_end),
            UnitLoanAction::BorrowBindingSourceRetired(retired),
            UnitLoanAction::Cleanup {
                point,
                action: UnitIterationCleanupAction::Drop(drop),
            },
            UnitLoanAction::Begin(later_consumer),
            UnitLoanAction::Cleanup {
                point: later_end_point,
                action: UnitIterationCleanupAction::EndCallLoan(later_end),
            },
            UnitLoanAction::Cleanup {
                point: auxiliary_point,
                action: UnitIterationCleanupAction::Drop(auxiliary_drop),
            },
        ] = actions
        else {
            return UnitLoanActionCoverage::Partial;
        };
        let (source, auxiliary, source_offset, auxiliary_offset) = match expected.source_index {
            0 => (first, second, 0, 1),
            1 => (second, first, 1, 0),
            _ => return UnitLoanActionCoverage::Partial,
        };
        let id_at = |id: Option<UnitLoanActionId>, offset| {
            id.is_some_and(|id| {
                Arc::ptr_eq(&id.identity, identity)
                    && self.start.checked_add(offset) == Some(id.index)
            })
        };
        let scope = &self.scope;
        let local = scope.callable.source_unit();
        let end_point = UnitDropPoint::AfterStatement(scope.consumer_statement);
        let Some(source_owner) = expected.source_owner.owner.as_ref() else {
            return UnitLoanActionCoverage::Partial;
        };
        let Some(auxiliary_owner) = expected.auxiliary_owner.owner.as_ref() else {
            return UnitLoanActionCoverage::Partial;
        };
        let call_return = UnitDropPoint::CallReturn(expected.later_consumer.call());
        let exact_owner_events = [
            super::UnitOwnerTransportEvent::Drop {
                point: end_point,
                symbol: scope.root,
                owner: source_owner.clone(),
            },
            super::UnitOwnerTransportEvent::Drop {
                point: call_return,
                symbol: expected.auxiliary_owner.symbol,
                owner: auxiliary_owner.clone(),
            },
        ];
        let Some(later_return) = self.later_return.as_ref() else {
            return UnitLoanActionCoverage::Partial;
        };
        let valid = self.exit_empty
            && later_consumer == &expected.later_consumer
            && later_consumer.kind() == LoanKind::Shared
            && later_consumer.target() == auxiliary.target()
            && later_consumer.call().source_unit() == local
            && later_consumer.argument().source_unit() == local
            && later_consumer.call() != scope.source_call
            && later_consumer.call() != scope.consumer_call
            && expected.later_statement.source_unit() == local
            && expected.later_statement != scope.entry
            && expected.later_statement != scope.completion
            && expected.later_statement != scope.consumer_statement
            && later_end == later_consumer
            && *later_end_point == call_return
            && later_return.loan == *later_consumer
            && id_at(Some(later_return.begin.clone()), 8)
            && self.start.checked_add(9) == Some(later_return.cursor)
            && later_return.owner == expected.auxiliary_owner
            && self.return_drop_claimed
            && self.removed_auxiliary_owner.as_ref() == Some(&expected.auxiliary_owner)
            && self
                .auxiliary_drop_link
                .as_ref()
                .is_some_and(|(begin, drop)| {
                    id_at(Some(begin.clone()), 8) && id_at(Some(drop.clone()), 10)
                })
            && self.owner_events == exact_owner_events
            && source_owner != auxiliary_owner
            && Arc::ptr_eq(&source_owner.identity, &auxiliary_owner.identity)
            && source_owner.callable == super::UnitOwnerCallable::Function(scope.callable)
            && auxiliary_owner.callable == source_owner.callable
            && expected.auxiliary_owner.symbol != scope.root
            && expected.auxiliary_owner.symbol != scope.binding
            && expected.auxiliary_owner.symbol.source_unit() == local
            && expected.auxiliary_owner.scope_depth == 1
            && expected.auxiliary_owner.origin.source_id()
                == expected.source_owner.origin.source_id()
            && expected.auxiliary_owner.declaration.source_id()
                == expected.source_owner.declaration.source_id()
            && *auxiliary_point == call_return
            && auxiliary_drop.point() == call_return
            && auxiliary_drop.target() == UnitDropTarget::Named(expected.auxiliary_owner.symbol)
            && auxiliary_drop.value_origin() == expected.auxiliary_owner.origin
            && !self.ambiguous
            && self.removed_source_owner.as_ref() == Some(&expected.source_owner)
            && scope.binding != scope.root
            && scope.binding.source_unit() == local
            && scope.root.source_unit() == local
            && scope.source_call.source_unit() == local
            && scope.consumer_call.source_unit() == local
            && scope.entry.source_unit() == local
            && scope.completion.source_unit() == local
            && scope.consumer_statement.source_unit() == local
            && scope.source_call != scope.consumer_call
            && scope.entry != scope.consumer_statement
            && scope.completion != scope.entry
            && scope.completion != scope.consumer_statement
            && expected.source_owner.owner.is_some()
            && expected.source_owner.symbol == scope.root
            && expected.source_owner.scope_depth == 1
            && source == &expected.source
            && auxiliary == &expected.auxiliary
            && auxiliary.call() == source.call()
            && auxiliary.argument() != source.argument()
            && auxiliary.argument().source_unit() == local
            && auxiliary.kind() == LoanKind::Shared
            && auxiliary.target() != source.target()
            && matches!(auxiliary.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == expected.auxiliary_owner.symbol)
            && consumer == &expected.consumer
            && source.call() == scope.source_call
            && consumer.call() == scope.consumer_call
            && source.kind() == LoanKind::Shared
            && consumer.kind() == LoanKind::Shared
            && source.argument().source_unit() == local
            && consumer.argument().source_unit() == local
            && matches!(source.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == scope.root)
            && consumer.target() == source.target()
            && handoff.binding() == scope.binding
            && handoff.source_loan() == source
            && handoff.auxiliary_loan_ends() == std::slice::from_ref(auxiliary)
            && auxiliaries
                .iter()
                .filter(|relation| {
                    id_at(Some(relation.source_begin()), source_offset)
                        && id_at(Some(relation.auxiliary_begin()), auxiliary_offset)
                        && id_at(Some(relation.handoff()), 2)
                        && relation.auxiliary_drop().is_none()
                        && relation.source_parameter() == expected.source_index
                        && Some(relation.source_owner()) == expected.source_owner.owner.as_ref()
                        && Some(relation.auxiliary_owner())
                            == expected.auxiliary_owner.owner.as_ref()
                        && relation.point() == UnitDropPoint::CallReturn(scope.source_call)
                })
                .count()
                == 1
            && auxiliaries
                .iter()
                .filter(|relation| {
                    [
                        relation.source_begin(),
                        relation.auxiliary_begin(),
                        relation.handoff(),
                    ]
                    .into_iter()
                    .chain(relation.auxiliary_drop())
                    .any(|id| (0..3).any(|offset| id_at(Some(id.clone()), offset)))
                })
                .count()
                == 1
            && returned.binding() == scope.binding
            && returned.loan() == consumer
            && id_at(returned.source_begin(), source_offset)
            && id_at(returned.handoff(), 2)
            && id_at(returned.begin(), 3)
            && binding_end.binding() == scope.binding
            && binding_end.point() == end_point
            && retired.transaction().binding() == scope.binding
            && retired.transaction().source_loan() == source
            && retired.transaction().point() == end_point
            && id_at(retired.source_begin(), source_offset)
            && id_at(retired.handoff(), 2)
            && id_at(retired.use_return(), 4)
            && id_at(retired.binding_end(), 5)
            && self.drop_link.as_ref().is_some_and(|(retirement, drop)| {
                id_at(Some(retirement.clone()), 6) && id_at(Some(drop.clone()), 7)
            })
            && *point == end_point
            && drop.point() == end_point
            && drop.target() == UnitDropTarget::Named(scope.root)
            && drop.value_origin() == expected.source_owner.origin;
        if valid {
            UnitLoanActionCoverage::Complete
        } else {
            UnitLoanActionCoverage::Partial
        }
    }
}

#[cfg(test)]
mod tests;
