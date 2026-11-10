//! SPEC-0341: one ordinary owned String argument call may select its caller.
//! This is only a routing trigger; the complete subset precheck remains mandatory.
use super::{Checker, RefineBody};
use lang_frontend::{
    ownership_checking::{
        LoanKind, OwnershipBindingKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget,
    },
    parser::{Expression, Item, ParameterModeMarker, Statement},
    type_checking::{
        BuiltinType, CallableResultSource, ExpressionCategory, ParameterMode, UnitCallTarget,
        UnitExpressionId, UnitTypeKind,
    },
};

impl Checker<'_, '_> {
    pub(super) fn bounded_named_call(&self) -> Option<UnitExpressionId> {
        let inputs = self.inputs;
        if inputs.receiver.is_some()
            || inputs.static_self.is_some()
            || inputs.borrow_return.is_some()
            || inputs.carrier_return.is_some()
            || !self.unit(inputs.return_type)
        {
            return None;
        }
        let Item::Function {
            parameters,
            type_parameters,
            extension_receiver: None,
            ..
        } = inputs
            .parsed
            .ast()
            .items()
            .get(inputs.function_item?)
            .ok()?
            .payload()
        else {
            return None;
        };
        if !type_parameters.is_empty() || !inputs.substitutions.is_empty() {
            return None;
        }
        let [parameter] = parameters.as_slice() else {
            return None;
        };
        if !matches!(parameter.mode_marker, Some(ParameterModeMarker::Own(_))) {
            return None;
        }
        let [symbol] = inputs.parameters.as_slice() else {
            return None;
        };
        if inputs.modes.as_slice() != [ParameterMode::Value] {
            return None;
        }
        let RefineBody::Block(body) = inputs.body else {
            return None;
        };
        let Statement::Block { elements } =
            inputs.parsed.ast().statements().get(body).ok()?.payload()
        else {
            return None;
        };
        let [statement] = elements.as_slice() else {
            return None;
        };
        let Statement::Expression { expression } = inputs
            .parsed
            .ast()
            .statements()
            .get(*statement)
            .ok()?
            .payload()
        else {
            return None;
        };
        let expressions = inputs.parsed.ast().expressions();
        let Expression::Call {
            callee, arguments, ..
        } = expressions.get(*expression).ok()?.payload()
        else {
            return None;
        };
        let [argument] = arguments.as_slice() else {
            return None;
        };
        let source = argument.value;
        if !matches!(expressions.get(*callee).ok()?.payload(), Expression::Name)
            || !matches!(expressions.get(source).ok()?.payload(), Expression::Name)
            || self.reference(source) != Some(*symbol)
        {
            return None;
        }
        let ty = self.symbol_type(*symbol)?;
        if !self.string(ty) {
            return None;
        }
        let binding = inputs
            .owned
            .bindings()
            .iter()
            .find(|b| b.symbol() == *symbol)?;
        if binding.kind() != OwnershipBindingKind::Owned {
            return None;
        }
        let id = self.id(*expression);
        let call = inputs.typed.call(id)?;
        let [argument] = call.arguments() else {
            return None;
        };
        if !matches!(call.target(), UnitCallTarget::Declaration(_))
            || call.receiver().is_some()
            || call.aborts()
            || call.result_source() != CallableResultSource::Owned
            || !call.instance().type_arguments().is_empty()
            || !matches!(
                self.kind(self.c(call.return_type())),
                Some(UnitTypeKind::Builtin(BuiltinType::Int | BuiltinType::Unit))
            )
            || argument.mode() != ParameterMode::Borrow
            || argument.argument_index() != 0
            || argument.parameter_index() != 0
            || argument.category() != ExpressionCategory::Place
            || argument.crosses_thread()
            || self.c(argument.parameter_type()) != ty
            || inputs.typed.expression_type(self.id(source)) != Some(ty)
        {
            return None;
        }
        let site = inputs.source_plan.call_site(inputs.source_token, id)?;
        if site.dispatch_key().is_none() || !site.delegation().is_empty() {
            return None;
        }
        let mut facts = inputs
            .owned
            .loans()
            .iter()
            .filter(|f| f.call() == id && f.argument() == self.id(source));
        let fact = facts.next()?;
        let UnitLoanTarget::Place(place) = fact.target() else {
            return None;
        };
        if facts.next().is_some()
            || fact.kind() != LoanKind::Shared
            || !place.is_root()
            || place.root() != *symbol
        {
            return None;
        }
        let point = UnitDropPoint::CallReturn(id);
        (inputs
            .owned
            .direct_call_loan_ends()
            .iter()
            .any(|end| end.loan() == fact && end.point() == point)
            && inputs.owned.drops().iter().any(|drop| {
                drop.point() == point && drop.target() == UnitDropTarget::Named(*symbol)
            }))
        .then_some(id)
    }
}

pub(super) fn bounded_named_call(inputs: &super::RefineInputs<'_>) -> Option<UnitExpressionId> {
    super::query_checker(inputs).bounded_named_call()
}
