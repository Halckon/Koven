//! Narrow same-owner receiver sequence route; independent of optional observer and single-call receipts.
use super::{Checker, RefineBody};
use lang_frontend::{
    ownership_checking::{UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget},
    parser::{Expression, Item, ParameterModeMarker, Statement},
    type_checking::{
        BuiltinType, CallableResultSource, Copyability, ExpressionCategory, ParameterMode,
        UnitCallReceiverOrigin, UnitCallTarget, UnitCallableTarget, UnitTypeKind,
    },
};

impl Checker<'_, '_> {
    pub(super) fn owned_receiver_sequence_route(&self) -> Option<()> {
        let inputs = self.inputs;
        if inputs.receiver.is_some()
            || inputs.static_self.is_some()
            || inputs.borrow_return.is_some()
            || inputs.carrier_return.is_some()
            || !inputs.substitutions.is_empty()
            || !self.unit(inputs.return_type)
            || inputs.modes.as_slice() != [ParameterMode::Value]
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
        if !type_parameters.is_empty()
            || !matches!(parameter.mode_marker, Some(ParameterModeMarker::Own(_)))
        {
            return None;
        }
        let ty = self.symbol_type(*symbol)?;
        if !self.class(ty)
            || !matches!(self.kind(ty), Some(UnitTypeKind::Nominal { arguments, .. }) if arguments.is_empty())
            || self.copyability_of(ty) != Copyability::MoveOnly
            || inputs.typed.is_resource_type(ty) != Some(false)
        {
            return None;
        }
        let RefineBody::Block(body) = inputs.body else {
            return None;
        };
        let Statement::Block { elements } = ast.statements().get(body).ok()?.payload() else {
            return None;
        };
        if !(2..=3).contains(&elements.len()) {
            return None;
        }
        let caller = inputs.source_plan.source(inputs.source_token)?;
        let receivers = inputs.single_receiver_facts?;
        let sequence_facts = inputs.owned_receiver_sequence_facts?;
        let mut previous = None;
        let mut checks = [None; 3];
        for (slot, statement) in checks.iter_mut().zip(elements) {
            let Statement::Expression { expression } =
                ast.statements().get(*statement).ok()?.payload()
            else {
                return None;
            };
            let node = ast.expressions().get(*expression).ok()?;
            let Expression::Call {
                callee, arguments, ..
            } = node.payload()
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
            // Strip only transparent groups to resolve the place. The outer
            // occurrence still owns the typed receiver and loan-begin span.
            let receiver_node = ast.expressions().get(*root).ok()?;
            let mut binding = *root;
            while let Expression::Group { expression } =
                ast.expressions().get(binding).ok()?.payload()
            {
                binding = *expression;
            }
            let id = self.id(*expression);
            let root_id = self.id(*root);
            let call = inputs.typed.call(id)?;
            let receiver = call.receiver()?;
            let UnitCallTarget::Symbol(member) = call.target() else {
                return None;
            };
            if !arguments.is_empty()
                || !matches!(
                    ast.expressions().get(binding).ok()?.payload(),
                    Expression::Name
                )
                || self.reference(binding) != Some(*symbol)
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
            let dispatch = site.dispatch_key()?;
            if dispatch == caller
                || dispatch.target() != UnitCallableTarget::Symbol(member)
                || !site.delegation().is_empty()
                || !inputs.function_ids.contains_key(dispatch)
            {
                return None;
            }
            // The loans share a root, not a callee. Each call proved its own direct dispatch.
            if previous.is_some_and(|(earlier, end)| earlier == id || end > node.span().start()) {
                return None;
            }

            let fact = receivers.receiver(inputs.owned, id)?;
            let UnitReceiverOwnershipTarget::Place(place) = fact.target() else {
                return None;
            };
            if fact.call() != id
                || fact.kind() != UnitReceiverOwnershipKind::SharedLoan
                || fact.source() != receiver.origin()
                || fact.receiver_type() != receiver.ty()
                || fact.is_receiver_reservation()
                || fact.activation_point().is_some()
                || !place.is_root()
                || place.root() != *symbol
                || fact.begin_span() != receiver_node.span()
                || fact.end_span() != node.span()
            {
                return None;
            }

            *slot = Some((id, fact));
            previous = Some((id, node.span().end()));
        }
        sequence_facts
            .validates(inputs.owned, *symbol, &checks[..elements.len()])
            .then_some(())
    }
}
