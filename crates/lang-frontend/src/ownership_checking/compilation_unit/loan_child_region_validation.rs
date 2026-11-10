//! Independent full-interval validation for one direct parent/child dependency.
use super::{
    UnitBorrowChildRegion, UnitLoanAction, UnitLoanActionCoverage, UnitLoanActionId,
    UnitLoanBindingDependencyDrop, UnitLoanFact, loan_binding_region_validation::OwnerSnapshot,
};
use crate::{
    name_resolution::UnitSymbolId,
    type_checking::{UnitExpressionId, UnitItemId, UnitStatementId},
};
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct Scope {
    pub(super) callable: UnitItemId,
    pub(super) parent: UnitSymbolId,
    pub(super) child: UnitSymbolId,
    pub(super) root: UnitSymbolId,
    pub(super) child_initializer: UnitExpressionId,
    pub(super) source_call: UnitExpressionId,
    pub(super) consumer_call: UnitExpressionId,
    pub(super) entry: UnitStatementId,
    pub(super) completion: UnitStatementId,
    pub(super) consumer_statement: UnitStatementId,
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
    pub(super) ambiguous: bool,
}

impl Candidate {
    pub(super) fn finish(
        self,
        identity: Arc<()>,
        actions: &[UnitLoanAction],
        dependency: Option<&UnitLoanBindingDependencyDrop>,
        removed: Option<OwnerSnapshot>,
    ) -> UnitBorrowChildRegion {
        let coverage = self.coverage(&identity, actions, dependency, removed);
        UnitBorrowChildRegion {
            identity,
            callable: self.scope.callable,
            parent: self.scope.parent,
            child: self.scope.child,
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
        dependency: Option<&UnitLoanBindingDependencyDrop>,
        removed: Option<OwnerSnapshot>,
    ) -> UnitLoanActionCoverage {
        use super::{UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction, UnitLoanTarget};
        use crate::ownership_checking::LoanKind;
        let Some(expected) = &self.expected else {
            return UnitLoanActionCoverage::Unsupported;
        };
        let Some(actions) = self.end.and_then(|end| all.get(self.start..end)) else {
            return UnitLoanActionCoverage::Partial;
        };
        let Some(dependency) = dependency else {
            return UnitLoanActionCoverage::Partial;
        };
        let [
            UnitLoanAction::Begin(source),
            UnitLoanAction::BorrowHandoffCommitted(handoff),
            UnitLoanAction::BorrowBindingChildCommitted(commit),
            UnitLoanAction::Begin(consumer),
            UnitLoanAction::BorrowBindingChildUseReturned(returned),
            UnitLoanAction::BorrowBindingEnd(child_end),
            UnitLoanAction::BorrowBindingChildRetired(child_retired),
            UnitLoanAction::BorrowBindingEnd(parent_end),
            UnitLoanAction::BorrowBindingSourceRetired(parent_retired),
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
            && expected.owner.owner.as_ref().is_some_and(|id| {
                id.callable == super::UnitOwnerCallable::Function(scope.callable)
            })
            && removed.as_ref() == Some(&expected.owner)
            && scope.parent != scope.child
            && scope.parent != scope.root
            && scope.child != scope.root
            && [scope.parent, scope.child, scope.root]
                .iter()
                .all(|symbol| symbol.source_unit() == local)
            && [
                scope.source_call,
                scope.consumer_call,
                scope.child_initializer,
            ]
            .iter()
            .all(|expr| expr.source_unit() == local)
            && [scope.entry, scope.completion, scope.consumer_statement]
                .iter()
                .all(|stmt| stmt.source_unit() == local)
            && scope.source_call != scope.consumer_call
            && scope.child_initializer != scope.source_call
            && scope.child_initializer != scope.consumer_call
            && scope.entry != scope.consumer_statement
            && scope.completion != scope.entry
            && scope.completion != scope.consumer_statement
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
            && handoff.binding() == scope.parent
            && handoff.source_loan() == source
            && handoff.auxiliary_loan_ends().is_empty()
            && commit.binding() == scope.child
            && commit.parent() == scope.parent
            && commit.initializer() == scope.child_initializer
            && id_at(commit.source_begin(), 0)
            && id_at(commit.parent_handoff(), 1)
            && returned.binding() == scope.child
            && returned.parent() == scope.parent
            && returned.loan() == consumer
            && id_at(returned.source_begin(), 0)
            && id_at(returned.parent_handoff(), 1)
            && id_at(returned.commit(), 2)
            && id_at(returned.begin(), 3)
            && child_end.binding() == scope.child
            && child_end.point() == end_point
            && child_retired.binding() == scope.child
            && child_retired.parent() == scope.parent
            && child_retired.point() == end_point
            && id_at(child_retired.source_begin(), 0)
            && id_at(child_retired.parent_handoff(), 1)
            && id_at(child_retired.commit(), 2)
            && id_at(child_retired.use_return(), 4)
            && id_at(child_retired.binding_end(), 5)
            && parent_end.binding() == scope.parent
            && parent_end.point() == end_point
            && parent_retired.transaction().binding() == scope.parent
            && parent_retired.transaction().source_loan() == source
            && parent_retired.transaction().point() == end_point
            && id_at(parent_retired.source_begin(), 0)
            && id_at(parent_retired.handoff(), 1)
            && id_at(parent_retired.binding_end(), 7)
            && parent_retired.use_return().is_none()
            && dependency.root() == scope.root
            && dependency.parent() == scope.parent
            && dependency.child() == scope.child
            && [
                (dependency.source_begin(), 0),
                (dependency.parent_handoff(), 1),
                (dependency.child_commit(), 2),
                (dependency.child_use_return(), 4),
                (dependency.child_retirement(), 6),
                (dependency.parent_retirement(), 8),
                (dependency.cleanup(), 9),
            ]
            .into_iter()
            .all(|(id, index)| id_at(Some(id), index))
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
