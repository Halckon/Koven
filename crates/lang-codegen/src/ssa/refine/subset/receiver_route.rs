//! SPEC-0341: one ordinary owned-class receiver call may select its caller.
//! This is only a routing trigger; the complete subset precheck remains mandatory.
use super::{Checker, RefineBody};
use lang_frontend::{
    ownership_checking::{
        OwnershipBindingKind, UnitDropPoint, UnitDropTarget, UnitReceiverOwnershipKind,
        UnitReceiverOwnershipTarget,
    },
    parser::{Expression, Item, ParameterModeMarker, Statement},
    type_checking::{
        BuiltinType, CallableResultSource, ParameterMode, UnitCallReceiverOrigin, UnitTypeKind,
    },
};

impl Checker<'_, '_> {
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
}
