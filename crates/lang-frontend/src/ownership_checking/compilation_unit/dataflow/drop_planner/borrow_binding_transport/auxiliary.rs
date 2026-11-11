//! Independent linear auxiliary-binding observations, not legacy observable authority.
#[cfg(test)]
mod tests;
use super::{ActionHandle, DropPlanner, PendingBindingTransport};
use crate::{
    name_resolution::UnitSymbolId,
    ownership_checking::{UnitBorrowBindingTransportFact, UnitLoanFact},
    parser::Expression,
    type_checking::{BuiltinType, CallableResultSource, ExpressionCategory, UnitTypeKind},
};

#[derive(Clone, Debug)]
pub(super) struct AuxiliaryObservation {
    source: ActionHandle,
    auxiliary: ActionHandle,
    handoff: ActionHandle,
    use_return: Option<ActionHandle>,
}

impl PendingBindingTransport {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn activate_auxiliary_observation(
        &mut self,
        binding: UnitSymbolId,
        source: &UnitLoanFact,
        source_begin: Option<&ActionHandle>,
        auxiliary_begin: Option<&ActionHandle>,
        handoff: &ActionHandle,
    ) {
        if self.binding != binding {
            return;
        }
        let seen = std::mem::replace(&mut self.auxiliary_seen, true);
        self.auxiliary = None;
        if seen || self.observable || self.source != *source {
            return;
        }
        let Some((source, auxiliary)) = source_begin.zip(auxiliary_begin) else {
            return;
        };
        if source.same_record(auxiliary)
            || !source.precedes(handoff)
            || !auxiliary.precedes(handoff)
            || !self
                .source_begin
                .as_ref()
                .is_some_and(|actual| actual.same_record(source))
            || !self
                .handoff
                .as_ref()
                .is_some_and(|actual| actual.same_record(handoff))
        {
            return;
        }
        self.auxiliary = Some(AuxiliaryObservation {
            source: source.clone(),
            auxiliary: auxiliary.clone(),
            handoff: handoff.clone(),
            use_return: None,
        });
    }

    pub(super) fn publish_auxiliary_retirement(
        self,
        actions: &mut super::super::action_source::ActionSource,
        transaction: UnitBorrowBindingTransportFact,
        binding_end: ActionHandle,
    ) {
        let Some(token) = self.auxiliary else {
            return;
        };
        let Some(used) = token.use_return else {
            return;
        };
        if self.observable
            || transaction.binding() != self.binding
            || transaction.source_loan() != &self.source
            || token.source.same_record(&token.auxiliary)
            || !self
                .source_begin
                .as_ref()
                .is_some_and(|h| h.same_record(&token.source))
            || !self
                .handoff
                .as_ref()
                .is_some_and(|h| h.same_record(&token.handoff))
        {
            return;
        }
        // Publish only the real retirement observation. Do not return authority to
        // the legacy Region/child/Dependency Drop-cause dispatcher.
        let _ = actions.retire_binding_source(
            transaction,
            Some(token.source),
            Some(token.handoff),
            Some(used),
            binding_end,
        );
    }
}

impl DropPlanner<'_, '_> {
    pub(super) fn observe_auxiliary_binding_use(
        &mut self,
        loan: &UnitLoanFact,
        acquisition: Option<&ActionHandle>,
        pending: &mut [PendingBindingTransport],
    ) {
        let place = self
            .checker
            .place(loan.argument().expression())
            .ok()
            .flatten();
        let single = pending.len() == 1;
        for binding in pending {
            let Some(mut token) = binding.auxiliary.take() else {
                continue;
            };
            let valid = single
                && !binding.observable
                && token.use_return.is_none()
                && place
                    .as_ref()
                    .is_some_and(|p| p.is_root() && p.root() == binding.binding)
                && self.loop_boundaries.is_empty()
                && self.owner_loop.is_none()
                && self.owner_choice.is_none()
                && self.owner_branch_depth == 0
                && self.scope_depth == 1
                && acquisition.is_some()
                && self
                    .checker
                    .parsed
                    .ast()
                    .expressions()
                    .get(loan.argument().expression())
                    .is_ok_and(|node| matches!(node.payload(), Expression::Name))
                && self
                    .consumer_region_loan(
                        loan.call().expression(),
                        loan.argument().expression(),
                        ExpressionCategory::Place,
                    )
                    .is_some_and(|(checked, _)| checked == *loan)
                && self.checker.typed.call(loan.call()).is_some_and(|call| {
                    call.result_source() == CallableResultSource::Owned
                        && self.checker.typed.types().get(call.return_type())
                            == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
                })
                && loan.target() == binding.source.target()
                && !token.source.same_record(&token.auxiliary)
                && binding
                    .source_begin
                    .as_ref()
                    .is_some_and(|h| h.same_record(&token.source))
                && binding
                    .handoff
                    .as_ref()
                    .is_some_and(|h| h.same_record(&token.handoff));
            if !valid {
                continue;
            }
            token.use_return = self.iteration_actions.return_binding_use(
                binding.binding,
                loan.clone(),
                Some(token.source.clone()),
                Some(token.handoff.clone()),
                acquisition.cloned(),
            );
            if token.use_return.is_some() {
                binding.auxiliary = Some(token);
            }
        }
    }
}
