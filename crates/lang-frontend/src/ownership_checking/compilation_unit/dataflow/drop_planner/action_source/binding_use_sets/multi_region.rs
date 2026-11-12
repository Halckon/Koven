//! Actual auxiliary-return and destruction capture; independent of the partial set's validity.
use super::super::super::{OwnedValue, borrow_binding_transport::PendingBindingTransport};
use super::super::BindingDropCause;
use super::*;
use crate::ownership_checking::compilation_unit::loan_binding_region_validation::OwnerSnapshot;
use crate::ownership_checking::{
    UnitLoanAuxiliarySequenceUse, UnitMultiBindingAuxiliaryUse, UnitMultiBindingSourceState,
    UnitOwnerTransportEvent,
};

#[derive(Clone, Default)]
pub(super) struct MultiState {
    pub(super) invalid: bool,
    pub(super) uses: Vec<UnitMultiBindingAuxiliaryUse>,
    pub(super) claimed: bool,
    pub(super) removed: Option<OwnerSnapshot>,
    pub(super) drop: Option<ActionHandle>,
    pub(super) exit: Option<usize>,
    pub(super) owner_events: Vec<UnitOwnerTransportEvent>,
}
pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) struct MultiAuxDropCause
{
    identity: Arc<()>,
    candidate: usize,
    end: UnitLoanActionId,
    removed: Option<OwnerSnapshot>,
}
fn snapshot(value: &OwnedValue) -> OwnerSnapshot {
    OwnerSnapshot {
        owner: value.owner.clone(),
        symbol: value.symbol,
        origin: value.origin,
        declaration: value.declaration,
        scope_depth: value.scope_depth,
    }
}
impl ActionSource {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn observe_multi_auxiliary_return(
        &mut self,
        loan: &UnitLoanFact,
        acquisition: Option<&ActionHandle>,
        values: &[OwnedValue],
        bindings: &[UnitSymbolId],
        transports: &[PendingBindingTransport],
    ) {
        for (index, c) in self.binding_use_sets.iter_mut().enumerate() {
            if !c
                .expected
                .auxiliary_uses
                .iter()
                .any(|p| p.call() == loan.call())
            {
                continue;
            }
            let next = c.multi.uses.len();
            let Some(begin) = acquisition else {
                c.multi.invalid = true;
                continue;
            };
            let owner = values
                .iter()
                .filter(|v| v.symbol == c.expected.auxiliary_owner.symbol)
                .collect::<Vec<_>>();
            let [owner] = owner.as_slice() else {
                c.multi.invalid = true;
                continue;
            };
            let actual = snapshot(owner);
            if c.multi.invalid
                || c.multi.exit.is_some()
                || c.multi.claimed
                || c.expected.auxiliary_uses.get(next) != Some(loan)
                || !Arc::ptr_eq(&begin.identity, &self.identity)
                || begin.index.checked_add(1) != Some(self.records.len())
                || !matches!(self.records.get(begin.index),Some(Record::Begin(l)) if l==loan)
                || actual != c.expected.auxiliary_owner
                || actual.owner.is_none()
                || c.expected.ordered_uses.get(next + c.returns.len())
                    != Some(&UseRole::Auxiliary(next))
            {
                c.multi.invalid = true;
                continue;
            }
            let progress = c.returns.len();
            let state = if progress < c.expected.consumers.len() {
                let source = values
                    .iter()
                    .filter(|v| v.symbol == c.expected.source_owner.symbol)
                    .collect::<Vec<_>>();
                let pending = transports
                    .iter()
                    .filter(|t| t.has_binding(c.expected.scope.binding))
                    .collect::<Vec<_>>();
                let ([source], [pending]) = (source.as_slice(), pending.as_slice()) else {
                    c.multi.invalid = true;
                    continue;
                };
                let Some((token, source_begin, handoff)) =
                    pending.use_set_obligation(c.expected.scope.binding, &c.expected.source)
                else {
                    c.multi.invalid = true;
                    continue;
                };
                if snapshot(source) != c.expected.source_owner
                    || c.end.is_some()
                    || c.source_cleanup.is_some()
                    || bindings
                        .iter()
                        .filter(|b| **b == c.expected.scope.binding)
                        .count()
                        != 1
                    || !Arc::ptr_eq(&token.identity, &self.identity)
                    || token.candidate != index
                    || token.next != progress
                    || c.source
                        .as_ref()
                        .is_none_or(|h| !h.same_record(source_begin))
                    || c.handoff.as_ref().is_none_or(|h| !h.same_record(handoff))
                {
                    c.multi.invalid = true;
                    continue;
                }
                let Some(owner) = source.owner.clone() else {
                    c.multi.invalid = true;
                    continue;
                };
                UnitMultiBindingSourceState::Active {
                    owner,
                    source_begin: UnitLoanActionId {
                        identity: source_begin.identity.clone(),
                        index: source_begin.index,
                    },
                    handoff: UnitLoanActionId {
                        identity: handoff.identity.clone(),
                        index: handoff.index,
                    },
                }
            } else {
                let Some((removed, drop)) = c.source_cleanup.as_ref() else {
                    c.multi.invalid = true;
                    continue;
                };
                let Some(end) = c.end.as_ref() else {
                    c.multi.invalid = true;
                    continue;
                };
                if c.source_invalid
                    || removed != &c.expected.source_owner
                    || !drop.precedes(begin)
                    || values
                        .iter()
                        .any(|v| v.symbol == c.expected.source_owner.symbol)
                    || bindings.contains(&c.expected.scope.binding)
                    || transports
                        .iter()
                        .any(|t| t.has_binding(c.expected.scope.binding))
                {
                    c.multi.invalid = true;
                    continue;
                }
                UnitMultiBindingSourceState::Retired {
                    binding_end: UnitLoanActionId {
                        identity: end.identity.clone(),
                        index: end.index,
                    },
                    drop: UnitLoanActionId {
                        identity: drop.identity.clone(),
                        index: drop.index,
                    },
                }
            };
            c.multi.uses.push(UnitMultiBindingAuxiliaryUse {
                actual: UnitLoanAuxiliarySequenceUse {
                    loan: loan.clone(),
                    begin: UnitLoanActionId {
                        identity: begin.identity.clone(),
                        index: begin.index,
                    },
                    end: UnitLoanActionId {
                        identity: self.identity.clone(),
                        index: self.records.len(),
                    },
                    owner: actual.owner.unwrap(),
                },
                returned_binding_uses: progress,
                source_state: state,
            });
        }
    }
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn multi_auxiliary_drop_cause(
        &mut self,
        point: UnitDropPoint,
        symbol: UnitSymbolId,
    ) -> Option<BindingDropCause> {
        let index = self
            .binding_use_sets
            .iter()
            .position(|c| c.expected.auxiliary_owner.symbol == symbol && c.multi.exit.is_none())?;
        let c = &mut self.binding_use_sets[index];
        if c.multi.uses.len() < c.expected.auxiliary_uses.len() {
            return None;
        }
        let returned = c.multi.uses.last()?;
        if c.multi.invalid
            || c.multi.claimed
            || point != UnitDropPoint::CallReturn(returned.loan().call())
            || returned.end().index.checked_add(1) != Some(self.records.len())
        {
            c.multi.invalid = true;
            return None;
        }
        c.multi.claimed = true;
        Some(BindingDropCause::MultiAuxiliary(MultiAuxDropCause {
            identity: self.identity.clone(),
            candidate: index,
            end: returned.end(),
            removed: None,
        }))
    }
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn verify_removed_multi_auxiliary(
        &mut self,
        mut cause: MultiAuxDropCause,
        value: &OwnedValue,
    ) -> Option<MultiAuxDropCause> {
        let c = self.binding_use_sets.get_mut(cause.candidate)?;
        let actual = snapshot(value);
        if !Arc::ptr_eq(&cause.identity, &self.identity)
            || c.multi.invalid
            || !c.multi.claimed
            || cause.removed.is_some()
            || actual != c.expected.auxiliary_owner
            || c.multi.uses.last().is_none_or(|r| r.end() != cause.end)
        {
            c.multi.invalid = true;
            return None;
        }
        cause.removed = Some(actual);
        Some(cause)
    }
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn record_multi_auxiliary_drop(
        &mut self,
        cause: MultiAuxDropCause,
        drop: ActionHandle,
    ) {
        let Some(c) = self.binding_use_sets.get_mut(cause.candidate) else {
            return;
        };
        if !Arc::ptr_eq(&cause.identity, &self.identity)
            || !Arc::ptr_eq(&drop.identity, &self.identity)
            || c.multi.invalid
            || !c.multi.claimed
            || c.multi.drop.is_some()
            || cause.removed.as_ref() != Some(&c.expected.auxiliary_owner)
            || c.multi.uses.last().is_none_or(|r| r.end() != cause.end)
            || cause.end.index.checked_add(1) != Some(drop.index)
        {
            c.multi.invalid = true;
            return;
        }
        c.multi.removed = cause.removed;
        c.multi.drop = Some(drop);
    }
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn multi_region_owner_event(
        &mut self,
        event: &UnitOwnerTransportEvent,
    ) {
        for c in &mut self.binding_use_sets {
            if c.multi.exit.is_none() {
                c.multi.owner_events.push(event.clone());
            }
        }
    }
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn finish_multi_region_statement(
        &mut self,
        statement: UnitStatementId,
        continues: bool,
        empty: bool,
    ) {
        for c in &mut self.binding_use_sets {
            if statement != c.expected.scope.completion {
                continue;
            }
            if !continues || !empty || c.multi.exit.is_some() {
                c.multi.invalid = true;
            }
            c.multi.exit = Some(self.records.len());
        }
    }
}
