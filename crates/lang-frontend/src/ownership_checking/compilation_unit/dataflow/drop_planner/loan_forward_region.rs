//! Narrow Complete admission, independent of the broader Partial forwarding family.
use super::borrow_forward_transport::ForwardScope;
use super::*;
use crate::type_checking::{BorrowReturnOrigin, UnitCallTarget};

impl DropPlanner<'_, '_> {
    pub(super) fn enter_forward_region(&mut self, item: ItemId, state: &mut ValueState) {
        let Some(scope) = state.forward_scope else {
            return;
        };
        if self.forward_region_boundary_clear(state, false)
            && self.forward_region_signature(item, scope).is_some()
        {
            state.forward_scope = Some(self.iteration_actions.enter_forward_region(scope));
        }
    }

    fn forward_region_signature(&self, item: ItemId, scope: ForwardScope) -> Option<()> {
        let Item::Function {
            name,
            type_parameters,
            extension_receiver,
            ..
        } = self.checker.parsed.ast().items().get(item).ok()?.payload()
        else {
            return None;
        };
        if !type_parameters.is_empty() || extension_receiver.is_some() {
            return None;
        }
        // The broader Partial projection intentionally preserves its first-match policy.
        // Complete instead requires the original checker collections to be unambiguous.
        let mut sources = self
            .checker
            .borrow_results
            .forwarded_source_loans()
            .iter()
            .filter(|source| source.call() == scope.body);
        let source = sources.next()?;
        if sources.next().is_some() || source.argument() != scope.argument {
            return None;
        }
        let mut origins = self
            .checker
            .borrow_return_origins
            .iter()
            .filter(|origin| origin.expression() == scope.body);
        let origin = origins.next()?;
        if origins.next().is_some()
            || !matches!(origin.origin(), crate::ownership_checking::UnitLoanTarget::Place(place)
                if place.is_root() && place.root() == scope.root)
        {
            return None;
        }
        let symbol = *self.checker.marker_symbol(*name)?;
        let signatures = self.checker.typed.signatures();
        let wrapper = signatures
            .declarations()
            .iter()
            .find(|decl| decl.symbol() == symbol)?
            .callable()?;
        let call = self.checker.typed.call(scope.body)?;
        let UnitCallTarget::Declaration(target) = call.target() else {
            return None;
        };
        let callee = signatures.declaration(target)?.callable()?;
        for signature in [wrapper, callee] {
            let [parameter] = signature.parameters() else {
                return None;
            };
            if !signature.type_parameters().is_empty()
                || signature.receiver().is_some()
                || parameter.mode() != ParameterMode::Borrow
                || self.checker.typed.types().get(parameter.ty())
                    != Some(&UnitTypeKind::Builtin(BuiltinType::String))
                || signature.return_type() != parameter.ty()
                || signature.borrow_return()?.origin() != BorrowReturnOrigin::Parameter(0)
            {
                return None;
            }
        }
        let [argument] = call.arguments() else {
            return None;
        };
        if argument.argument_index() != 0
            || argument.parameter_index() != 0
            || argument.mode() != ParameterMode::Borrow
            || argument.crosses_thread()
            || call.result_source() != callee.result_source()
            || call.return_type() != callee.return_type()
            || argument.parameter_type() != callee.parameters()[0].ty()
            || self.checker.typed.expression_type(scope.argument) != Some(argument.parameter_type())
        {
            return None;
        }
        Some(())
    }

    /// Check before drop_all clears state: no obligation may disappear unobserved.
    pub(super) fn forward_region_boundary_clear(&self, state: &ValueState, live: bool) -> bool {
        let ValueState {
            forward_scope,
            branch_forward_plan,
            nested_forward,
            replace_consumer,
            lambda,
            borrow_bindings,
            pending_borrow_results,
            borrow_initializer_sources,
            pending_loans,
            borrow_call_transports,
            borrow_binding_transports,
            borrow_forward_transports,
            iterations,
            pending_temporaries,
            values,
            replacements,
            pending_borrows,
            pending_capture_borrows,
            closures,
            closure_asap,
            this,
        } = state;
        let transport_ok = if live {
            matches!(borrow_forward_transports.as_slice(), [transport]
                if forward_scope.is_some_and(|scope| scope.region.is_some() && transport.matches_region(scope)))
        } else {
            borrow_forward_transports.is_empty()
        };
        nested_forward.is_none()
            && self.scope_depth == 0
            && branch_forward_plan.is_none()
            && self.loop_boundaries.is_empty()
            && self.owner_choice.is_none()
            && self.owner_loop.is_none()
            && self.owner_branch_depth == 0
            && self.call_return_copy_source.is_none()
            && self.pending_borrow_handoffs.is_empty()
            && replace_consumer.is_none()
            && lambda.is_none()
            && this.is_none()
            && borrow_bindings.is_empty()
            && pending_borrow_results.is_empty()
            && borrow_initializer_sources.is_empty()
            && pending_loans.is_empty()
            && borrow_call_transports.is_empty()
            && borrow_binding_transports.is_empty()
            && transport_ok
            && iterations.is_empty()
            && pending_temporaries.is_empty()
            && values.is_empty()
            && replacements.is_empty()
            && pending_borrows.is_empty()
            && pending_capture_borrows.is_empty()
            && closures.is_empty()
            && closure_asap.is_empty()
    }
}
