//! Independent eight-record auxiliary-bearing binding interval validation.
use super::{
    UnitBorrowAuxiliaryBindingRegion, UnitLoanAction, UnitLoanActionCoverage, UnitLoanActionId,
    UnitLoanAuxiliaryRetirement, UnitLoanFact,
};
use std::sync::Arc;

use super::loan_binding_region_validation::{OwnerSnapshot, Scope};

#[derive(Clone)]
pub(super) struct Expected {
    pub(super) source: UnitLoanFact,
    pub(super) auxiliary: UnitLoanFact,
    pub(super) consumer: UnitLoanFact,
    pub(super) owner: OwnerSnapshot,
}

#[derive(Clone)]
pub(super) struct Candidate {
    pub(super) scope: Scope,
    pub(super) expected: Option<Expected>,
    pub(super) start: usize,
    pub(super) end: Option<usize>,
    pub(super) exit_empty: bool,
    pub(super) drop_link: Option<(UnitLoanActionId, UnitLoanActionId)>,
    pub(super) ambiguous: bool,
    pub(super) removed_owner: Option<OwnerSnapshot>,
}

impl Candidate {
    pub(super) fn finish(
        self,
        identity: Arc<()>,
        actions: &[UnitLoanAction],
        auxiliaries: &[UnitLoanAuxiliaryRetirement],
    ) -> UnitBorrowAuxiliaryBindingRegion {
        let coverage = self.coverage(&identity, actions, auxiliaries);
        UnitBorrowAuxiliaryBindingRegion {
            identity,
            callable: self.scope.callable,
            binding: self.scope.binding,
            root: self.scope.root,
            source_call: self.scope.source_call,
            consumer_call: self.scope.consumer_call,
            entry_statement: self.scope.entry,
            completion_statement: self.scope.completion,
            coverage,
            records: self.start..self.end.unwrap_or(self.start),
        }
    }

    fn coverage(
        &self,
        identity: &Arc<()>,
        all: &[UnitLoanAction],
        auxiliaries: &[UnitLoanAuxiliaryRetirement],
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
            UnitLoanAction::Begin(source),
            UnitLoanAction::Begin(auxiliary),
            UnitLoanAction::BorrowHandoffCommitted(handoff),
            UnitLoanAction::Begin(consumer),
            UnitLoanAction::BorrowBindingUseReturned(returned),
            UnitLoanAction::BorrowBindingEnd(binding_end),
            UnitLoanAction::BorrowBindingSourceRetired(retired),
            UnitLoanAction::Cleanup {
                point,
                action: UnitIterationCleanupAction::Drop(drop),
            },
        ] = actions
        else {
            return UnitLoanActionCoverage::Partial;
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
        let valid = self.exit_empty
            && !self.ambiguous
            && self.removed_owner.as_ref() == Some(&expected.owner)
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
            && expected.owner.owner.is_some()
            && expected.owner.symbol == scope.root
            && expected.owner.scope_depth == 1
            && source == &expected.source
            && auxiliary == &expected.auxiliary
            && auxiliary.call() == source.call()
            && auxiliary.argument() != source.argument()
            && auxiliary.argument().source_unit() == local
            && auxiliary.kind() == LoanKind::Shared
            && auxiliary.target() == source.target()
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
                    id_at(Some(relation.source_begin()), 0)
                        && id_at(Some(relation.auxiliary_begin()), 1)
                        && id_at(Some(relation.handoff()), 2)
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
                    .any(|id| (0..3).any(|offset| id_at(Some(id.clone()), offset)))
                })
                .count()
                == 1
            && returned.binding() == scope.binding
            && returned.loan() == consumer
            && id_at(returned.source_begin(), 0)
            && id_at(returned.handoff(), 2)
            && id_at(returned.begin(), 3)
            && binding_end.binding() == scope.binding
            && binding_end.point() == end_point
            && retired.transaction().binding() == scope.binding
            && retired.transaction().source_loan() == source
            && retired.transaction().point() == end_point
            && id_at(retired.source_begin(), 0)
            && id_at(retired.handoff(), 2)
            && id_at(retired.use_return(), 4)
            && id_at(retired.binding_end(), 5)
            && self.drop_link.as_ref().is_some_and(|(retirement, drop)| {
                id_at(Some(retirement.clone()), 6) && id_at(Some(drop.clone()), 7)
            })
            && *point == end_point
            && drop.point() == end_point
            && drop.target() == UnitDropTarget::Named(scope.root)
            && drop.value_origin() == expected.owner.origin;
        if valid {
            UnitLoanActionCoverage::Complete
        } else {
            UnitLoanActionCoverage::Partial
        }
    }
}

#[cfg(test)]
mod tests;
