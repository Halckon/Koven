//! Protocol 2 admission: explicit Shared receiver on an independently owned parameter.
use super::*;
use crate::name_resolution::SymbolKind;
use crate::ownership_checking::compilation_unit::loan_region_validation::ExpectedReceiver;
use crate::ownership_checking::{
    OwnershipBindingKind, UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget,
};
use crate::parser::ParameterModeMarker;
use crate::type_checking::{CallableResultSource, UnitCallTarget, UnitItemId};

impl DropPlanner<'_, '_> {
    pub(super) fn loan_receiver_region_expected(
        &self,
        expression: ExpressionId,
        callable: UnitItemId,
        state: &ValueState,
    ) -> Option<ExpectedReceiver> {
        if !self.loop_boundaries.is_empty()
            || !state.iterations.is_empty()
            || !state.pending_loans.is_empty()
            || !state.pending_temporaries.is_empty()
            || !state.pending_borrows.is_empty()
            || !state.pending_capture_borrows.is_empty()
            || !state.pending_borrow_results.is_empty()
        {
            return None;
        }
        let id = self.checker.unit_expression(expression);
        let call = self.checker.typed.call(id)?;
        let typed = call.receiver()?;
        if !matches!(call.target(), UnitCallTarget::Symbol(_))
            || call.result_source() != CallableResultSource::Owned
            || call.aborts()
            || !call.instance().type_arguments().is_empty()
            || !call.arguments().is_empty()
            || typed.mode() != ParameterMode::Borrow
            || !matches!(
                self.checker.typed.types().get(call.return_type()),
                Some(UnitTypeKind::Builtin(BuiltinType::Int | BuiltinType::Unit))
            )
        {
            return None;
        }
        let node = self
            .checker
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .ok()?;
        let Expression::Call {
            callee, arguments, ..
        } = node.payload()
        else {
            return None;
        };
        if !arguments.is_empty() {
            return None;
        }
        let Expression::Member {
            receiver,
            safe: false,
            ..
        } = self
            .checker
            .parsed
            .ast()
            .expressions()
            .get(*callee)
            .ok()?
            .payload()
        else {
            return None;
        };
        let operand = self
            .checker
            .parsed
            .ast()
            .expressions()
            .get(*receiver)
            .ok()?;
        if !matches!(operand.payload(), Expression::Name)
            || typed.origin()
                != UnitCallReceiverOrigin::Expression(self.checker.unit_expression(*receiver))
            || self
                .checker
                .typed
                .expression_type(self.checker.unit_expression(*receiver))
                != Some(typed.ty())
        {
            return None;
        }
        let place = self.checker.place(*receiver).ok()??;
        let root = place.root();
        if !place.is_root()
            || self.checker.symbol_kind(root) != Some(SymbolKind::ValueParameter)
            || !self
                .checker
                .bindings
                .get(&root)
                .is_some_and(|binding| binding.kind() == OwnershipBindingKind::Owned)
        {
            return None;
        }
        let Item::Function { parameters, .. } = self
            .checker
            .parsed
            .ast()
            .items()
            .get(callable.item())
            .ok()?
            .payload()
        else {
            return None;
        };
        let parameter = parameters
            .iter()
            .find(|parameter| self.checker.marker_symbol(parameter.name).copied() == Some(root))?;
        if !matches!(parameter.mode_marker, Some(ParameterModeMarker::Own(_))) {
            return None;
        }
        let owner_origin = marker_span(parameter.name);
        let owned = &state.values[state.position(root)?];
        if owned.origin != owner_origin || owned.declaration != owner_origin {
            return None;
        }
        let UnitTypeKind::Nominal {
            declaration,
            arguments,
        } = self.checker.typed.types().get(typed.ty())?
        else {
            return None;
        };
        if !arguments.is_empty()
            || self.checker.typed.copyability(typed.ty()) != Copyability::MoveOnly
            || self.checker.typed.is_resource_type(typed.ty()) != Some(false)
            || !self
                .checker
                .typed
                .signatures()
                .declaration(*declaration)
                .and_then(|signature| signature.nominal())
                .is_some_and(|nominal| nominal.kind() == NominalKind::Class)
        {
            return None;
        }
        let [index] = self.loan_region_index().receivers.get(&id)?.as_slice() else {
            return None;
        };
        let fact = &self.checker.receiver_facts[*index];
        let contract = self.checker.receiver_contracts_by_call.get(&id)?;
        if fact.call() != id
            || fact.kind() != UnitReceiverOwnershipKind::SharedLoan
            || fact.is_receiver_reservation()
            || fact.activation_point().is_some()
            || fact.source() != typed.origin()
            || fact.receiver_type() != typed.ty()
            || fact.begin_span() != operand.span()
            || fact.end_span() != node.span()
            || fact.target() != &UnitReceiverOwnershipTarget::Place(place)
            || contract.call() != id
            || contract.target() != call.target()
            || contract.kind() != UnitCallArgumentOwnershipKind::SharedLoan
            || contract.source() != typed.origin()
            || contract.receiver_type() != typed.ty()
            || contract.category() != typed.category()
            || contract.receiver_span() != operand.span()
            || contract.call_span() != node.span()
            || contract.declaration_span() != fact.declaration_span()
        {
            return None;
        }
        Some(ExpectedReceiver {
            receiver: fact.clone(),
            root,
            receiver_origin: operand.span(),
            owner_origin,
        })
    }
}
