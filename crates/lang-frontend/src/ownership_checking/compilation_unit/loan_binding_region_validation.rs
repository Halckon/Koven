//! Independent validation of the entire actually entered binding interval.
use super::{
    UnitBorrowBindingRegion, UnitLoanAction, UnitLoanActionCoverage, UnitLoanActionId, UnitLoanFact,
};
use crate::{
    name_resolution::UnitSymbolId,
    source::Span,
    type_checking::{UnitExpressionId, UnitItemId, UnitStatementId},
};
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct Scope {
    pub(super) callable: UnitItemId,
    pub(super) binding: UnitSymbolId,
    pub(super) root: UnitSymbolId,
    pub(super) source_call: UnitExpressionId,
    pub(super) consumer_call: UnitExpressionId,
    pub(super) entry: UnitStatementId,
    pub(super) completion: UnitStatementId,
    pub(super) consumer_statement: UnitStatementId,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct OwnerSnapshot {
    pub(super) owner: Option<super::UnitOwnerId>,
    pub(super) symbol: UnitSymbolId,
    pub(super) origin: Span,
    pub(super) declaration: Span,
    pub(super) scope_depth: usize,
}

#[derive(Clone)]
pub(super) struct Expected {
    pub(super) source: UnitLoanFact,
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
    ) -> UnitBorrowBindingRegion {
        let coverage = self.coverage(&identity, actions);
        UnitBorrowBindingRegion {
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

    fn coverage(&self, identity: &Arc<()>, all: &[UnitLoanAction]) -> UnitLoanActionCoverage {
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
            && source == &expected.source
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
            && handoff.auxiliary_loan_ends().is_empty()
            && returned.binding() == scope.binding
            && returned.loan() == consumer
            && id_at(returned.source_begin(), 0)
            && id_at(returned.handoff(), 1)
            && id_at(returned.begin(), 2)
            && binding_end.binding() == scope.binding
            && binding_end.point() == end_point
            && retired.transaction().binding() == scope.binding
            && retired.transaction().source_loan() == source
            && retired.transaction().point() == end_point
            && id_at(retired.source_begin(), 0)
            && id_at(retired.handoff(), 1)
            && id_at(retired.use_return(), 3)
            && id_at(retired.binding_end(), 4)
            && self.drop_link.as_ref().is_some_and(|(retirement, drop)| {
                id_at(Some(retirement.clone()), 5) && id_at(Some(drop.clone()), 6)
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
