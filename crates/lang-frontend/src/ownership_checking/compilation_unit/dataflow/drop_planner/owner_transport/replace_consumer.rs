//! Strict ADR-0038 B carrier. The saved Break state moves into the actual suffix.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitOwnerReplaceBreakConsumerActionV1 as Action,
    UnitOwnerReplaceBreakConsumerV1, UnitOwnerReplaceBreakObservationV1,
    UnitOwnerReplaceBreakStateV1,
};

#[derive(Clone, Debug)]
pub(in super::super) struct BindConsumerTarget {
    pub(in super::super) initializer: UnitExpressionId,
    pub(in super::super) statement: UnitStatementId,
    pub(in super::super) retired: UnitSymbolId,
}

/// State-owned path alternatives, never projected to a new producer token.
#[derive(Clone, Debug)]
pub(in super::super) struct BindConsumerCarrier {
    incoming: [OwnedValue; 2],
    target: BindConsumerTarget,
    actions: Vec<Action>,
    bound: Option<OwnedValue>,
}
pub(super) fn snapshot(value: &OwnedValue) -> Option<UnitOwnerReplaceBreakStateV1> {
    Some(UnitOwnerReplaceBreakStateV1 {
        symbol: value.symbol,
        owner: value.owner.clone()?,
        origin: value.origin,
        declaration: value.declaration,
        scope_depth: value.scope_depth,
    })
}
impl BindConsumerCarrier {
    pub(in super::super) fn handoff(
        condition: &OwnedValue,
        body: OwnedValue,
        target: BindConsumerTarget,
        boundary: &UnitOwnerReplaceBreakObservationV1,
    ) -> Option<Self> {
        if snapshot(condition).as_ref() != Some(boundary.condition_false_exit())
            || snapshot(&body).as_ref() != Some(boundary.body_completion())
            || !boundary.is_consistent()
        {
            return None;
        }
        Some(Self {
            // The condition value is retained only as the legacy planner projection.
            // The path carrier owns its identity reference and the moved actual Q state.
            incoming: [condition.clone(), body],
            target,
            actions: Vec::new(),
            bound: None,
        })
    }
    pub(in super::super) fn take(
        mut self,
        expression: UnitExpressionId,
        actual: Option<&OwnedValue>,
    ) -> Option<Self> {
        if expression != self.target.initializer
            || actual != Some(&self.incoming[0])
            || !self.actions.is_empty()
            || self.bound.is_some()
        {
            return None;
        }
        self.actions
            .push(Action::Take(expression, self.incoming[0].symbol));
        Some(self)
    }
    pub(in super::super) fn bind(
        mut self,
        expression: UnitExpressionId,
        actual: &OwnedValue,
    ) -> Option<Self> {
        if expression != self.target.initializer
            || actual.symbol != self.target.retired
            || self.actions != [Action::Take(expression, self.incoming[0].symbol)]
            || self.bound.is_some()
        {
            return None;
        }
        self.actions.push(Action::Bind(expression, actual.symbol));
        self.bound = Some(actual.clone());
        Some(self)
    }
    fn drop(
        mut self,
        point: UnitDropPoint,
        actual: &OwnedValue,
    ) -> Option<UnitOwnerReplaceBreakConsumerV1> {
        if point != UnitDropPoint::AfterStatement(self.target.statement)
            || self.bound.as_ref() != Some(actual)
            || self.actions
                != [
                    Action::Take(self.target.initializer, self.incoming[0].symbol),
                    Action::Bind(self.target.initializer, self.target.retired),
                ]
        {
            return None;
        }
        self.actions.push(Action::Drop(point, actual.symbol));
        Some(UnitOwnerReplaceBreakConsumerV1 {
            incoming: [snapshot(&self.incoming[0])?, snapshot(&self.incoming[1])?],
            initializer: self.target.initializer,
            statement: self.target.statement,
            retired: self.target.retired,
            actions: self.actions,
        })
    }
}
/// Distinct capabilities share a planner slot, never each other's action protocol.
#[derive(Clone, Debug)]
pub(in super::super) enum ReplaceConsumerCarrier {
    Bind(BindConsumerCarrier),
    Call(super::replace_call_consumer::CallConsumerCarrier),
}
impl ReplaceConsumerCarrier {
    pub(in super::super) fn handoff(
        condition: &OwnedValue,
        body: OwnedValue,
        target: crate::ownership_checking::compilation_unit::owner_transport::ReplaceBreakSuffix,
        boundary: &UnitOwnerReplaceBreakObservationV1,
    ) -> Option<Self> {
        use crate::ownership_checking::compilation_unit::owner_transport::ReplaceBreakSuffix as S;
        match target {
            S::Bind {
                initializer,
                statement,
                retired,
            } => BindConsumerCarrier::handoff(
                condition,
                body,
                BindConsumerTarget {
                    initializer,
                    statement,
                    retired,
                },
                boundary,
            )
            .map(Self::Bind),
            S::Call(target) => super::replace_call_consumer::CallConsumerCarrier::handoff(
                condition, body, target, boundary,
            )
            .map(Self::Call),
        }
    }
    pub(in super::super) fn take(
        self,
        expression: UnitExpressionId,
        actual: Option<&OwnedValue>,
    ) -> Option<Self> {
        match self {
            Self::Bind(value) => value.take(expression, actual).map(Self::Bind),
            Self::Call(value) => value.take(expression, actual).map(Self::Call),
        }
    }
    fn bind(self, expression: UnitExpressionId, actual: &OwnedValue) -> Option<Self> {
        match self {
            Self::Bind(value) => value.bind(expression, actual).map(Self::Bind),
            Self::Call(_) => None,
        }
    }
    fn drop(
        self,
        point: UnitDropPoint,
        actual: &OwnedValue,
    ) -> Option<UnitOwnerReplaceBreakConsumerV1> {
        match self {
            Self::Bind(value) => value.drop(point, actual),
            Self::Call(_) => None,
        }
    }
}
impl DropPlanner<'_, '_> {
    pub(in super::super) fn bind_replace_consumer(
        &mut self,
        carrier: Option<ReplaceConsumerCarrier>,
        initializer: ExpressionId,
        symbol: UnitSymbolId,
        state: &mut ValueState,
    ) {
        if let Some(carrier) = carrier
            && let Some(index) = state.position(symbol)
        {
            state.replace_consumer = carrier.bind(
                self.checker.unit_expression(initializer),
                &state.values[index],
            );
        }
    }
    pub(in super::super) fn drop_replace_consumer(
        &mut self,
        point: PlannerDropPoint,
        value: &OwnedValue,
        state: &mut ValueState,
    ) {
        if let Some(carrier) = state.replace_consumer.take()
            && let Some(consumer) = carrier.drop(point.into_unit(self.checker.source_unit), value)
            && let Some(transaction) = &mut self.owner_loop
        {
            transaction.complete_replace_consumer(consumer);
        }
    }
}
