//! Carry actual pending source tokens through explicit local borrow bindings.
mod auxiliary;
mod child;
pub(super) mod dependency_drop;
mod observation;
use super::action_source::ActionHandle;
use super::{DropPlanner, PlannerDropPoint, ValueState};
use crate::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::{
        UnitBorrowBindingTransportFact, UnitIterationCleanupAction as Action, UnitLoanFact,
    },
    type_checking::{CallableResultSource, UnitCallTarget},
};

#[derive(Clone, Debug)]
pub(super) struct PendingBindingTransport {
    binding: UnitSymbolId,
    source: UnitLoanFact,
    source_begin: Option<ActionHandle>,
    handoff: Option<ActionHandle>,
    use_return: UseReturn,
    observable: bool,
    child: child::ChildState,
    dependency_owner: Option<super::OwnedValue>,
    auxiliary: Option<auxiliary::AuxiliaryObservation>,
    auxiliary_seen: bool,
}

#[derive(Clone, Debug)]
enum UseReturn {
    Unused,
    Unique(ActionHandle),
    Ambiguous,
}

// Observation identities never participate in the existing semantic retain condition.
impl PartialEq for PendingBindingTransport {
    fn eq(&self, other: &Self) -> bool {
        self.binding == other.binding && self.source == other.source
    }
}
impl Eq for PendingBindingTransport {}
impl PendingBindingTransport {
    fn record_use(&mut self, handle: Option<ActionHandle>) {
        self.use_return = match (&self.use_return, handle) {
            (UseReturn::Unused, Some(handle)) => UseReturn::Unique(handle),
            _ => UseReturn::Ambiguous,
        };
    }

    pub(super) fn commit_handoff(&mut self, binding: UnitSymbolId, handle: ActionHandle) {
        if self.binding == binding {
            self.handoff = Some(handle);
        }
    }

    pub(super) fn reconcile(&mut self, alternatives: &[Self]) {
        // This narrowly linear observation capability never crosses a merge.
        self.auxiliary = None;
        self.auxiliary_seen = true;
        let mut matching = alternatives.iter().filter(|other| *other == self);
        let other = matching.next();
        let unique = matching.next().is_none();
        let same = |left: &Option<ActionHandle>, right: &Option<ActionHandle>| {
            left.as_ref()
                .zip(right.as_ref())
                .is_some_and(|(a, b)| a.same_record(b))
        };
        if let Some(other) = other.filter(|_| unique) {
            if !same(&self.source_begin, &other.source_begin) {
                self.source_begin = None;
            }
            if !same(&self.handoff, &other.handoff) {
                self.handoff = None;
            }
            let same_use = match (&self.use_return, &other.use_return) {
                (UseReturn::Unused, UseReturn::Unused) => true,
                (UseReturn::Unique(a), UseReturn::Unique(b)) => a.same_record(b),
                _ => false,
            };
            if !same_use {
                self.use_return = UseReturn::Ambiguous;
            }
            self.observable &= other.observable;
            if self.dependency_owner != other.dependency_owner {
                self.dependency_owner = None;
            }
            self.child.reconcile(&other.child);
            if self.source_begin.is_none() || self.handoff.is_none() {
                self.child.lose();
                self.dependency_owner = None;
            }
        } else {
            self.source_begin = None;
            self.handoff = None;
            self.use_return = UseReturn::Ambiguous;
            self.observable = false;
            self.dependency_owner = None;
            self.child.lose();
        }
    }
}

impl DropPlanner<'_, '_> {
    pub(super) fn transport_borrow_binding_source(&self, id: ExpressionId, state: &mut ValueState) {
        let source_call = self.checker.unit_expression(id);
        let Some(call) = self.checker.typed.call(source_call) else {
            return;
        };
        if call.receiver().is_some()
            || !matches!(call.target(), UnitCallTarget::Declaration(_))
            || !matches!(call.result_source(), CallableResultSource::Borrow(_))
        {
            return;
        }
        let Some(binding) = self
            .borrow_bindings_initialized_by(source_call)
            .find(|binding| {
                binding.parent().is_none()
                    && state.pending_borrow_results.contains(&binding.binding())
            })
        else {
            return;
        };
        let Some(source) = binding.source_loan() else {
            return;
        };
        let Some(index) =
            state
                .pending_loans
                .iter()
                .position(|super::model::PendingLoan(action, _, _)| {
                    matches!(action, Action::EndCallLoan(loan)
                if loan.call() == source.call()
                && loan.argument() == source.argument()
                && loan.target() == binding.origin()
                && self.observes_borrow_auxiliary(loan, state))
                })
        else {
            return;
        };
        // The normal-return retain must not discard this token. Move it intact;
        // the binding endpoint alone cannot stand in for a source-token end.
        let super::model::PendingLoan(Action::EndCallLoan(source), _, source_begin) =
            state.pending_loans.remove(index)
        else {
            unreachable!("matched actual source token")
        };
        let observable = self.observes_binding_source(binding.binding(), &source, state);
        let dependency_owner = observable
            .then(|| self.binding_dependency_owner(&source, state))
            .flatten();
        state
            .borrow_binding_transports
            .push(PendingBindingTransport {
                binding: binding.binding(),
                source,
                source_begin,
                handoff: None,
                use_return: UseReturn::Unused,
                observable,
                child: child::ChildState::Never,
                dependency_owner,
                auxiliary: None,
                auxiliary_seen: false,
            });
    }

    pub(super) fn finish_borrow_binding_transport(
        &mut self,
        binding: UnitSymbolId,
        point: PlannerDropPoint,
        state: &mut ValueState,
    ) -> Option<(UnitBorrowBindingTransportFact, PendingBindingTransport)> {
        // Invoked only after an actual initialized binding is removed. Existing
        // child-before-parent termination and owner cleanup remain authoritative.
        let index = state
            .borrow_binding_transports
            .iter()
            .position(|pending| pending.binding == binding)?;
        let pending = state.borrow_binding_transports.remove(index);
        let transaction = UnitBorrowBindingTransportFact {
            source: pending.source.clone(),
            binding: pending.binding,
            point: point.into_unit(self.checker.source_unit),
        };
        // Keep the old projection's publication point, before the binding-end record.
        self.borrow_binding_transports.push(transaction.clone());
        Some((transaction, pending))
    }
}

impl PendingBindingTransport {
    pub(super) fn publish_retirement(
        self,
        actions: &mut super::action_source::ActionSource,
        transaction: UnitBorrowBindingTransportFact,
        binding_end: ActionHandle,
    ) -> Option<ActionHandle> {
        if self.observable {
            let use_return = match &self.use_return {
                UseReturn::Unique(handle) => Some(handle.clone()),
                UseReturn::Unused | UseReturn::Ambiguous => None,
            };
            let retirement = actions.retire_binding_source(
                transaction,
                self.source_begin,
                self.handoff,
                use_return,
                binding_end,
            )?;
            if matches!(self.use_return, UseReturn::Unused)
                && let child::ChildState::Retired {
                    retirement: child, ..
                } = self.child
                && let Some(owner) = self.dependency_owner
            {
                actions.attach_binding_dependency(&retirement, child, owner);
            }
            Some(retirement)
        } else {
            self.publish_auxiliary_retirement(actions, transaction, binding_end);
            None
        }
    }
}

#[cfg(test)]
mod tests;
