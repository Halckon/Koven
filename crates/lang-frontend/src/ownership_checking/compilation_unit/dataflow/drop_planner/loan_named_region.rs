//! Protocol 3: one direct Shared argument from a real owned caller String parameter.
use super::*;
use crate::name_resolution::SymbolKind;
use crate::ownership_checking::compilation_unit::loan_region_validation::ExpectedNamed;
use crate::ownership_checking::{LoanKind, OwnershipBindingKind, UnitLoanTarget};
use crate::parser::ParameterModeMarker;
use crate::type_checking::{CallableResultSource, UnitCallTarget, UnitItemId};

impl DropPlanner<'_, '_> {
    pub(super) fn is_named_loan_region(&self, expression: ExpressionId) -> bool {
        self.named_loan_region_operand(expression).is_some()
    }

    fn named_loan_region_operand(&self, expression: ExpressionId) -> Option<ExpressionId> {
        let Expression::Call {
            callee, arguments, ..
        } = self
            .checker
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .ok()?
            .payload()
        else {
            return None;
        };
        let [argument] = arguments.as_slice() else {
            return None;
        };
        let ast = self.checker.parsed.ast().expressions();
        if !matches!(ast.get(*callee).ok()?.payload(), Expression::Name)
            || !matches!(ast.get(argument.value).ok()?.payload(), Expression::Name)
        {
            return None;
        }
        Some(argument.value)
    }

    pub(super) fn loan_named_region_expected(
        &self,
        expression: ExpressionId,
        callable: UnitItemId,
        state: &ValueState,
    ) -> Option<ExpectedNamed> {
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
        let argument = self.named_loan_region_operand(expression)?;
        let id = self.checker.unit_expression(expression);
        let argument_id = self.checker.unit_expression(argument);
        let call_origin = self
            .checker
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .ok()?
            .span();
        let call = self.checker.typed.call(id)?;
        if !matches!(call.target(), UnitCallTarget::Declaration(_))
            || call.receiver().is_some()
            || call.result_source() != CallableResultSource::Owned
            || call.aborts()
            || !call.instance().type_arguments().is_empty()
            || !matches!(
                self.checker.typed.types().get(call.return_type()),
                Some(UnitTypeKind::Builtin(BuiltinType::Int | BuiltinType::Unit))
            )
        {
            return None;
        }
        let [typed] = call.arguments() else {
            return None;
        };
        let [contract] = self.checker.contracts_by_call.get(&id)?.as_slice() else {
            return None;
        };
        if typed.argument_index() != 0
            || typed.mode() != ParameterMode::Borrow
            || typed.category() != ExpressionCategory::Place
            || contract.category() != typed.category()
            || self.checker.typed.expression_category(argument_id) != Some(typed.category())
            || typed.crosses_thread()
            || contract.crosses_thread()
            || contract.kind() != UnitCallArgumentOwnershipKind::SharedLoan
            || contract.call() != id
            || contract.argument() != argument_id
            || contract.parameter_index() != typed.parameter_index()
            || contract.parameter_type() != typed.parameter_type()
            || self.checker.typed.expression_type(argument_id) != Some(typed.parameter_type())
            || !matches!(
                self.checker.typed.types().get(typed.parameter_type()),
                Some(UnitTypeKind::Builtin(BuiltinType::String))
            )
        {
            return None;
        }
        let place = self.checker.place(argument).ok()??;
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
        let owner = &state.values[state.position(root)?];
        if owner.origin != owner_origin || owner.declaration != owner_origin {
            return None;
        }
        let [index] = self.loan_region_index().loans.get(&id)?.as_slice() else {
            return None;
        };
        let loan = &self.checker.loans[*index];
        let origin = self
            .checker
            .parsed
            .ast()
            .expressions()
            .get(argument)
            .ok()?
            .span();
        if loan.call() != id
            || loan.argument() != argument_id
            || loan.kind() != LoanKind::Shared
            || loan.target() != &UnitLoanTarget::Place(place)
            || loan.begin_span() != origin
            || contract.argument_span() != origin
            || contract.loan_begin_span() != origin
            || contract.call_span() != call_origin
            || loan.end_span() != call_origin
            || loan.parameter_span() != contract.parameter_span()
        {
            return None;
        }
        Some(ExpectedNamed {
            loan: loan.clone(),
            root,
            argument_origin: origin,
            call_origin,
            owner_origin,
        })
    }
}
