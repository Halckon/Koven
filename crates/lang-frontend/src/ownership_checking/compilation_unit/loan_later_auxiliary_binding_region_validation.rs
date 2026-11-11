//! Independent eleven-record two-root later-use interval with an actual return sidecar.
use super::{
    UnitBorrowLaterAuxiliaryBindingRegion, UnitLoanAction, UnitLoanActionCoverage,
    UnitLoanActionId, UnitLoanDistinctAuxiliaryRetirement, UnitLoanFact,
};
use std::sync::Arc;

use super::loan_binding_region_validation::{OwnerSnapshot, Scope};

pub(super) const MAX_AUXILIARY_SEQUENCE_USES: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Protocol {
    Single,
    Sequence,
    Unsupported,
}
impl Protocol {
    pub(super) fn from_expected(expected: Option<&Expected>) -> Self {
        Self::from_count(expected.map_or(0, |e| e.later_uses.len()))
    }
    pub(super) fn from_count(count: usize) -> Self {
        match count {
            1 => Self::Single,
            2..=MAX_AUXILIARY_SEQUENCE_USES => Self::Sequence,
            _ => Self::Unsupported,
        }
    }
}

#[derive(Clone)]
pub(super) struct Expected {
    /// Verified positional parameter and chronological Begin offset, strictly 0 or 1.
    pub(super) later_uses: Vec<(UnitLoanFact, crate::type_checking::UnitStatementId)>,
    pub(super) later_consumer: UnitLoanFact,
    pub(super) later_statement: crate::type_checking::UnitStatementId,
    pub(super) source_index: usize,
    pub(super) source: UnitLoanFact,
    pub(super) auxiliary: UnitLoanFact,
    pub(super) consumer: UnitLoanFact,
    pub(super) source_owner: OwnerSnapshot,
    pub(super) auxiliary_owner: OwnerSnapshot,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct LaterReturn {
    pub(super) loan: UnitLoanFact,
    pub(super) begin: UnitLoanActionId,
    pub(super) cursor: usize,
    pub(super) owner: OwnerSnapshot,
}

#[derive(Clone)]
pub(super) struct Candidate {
    pub(super) protocol: Protocol,
    pub(super) actual_returns: Vec<LaterReturn>,
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
        self.interval(identity, coverage)
    }

    pub(super) fn finish_sequence(
        self,
        identity: Arc<()>,
        actions: &[UnitLoanAction],
        auxiliaries: &[UnitLoanDistinctAuxiliaryRetirement],
    ) -> super::UnitBorrowAuxiliarySequenceRegion {
        #[cfg(test)]
        tests::record_sequence(&self, &identity, actions, auxiliaries);
        let coverage = self.coverage_for(Protocol::Sequence, &identity, actions, auxiliaries);
        let uses = if coverage == UnitLoanActionCoverage::Complete {
            self.actual_returns
                .iter()
                .filter_map(|returned| {
                    Some(super::UnitLoanAuxiliarySequenceUse {
                        loan: returned.loan.clone(),
                        begin: returned.begin.clone(),
                        end: UnitLoanActionId {
                            identity: identity.clone(),
                            index: returned.cursor,
                        },
                        owner: returned.owner.owner.clone()?,
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        super::UnitBorrowAuxiliarySequenceRegion {
            interval: self.interval(identity, coverage),
            uses,
        }
    }

    fn interval(
        self,
        identity: Arc<()>,
        coverage: UnitLoanActionCoverage,
    ) -> UnitBorrowLaterAuxiliaryBindingRegion {
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
        self.coverage_for(Protocol::Single, identity, all, auxiliaries)
    }

    fn coverage_for(
        &self,
        protocol: Protocol,
        identity: &Arc<()>,
        all: &[UnitLoanAction],
        auxiliaries: &[UnitLoanDistinctAuxiliaryRetirement],
    ) -> UnitLoanActionCoverage {
        use super::{UnitDropPoint, UnitDropTarget, UnitIterationCleanupAction, UnitLoanTarget};
        use crate::ownership_checking::LoanKind;
        if self.protocol != protocol || protocol == Protocol::Unsupported {
            return UnitLoanActionCoverage::Unsupported;
        }
        let Some(expected) = &self.expected else {
            return UnitLoanActionCoverage::Unsupported;
        };
        if Protocol::from_expected(Some(expected)) != self.protocol
            || expected.later_uses.last()
                != Some(&(expected.later_consumer.clone(), expected.later_statement))
            || self.actual_returns.len() != expected.later_uses.len()
            || self.actual_returns.last() != self.later_return.as_ref()
        {
            return UnitLoanActionCoverage::Partial;
        }
        let count = expected.later_uses.len();
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
        ] = actions.get(..8).unwrap_or_default()
        else {
            return UnitLoanActionCoverage::Partial;
        };
        if actions.len() != 9 + 2 * count {
            return UnitLoanActionCoverage::Partial;
        }
        let Some(UnitLoanAction::Cleanup {
            point: auxiliary_point,
            action: UnitIterationCleanupAction::Drop(auxiliary_drop),
        }) = actions.last()
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
        let suffix_valid = expected
            .later_uses
            .iter()
            .zip(&self.actual_returns)
            .enumerate()
            .all(|(index, ((planned, statement), actual))| {
                let offset = 8 + 2 * index;
                let [
                    UnitLoanAction::Begin(begin),
                    UnitLoanAction::Cleanup {
                        point,
                        action: UnitIterationCleanupAction::EndCallLoan(end),
                    },
                ] = &actions[offset..offset + 2]
                else {
                    return false;
                };
                begin == planned
                    && end == planned
                    && actual.loan == *planned
                    && planned.kind() == LoanKind::Shared
                    && planned.target() == auxiliary.target()
                    && planned.call().source_unit() == local
                    && planned.argument().source_unit() == local
                    && planned.call() != scope.source_call
                    && planned.call() != scope.consumer_call
                    && statement.source_unit() == local
                    && *statement != scope.entry
                    && *statement != scope.completion
                    && *statement != scope.consumer_statement
                    && expected.later_uses[..index]
                        .iter()
                        .all(|(previous, previous_statement)| {
                            previous.call() != planned.call()
                                && previous.argument() != planned.argument()
                                && previous_statement != statement
                        })
                    && *point == UnitDropPoint::CallReturn(planned.call())
                    && id_at(Some(actual.begin.clone()), offset)
                    && self.start.checked_add(offset + 1) == Some(actual.cursor)
                    && actual.owner == expected.auxiliary_owner
            });
        let last_begin = 8 + 2 * (count - 1);
        let valid = self.exit_empty
            && suffix_valid
            && id_at(Some(later_return.begin.clone()), last_begin)
            && self.start.checked_add(last_begin + 1) == Some(later_return.cursor)
            && self.return_drop_claimed
            && self.removed_auxiliary_owner.as_ref() == Some(&expected.auxiliary_owner)
            && self
                .auxiliary_drop_link
                .as_ref()
                .is_some_and(|(begin, drop)| {
                    id_at(Some(begin.clone()), last_begin)
                        && id_at(Some(drop.clone()), 8 + 2 * count)
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
