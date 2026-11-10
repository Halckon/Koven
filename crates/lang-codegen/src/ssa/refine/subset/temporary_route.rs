//! SPEC-0341: one ordinary temporary String literal call may select its caller.
//! This is only a routing trigger; the complete subset precheck remains mandatory.
use super::{Checker, RefineBody};
use lang_frontend::{
    ownership_checking::{LoanKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget},
    parser::{Expression, Item, Statement, StringPart},
    type_checking::{
        BuiltinType, CallableResultSource, ExpressionCategory, ParameterMode, UnitCallTarget,
        UnitExpressionId, UnitTypeKind,
    },
};

impl Checker<'_, '_> {
    pub(super) fn bounded_temporary_call(&self) -> Option<UnitExpressionId> {
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
        if !parameters.is_empty() || !inputs.parameters.is_empty() || !inputs.modes.is_empty() {
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
        let Expression::String { parts, .. } = expressions.get(source).ok()?.payload() else {
            return None;
        };
        if !matches!(expressions.get(*callee).ok()?.payload(), Expression::Name)
            || !parts.iter().all(|part| matches!(part, StringPart::Text(_)))
        {
            return None;
        }
        let ty = inputs.typed.expression_type(self.id(source))?;
        if !self.string(ty) {
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
            || argument.category() != ExpressionCategory::Temporary
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
        if facts.next().is_some()
            || fact.kind() != LoanKind::Shared
            || fact.target() != &UnitLoanTarget::Temporary(self.id(source))
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
                drop.point() == point && drop.target() == UnitDropTarget::Temporary(self.id(source))
            }))
        .then_some(id)
    }
}
