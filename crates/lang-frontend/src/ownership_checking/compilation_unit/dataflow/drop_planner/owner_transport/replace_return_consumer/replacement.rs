//! Actual suffix replacement callbacks preserve the old selection while R retires.
use super::*;
use crate::ownership_checking::{UnitDropPoint, UnitOwnerProducerCatalogV1};

pub(super) fn admit(
    checker: &Checker<'_>,
    call: ExpressionId,
    root: UnitSymbolId,
) -> Option<UnitExpressionId> {
    let ast = checker.parsed.ast();
    let Expression::Call {
        callee,
        type_arguments,
        arguments,
        ..
    } = ast.expressions().get(call).ok()?.payload()
    else {
        return None;
    };
    let [place, value] = arguments.as_slice() else {
        return None;
    };
    if !type_arguments.is_empty()
        || !matches!(
            ast.expressions().get(*callee).ok()?.payload(),
            Expression::Name
        )
        || place.named_prefix.is_some()
        || value.named_prefix.is_some()
        || value.mode_marker.is_some()
        || !matches!(
            place.mode_marker,
            Some(crate::parser::ParameterModeMarker::Inout(_))
        )
    {
        return None;
    }
    let node = ast.expressions().get(place.value).ok()?;
    if !matches!(node.payload(), Expression::Name)
        || checker.reference_symbol(node.span()) != Some(root)
        || !matches!(ast.expressions().get(value.value).ok()?.payload(), Expression::String { parts } if parts.iter().all(|p| !matches!(p, StringPart::Interpolation { .. })))
    {
        return None;
    }
    let plan = checker
        .ownership_primitives
        .iter()
        .find(|p| p.descriptor().expression() == checker.unit_expression(call))?;
    if plan.descriptor().kind() != OwnershipPrimitiveKind::Replace
        || plan.places().len() != 1
        || !plan.places()[0].is_root()
        || plan.places()[0].root() != root
        || plan.descriptor().operands().len() != 2
        || plan.descriptor().operands()[1] != checker.unit_expression(value.value)
        || checker.typed.types().get(plan.descriptor().value_type())
            != Some(&UnitTypeKind::Builtin(BuiltinType::String))
    {
        return None;
    }
    Some(checker.unit_expression(value.value))
}

#[derive(Clone, Debug)]
pub(super) struct Frame {
    pub(super) catalog: UnitOwnerProducerCatalogV1,
    committed: Option<OwnedValue>,
    stage: u8,
}
impl Frame {
    pub(super) fn new(callable: UnitOwnerCallable) -> Self {
        Self {
            catalog: UnitOwnerProducerCatalogV1 {
                identity: std::sync::Arc::new(()),
                callable,
                definitions: Vec::new(),
            },
            committed: None,
            stage: 0,
        }
    }
    pub(super) fn completed_actions(
        &self,
        call: UnitExpressionId,
        argument: UnitExpressionId,
        root: UnitSymbolId,
    ) -> Option<Vec<Action>> {
        let [definition] = self.catalog.definitions() else {
            return None;
        };
        if self.stage != 5
            || self.committed.is_none()
            || definition.origin() != UnitOwnerOrigin::String(argument)
        {
            return None;
        }
        let owner = definition.id();
        Some(vec![
            Action::Create(argument, owner.clone()),
            Action::Pending(call, argument, owner.clone()),
            Action::Replace(call, root, owner.clone()),
            Action::Drop(UnitDropPoint::CallReturn(call), root, owner),
            Action::Result(call),
        ])
    }
}
impl DropPlanner<'_, '_> {
    pub(in super::super::super) fn replace_return_string(
        &mut self,
        expression: ExpressionId,
        state: &mut ValueState,
    ) {
        let identity = self.checker.unit_expression(expression);
        if !matches!(
            &state.replace_consumer,
            Some(ReplaceConsumerCarrier::Return(_))
        ) {
            return;
        }
        let Some(ReplaceConsumerCarrier::Return(mut carrier)) = state.replace_consumer.take()
        else {
            return;
        };
        if carrier.target.replacement != Some(identity) {
            state.replace_consumer = Some(ReplaceConsumerCarrier::Return(carrier));
            return;
        }
        let Some(frame) = &mut carrier.replacement else {
            return;
        };
        if frame.stage != 0
            || !carrier.actions.is_empty()
            || state
                .position(carrier.incoming[0].symbol)
                .map(|i| &state.values[i])
                != Some(&carrier.incoming[0])
        {
            return;
        }
        let owner = UnitOwnerId {
            identity: frame.catalog.identity.clone(),
            callable: frame.catalog.callable,
            index: 0,
        };
        frame.catalog.definitions.push(UnitOwnerDefinition {
            id: owner.clone(),
            origin: UnitOwnerOrigin::String(identity),
        });
        carrier.actions.push(Action::Create(identity, owner));
        frame.stage = 1;
        state.replace_consumer = Some(ReplaceConsumerCarrier::Return(carrier));
    }
    pub(in super::super::super) fn pending_replace_return(
        &mut self,
        call: ExpressionId,
        argument: ExpressionId,
        state: &mut ValueState,
    ) {
        let registered = self.pending_value_registered(call, argument, state);
        let identity = self.checker.unit_expression(call);
        let argument = self.checker.unit_expression(argument);
        if !matches!(
            &state.replace_consumer,
            Some(ReplaceConsumerCarrier::Return(_))
        ) {
            return;
        }
        let Some(ReplaceConsumerCarrier::Return(mut carrier)) = state.replace_consumer.take()
        else {
            return;
        };
        if carrier.target.replacement.is_none() || carrier.target.payload != identity {
            state.replace_consumer = Some(ReplaceConsumerCarrier::Return(carrier));
            return;
        }
        let Some(frame) = &mut carrier.replacement else {
            return;
        };
        if frame.stage != 1
            || carrier.target.replacement != Some(argument)
            || !registered
            || state
                .position(carrier.incoming[0].symbol)
                .map(|i| &state.values[i])
                != Some(&carrier.incoming[0])
        {
            return;
        }
        carrier.actions.push(Action::Pending(
            identity,
            argument,
            frame.catalog.definitions[0].id(),
        ));
        frame.stage = 2;
        state.replace_consumer = Some(ReplaceConsumerCarrier::Return(carrier));
    }
    pub(in super::super::super) fn commit_replace_return(
        &mut self,
        call: ExpressionId,
        state: &mut ValueState,
    ) {
        let identity = self.checker.unit_expression(call);
        if !matches!(
            &state.replace_consumer,
            Some(ReplaceConsumerCarrier::Return(_))
        ) {
            return;
        }
        let Some(ReplaceConsumerCarrier::Return(mut carrier)) = state.replace_consumer.take()
        else {
            return;
        };
        if carrier.target.replacement.is_none() || carrier.target.payload != identity {
            state.replace_consumer = Some(ReplaceConsumerCarrier::Return(carrier));
            return;
        }
        let Some(frame) = &mut carrier.replacement else {
            return;
        };
        if frame.stage != 2
            || admit(self.checker, call, carrier.incoming[0].symbol) != carrier.target.replacement
        {
            return;
        }
        let Some(index) = state.position(carrier.incoming[0].symbol) else {
            return;
        };
        let mut expected = carrier.incoming[0].clone();
        let Ok(node) = self.checker.parsed.ast().expressions().get(call) else {
            return;
        };
        expected.origin = node.span();
        // Legacy scalar P remains a projection, never R's identity or the detached result.
        if state.values[index] != expected {
            return;
        }
        frame.committed = Some(state.values[index].clone());
        carrier.actions.push(Action::Replace(
            identity,
            expected.symbol,
            frame.catalog.definitions[0].id(),
        ));
        frame.stage = 3;
        state.replace_consumer = Some(ReplaceConsumerCarrier::Return(carrier));
    }
    pub(in super::super::super) fn drop_replace_return(
        &mut self,
        point: PlannerDropPoint,
        actual: &OwnedValue,
        state: &mut ValueState,
    ) -> bool {
        let is_replacement = matches!(&state.replace_consumer, Some(ReplaceConsumerCarrier::Return(c)) if c.replacement.is_some());
        if !is_replacement {
            return false;
        }
        let Some(ReplaceConsumerCarrier::Return(mut carrier)) = state.replace_consumer.take()
        else {
            return true;
        };
        let Some(frame) = &mut carrier.replacement else {
            return true;
        };
        let point = point.into_unit(self.checker.source_unit);
        if frame.stage != 3
            || frame.committed.as_ref() != Some(actual)
            || state.position(actual.symbol).is_some()
            || point != UnitDropPoint::CallReturn(carrier.target.payload)
        {
            return true;
        }
        carrier.actions.push(Action::Drop(
            point,
            actual.symbol,
            frame.catalog.definitions[0].id(),
        ));
        frame.stage = 4;
        state.replace_consumer = Some(ReplaceConsumerCarrier::Return(carrier));
        true
    }
    pub(in super::super::super) fn finish_replace_return_call(
        &mut self,
        call: ExpressionId,
        state: &mut ValueState,
    ) -> Option<ReplaceConsumerCarrier> {
        let identity = self.checker.unit_expression(call);
        let is_target = matches!(&state.replace_consumer, Some(ReplaceConsumerCarrier::Return(c)) if c.replacement.is_some() && c.target.payload == identity);
        if !is_target {
            return None;
        }
        let ReplaceConsumerCarrier::Return(mut carrier) = state.replace_consumer.take()? else {
            return None;
        };
        let frame = carrier.replacement.as_mut()?;
        if frame.stage != 4 || state.position(carrier.incoming[0].symbol).is_some() {
            return None;
        }
        carrier.actions.push(Action::Result(identity));
        frame.stage = 5;
        Some(ReplaceConsumerCarrier::Return(carrier))
    }
}
