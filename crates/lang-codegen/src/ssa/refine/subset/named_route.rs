//! SPEC-0341 bounded caller routing triggers: owned String argument calls, owned-class
//! receiver calls, and literal String temporary calls. The complete subset precheck remains mandatory.
use super::{Checker, RefineBody};
use lang_frontend::{
    ownership_checking::{
        LoanKind, OwnershipBindingKind, UnitDropPoint, UnitDropTarget, UnitLoanTarget,
        UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget,
    },
    parser::{Expression, Item, ParameterModeMarker, Statement, StringPart},
    type_checking::{
        BuiltinType, CallableResultSource, ExpressionCategory, ParameterMode,
        UnitCallReceiverOrigin, UnitCallTarget, UnitExpressionId, UnitTypeKind,
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

    pub(super) fn bounded_receiver_route(&self) -> bool {
        self.bounded_receiver_candidate().unwrap_or(false)
    }

    fn bounded_receiver_candidate(&self) -> Option<bool> {
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
        if !arguments.is_empty() {
            return None;
        }
        let Expression::Member {
            receiver: source,
            safe: false,
            ..
        } = expressions.get(*callee).ok()?.payload()
        else {
            return None;
        };
        if !matches!(expressions.get(*source).ok()?.payload(), Expression::Name)
            || self.reference(*source) != Some(*symbol)
        {
            return None;
        }
        let ty = self.symbol_type(*symbol)?;
        if !self.class(ty)
            || !matches!(self.kind(ty), Some(UnitTypeKind::Nominal { arguments, .. }) if arguments.is_empty())
            || inputs.typed.is_resource_type(ty) != Some(false)
        {
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
        let receiver = call.receiver()?;
        if !call.arguments().is_empty()
            || call.aborts()
            || call.result_source() != CallableResultSource::Owned
            || !call.instance().type_arguments().is_empty()
            || !matches!(
                self.kind(self.c(call.return_type())),
                Some(UnitTypeKind::Builtin(BuiltinType::Int | BuiltinType::Unit))
            )
            || receiver.mode() != ParameterMode::Borrow
            || self.c(receiver.ty()) != ty
            || receiver.origin() != UnitCallReceiverOrigin::Expression(self.id(*source))
        {
            return None;
        }
        let site = inputs.source_plan.call_site(inputs.source_token, id)?;
        if site.dispatch_key().is_none() || !site.delegation().is_empty() {
            return None;
        }
        let fact = inputs.owned.receiver_fact(id)?;
        let UnitReceiverOwnershipTarget::Place(place) = fact.target() else {
            return None;
        };
        if fact.kind() != UnitReceiverOwnershipKind::SharedLoan
            || fact.source() != receiver.origin()
            || fact.receiver_type() != receiver.ty()
            || fact.is_receiver_reservation()
            || fact.activation_point().is_some()
            || !place.is_root()
            || place.root() != *symbol
        {
            return None;
        }
        let point = UnitDropPoint::CallReturn(id);
        Some(
            inputs
                .owned
                .receiver_call_return_ends()
                .iter()
                .any(|end| end.receiver() == fact && end.point() == point)
                && inputs.owned.drops().iter().any(|drop| {
                    drop.point() == point && drop.target() == UnitDropTarget::Named(*symbol)
                }),
        )
    }

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
        if arguments.is_empty()
            || !matches!(expressions.get(*callee).ok()?.payload(), Expression::Name)
        {
            return None;
        }
        let id = self.id(*expression);
        let call = inputs.typed.call(id)?;
        if call.arguments().len() != arguments.len()
            || !matches!(call.target(), UnitCallTarget::Declaration(_))
            || call.receiver().is_some()
            || call.aborts()
            || call.result_source() != CallableResultSource::Owned
            || !call.instance().type_arguments().is_empty()
            || !matches!(
                self.kind(self.c(call.return_type())),
                Some(UnitTypeKind::Builtin(BuiltinType::Int | BuiltinType::Unit))
            )
        {
            return None;
        }
        // Parameter slots form a complete permutation; evaluation remains source ordered.
        let mut parameters = call
            .arguments()
            .iter()
            .map(|a| a.parameter_index())
            .collect::<Vec<_>>();
        parameters.sort_unstable();
        if parameters != (0..arguments.len()).collect::<Vec<_>>() {
            return None;
        }
        for (index, source) in arguments.iter().enumerate() {
            let source = source.value;
            let Expression::String { parts, .. } = expressions.get(source).ok()?.payload() else {
                return None;
            };
            let ty = inputs.typed.expression_type(self.id(source))?;
            let argument = call.arguments().get(index)?;
            if !parts.iter().all(|part| matches!(part, StringPart::Text(_)))
                || !self.string(ty)
                || argument.mode() != ParameterMode::Borrow
                || argument.argument_index() != index
                || argument.category() != ExpressionCategory::Temporary
                || argument.crosses_thread()
                || self.c(argument.parameter_type()) != ty
            {
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
            if !inputs
                .owned
                .direct_call_loan_ends()
                .iter()
                .any(|end| end.loan() == fact && end.point() == point)
                || !inputs.owned.drops().iter().any(|drop| {
                    drop.point() == point
                        && drop.target() == UnitDropTarget::Temporary(self.id(source))
                })
            {
                return None;
            }
        }
        let site = inputs.source_plan.call_site(inputs.source_token, id)?;
        if site.dispatch_key().is_none() || !site.delegation().is_empty() {
            return None;
        }
        Some(id)
    }
}

pub(in crate::ssa::refine) fn bounded_named_call(
    inputs: &super::RefineInputs<'_>,
) -> Option<UnitExpressionId> {
    super::query_checker(inputs).bounded_named_call()
}

pub(in crate::ssa::refine) fn bounded_temporary_call(
    inputs: &super::RefineInputs<'_>,
) -> Option<UnitExpressionId> {
    super::query_checker(inputs).bounded_temporary_call()
}
