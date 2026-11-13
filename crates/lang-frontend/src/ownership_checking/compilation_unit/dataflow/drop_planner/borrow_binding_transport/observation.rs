//! Bounded admission of actual binding tokens using existing typed/ownership facts.
use super::super::action_source::ActionHandle;
use super::{DropPlanner, PendingBindingTransport, UseReturn, ValueState};
use crate::{
    name_resolution::{SymbolKind, UnitSymbolId},
    ownership_checking::{BorrowBindingStorage, UnitLoanFact, UnitLoanTarget},
    parser::Expression,
    type_checking::{
        BorrowReturnOrigin, BuiltinType, CallableResultSource, ExpressionCategory, UnitTypeKind,
    },
};

impl DropPlanner<'_, '_> {
    pub(super) fn observes_binding_source(
        &self,
        binding: UnitSymbolId,
        source: &UnitLoanFact,
        state: &ValueState,
    ) -> bool {
        let Some(fact) = self
            .checker
            .borrow_results
            .bindings
            .iter()
            .find(|fact| fact.binding() == binding)
        else {
            return false;
        };
        let UnitLoanTarget::Place(place) = source.target() else {
            return false;
        };
        let Some((checked, parameter)) = self.consumer_region_loan(
            source.call().expression(),
            source.argument().expression(),
            ExpressionCategory::Place,
        ) else {
            return false;
        };
        let Some(call) = self.checker.typed.call(source.call()) else {
            return false;
        };
        self.loop_boundaries.is_empty()
            && state.iterations.is_empty()
            && state.borrow_bindings.is_empty()
            && state.pending_borrow_results.as_slice() == [binding]
            && fact.parent().is_none()
            && fact.storage() == BorrowBindingStorage::BorrowedStorage
            && fact.initializer() == source.call()
            && fact.origin() == source.target()
            && checked == *source
            && call
                .borrow_return()
                .is_some_and(|result| result.origin() == BorrowReturnOrigin::Parameter(parameter))
            && self.checker.typed.types().get(call.return_type())
                == Some(&UnitTypeKind::Builtin(BuiltinType::String))
            && place.is_root()
            && self.checker.symbol_kind(place.root()) == Some(SymbolKind::Variable)
            && self.loan_provenance().formed_owned.contains(&place.root())
            && !self
                .loan_provenance()
                .iteration_providers
                .contains(&place.root())
            && matches!(state.values.as_slice(), [owner] if owner.symbol == place.root())
            && self
                .checker
                .parsed
                .ast()
                .expressions()
                .get(source.argument().expression())
                .is_ok_and(|node| matches!(node.payload(), Expression::Name))
    }

    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn observe_binding_use_return(
        &mut self,
        loan: UnitLoanFact,
        acquisition: Option<ActionHandle>,
        pending: &mut [PendingBindingTransport],
    ) {
        self.observe_auxiliary_binding_use(&loan, acquisition.as_ref(), pending);
        self.observe_distinct_binding_use(&loan, acquisition.as_ref(), pending);
        // Use the original argument place, never the canonical source root in loan.target().
        let Ok(Some(place)) = self.checker.place(loan.argument().expression()) else {
            return;
        };
        let single = pending.len() == 1;
        let Some(binding) = pending
            .iter_mut()
            .find(|pending| pending.binding == place.root())
        else {
            return;
        };
        let supported = single
            && binding.observable
            && place.is_root()
            && self.loop_boundaries.is_empty()
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
                .is_some_and(|(checked, _)| checked == loan)
            && self.checker.typed.call(loan.call()).is_some_and(|call| {
                call.result_source() == CallableResultSource::Owned
                    && self.checker.typed.types().get(call.return_type())
                        == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
            })
            && loan.target() == binding.source.target();
        if !supported {
            binding.use_return = UseReturn::Ambiguous;
            return;
        }
        let handle = self.iteration_actions.return_binding_use(
            binding.binding,
            loan,
            binding.source_begin.clone(),
            binding.handoff.clone(),
            acquisition,
        );
        binding.record_use(handle);
    }
}
