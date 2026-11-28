//! An imported shared parameter can forward its entry loan without taking ownership.
use super::{Checker, RefineBody};
use lang_frontend::{
    ownership_checking::{
        OwnershipBindingKind, UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget,
    },
    parser::{Expression, Item, Statement},
    type_checking::{
        BuiltinType, CallableResultSource, Copyability, ExpressionCategory, ParameterMode,
        UnitCallReceiverOrigin, UnitCallTarget, UnitTypeKind,
    },
};

impl Checker<'_, '_> {
    /// This is an entry-loan forwarding route, not eligible for the owned receiver receipt.
    pub(super) fn imported_receiver_route(&self) -> Option<()> {
        let inputs = self.inputs;
        if inputs.receiver.is_some()
            || inputs.static_self.is_some()
            || inputs.borrow_return.is_some()
            || inputs.carrier_return.is_some()
            || !inputs.substitutions.is_empty()
            || !self.unit(inputs.return_type)
            || inputs.modes.as_slice() != [ParameterMode::Borrow]
        {
            return None;
        }
        let [symbol] = inputs.parameters.as_slice() else {
            return None;
        };
        let ast = inputs.parsed.ast();
        let Item::Function {
            parameters,
            type_parameters,
            extension_receiver: None,
            ..
        } = ast.items().get(inputs.function_item?).ok()?.payload()
        else {
            return None;
        };
        let [parameter] = parameters.as_slice() else {
            return None;
        };
        if !type_parameters.is_empty() || parameter.mode_marker.is_some() {
            return None;
        }
        let RefineBody::Block(body) = inputs.body else {
            return None;
        };
        let Statement::Block { elements } = ast.statements().get(body).ok()?.payload() else {
            return None;
        };
        let [statement] = elements.as_slice() else {
            return None;
        };
        let Statement::Expression { expression } = ast.statements().get(*statement).ok()?.payload()
        else {
            return None;
        };
        let call_node = ast.expressions().get(*expression).ok()?;
        let Expression::Call {
            callee, arguments, ..
        } = call_node.payload()
        else {
            return None;
        };
        let Expression::Member {
            receiver: root,
            safe: false,
            ..
        } = ast.expressions().get(*callee).ok()?.payload()
        else {
            return None;
        };
        let root_node = ast.expressions().get(*root).ok()?;
        let ty = self.symbol_type(*symbol)?;
        if !arguments.is_empty()
            || !matches!(root_node.payload(), Expression::Name)
            || self.reference(*root) != Some(*symbol)
            || !self.class(ty)
            || !matches!(self.kind(ty), Some(UnitTypeKind::Nominal { arguments, .. }) if arguments.is_empty())
            || self.copyability_of(ty) != Copyability::MoveOnly
            || inputs.typed.is_resource_type(ty) != Some(false)
            || !inputs.owned.bindings().iter().any(|binding| {
                binding.symbol() == *symbol && binding.kind() == OwnershipBindingKind::Shared
            })
        {
            return None;
        }
        let id = self.id(*expression);
        let root_id = self.id(*root);
        let call = inputs.typed.call(id)?;
        let receiver = call.receiver()?;
        if !matches!(call.target(), UnitCallTarget::Symbol(_))
            || call.aborts()
            || !call.arguments().is_empty()
            || !call.instance().type_arguments().is_empty()
            || call.result_source() != CallableResultSource::Owned
            || self.kind(self.c(call.return_type()))
                != Some(&UnitTypeKind::Builtin(BuiltinType::Int))
            || receiver.mode() != ParameterMode::Borrow
            || receiver.category() != ExpressionCategory::Place
            || receiver.origin() != UnitCallReceiverOrigin::Expression(root_id)
            || self.c(receiver.ty()) != ty
            || inputs.typed.expression_type(root_id) != Some(receiver.ty())
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
        (fact.call() == id
            && fact.kind() == UnitReceiverOwnershipKind::SharedLoan
            && fact.source() == receiver.origin()
            && fact.receiver_type() == receiver.ty()
            && !fact.is_receiver_reservation()
            && fact.activation_point().is_none()
            && place.is_root()
            && place.root() == *symbol
            && fact.begin_span() == root_node.span()
            && fact.end_span() == call_node.span())
        .then_some(())
    }
}
