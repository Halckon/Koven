//! Move the saved path alternatives through the actual Name and Return traversal.
mod replacement;
use super::replace_consumer::{ReplaceConsumerCarrier, snapshot};
use super::*;
use crate::ownership_checking::compilation_unit::owner_transport::ReturnConsumerTarget;
use crate::ownership_checking::{
    UnitOwnerReplaceBreakObservationV1, UnitOwnerReplaceBreakReturnActionV1 as Action,
    UnitOwnerReplaceBreakReturnConsumerV1,
};

pub(super) fn admit(
    checker: &Checker<'_>,
    statement: StatementId,
    root: UnitSymbolId,
) -> Option<ReturnConsumerTarget> {
    let ast = checker.parsed.ast();
    let Statement::Expression { expression } = ast.statements().get(statement).ok()?.payload()
    else {
        return None;
    };
    let Expression::Return {
        value: Some(payload),
        ..
    } = ast.expressions().get(*expression).ok()?.payload()
    else {
        return None;
    };
    let payload_node = ast.expressions().get(*payload).ok()?;
    let replacement = replacement::admit(checker, *payload, root);
    let named = matches!(payload_node.payload(), Expression::Name)
        && checker.reference_symbol(payload_node.span()) == Some(root)
        && checker
            .typed
            .expression_category(checker.unit_expression(*payload))
            == Some(ExpressionCategory::Place);
    if (!named && replacement.is_none())
        || checker
            .typed
            .expression_type(checker.unit_expression(*payload))
            .and_then(|ty| checker.typed.types().get(ty))
            != Some(&UnitTypeKind::Builtin(BuiltinType::String))
        || checker
            .typed
            .expression_type(checker.unit_expression(*expression))
            .and_then(|ty| checker.typed.types().get(ty))
            != Some(&UnitTypeKind::Builtin(BuiltinType::Nothing))
    {
        return None;
    }
    Some(ReturnConsumerTarget {
        expression: checker.unit_expression(*expression),
        payload: checker.unit_expression(*payload),
        statement: UnitStatementId::new(checker.source_unit, statement),
        replacement,
        binding: None,
    })
}

#[derive(Clone, Debug)]
pub(in super::super) struct ReturnConsumerCarrier {
    incoming: [OwnedValue; 2],
    target: ReturnConsumerTarget,
    actions: Vec<Action>,
    bound: Option<OwnedValue>,
    replacement: Option<Box<replacement::Frame>>,
}
impl ReturnConsumerCarrier {
    pub(super) fn handoff(
        condition: &OwnedValue,
        body: OwnedValue,
        target: ReturnConsumerTarget,
        boundary: &UnitOwnerReplaceBreakObservationV1,
    ) -> Option<Self> {
        if snapshot(condition).as_ref() != Some(boundary.condition_false_exit())
            || snapshot(&body).as_ref() != Some(boundary.body_completion())
            || !boundary.is_consistent()
        {
            return None;
        }
        Some(Self {
            incoming: [condition.clone(), body],
            replacement: target.replacement.map(|_| {
                Box::new(replacement::Frame::new(
                    boundary.input_producers().callable(),
                ))
            }),
            target,
            actions: Vec::new(),
            bound: None,
        })
    }
    pub(super) fn take(
        mut self,
        expression: UnitExpressionId,
        actual: Option<&OwnedValue>,
    ) -> Option<Self> {
        if let Some((initializer, _, symbol)) = self.target.binding
            && expression == self.target.payload
        {
            if self.bound.as_ref() != actual
                || actual.is_none()
                || self.actions
                    != [
                        Action::Take(initializer, self.incoming[0].symbol),
                        Action::Bind(initializer, symbol),
                    ]
            {
                return None;
            }
            self.actions.push(Action::Take(expression, symbol));
        } else {
            let initial = self.target.binding.map_or(self.target.payload, |b| b.0);
            if expression != initial
                || actual != Some(&self.incoming[0])
                || !self.actions.is_empty()
                || self.bound.is_some()
            {
                return None;
            }
            self.actions
                .push(Action::Take(expression, self.incoming[0].symbol));
        }
        Some(self)
    }
    pub(super) fn bind(
        mut self,
        expression: UnitExpressionId,
        actual: &OwnedValue,
    ) -> Option<Self> {
        let (initializer, _, symbol) = self.target.binding?;
        if expression != initializer
            || actual.symbol != symbol
            || self.bound.is_some()
            || self.actions != [Action::Take(initializer, self.incoming[0].symbol)]
        {
            return None;
        }
        self.actions.push(Action::Bind(initializer, symbol));
        self.bound = Some(actual.clone());
        Some(self)
    }
    fn deliver(
        mut self,
        expression: UnitExpressionId,
        payload: UnitExpressionId,
        owner: Option<&UnitOwnerId>,
        state: &ValueState,
    ) -> Option<UnitOwnerReplaceBreakReturnConsumerV1> {
        let expected = if let Some(frame) = &self.replacement {
            frame.completed_actions(
                self.target.payload,
                self.target.replacement?,
                self.incoming[0].symbol,
            )?
        } else if let Some((initializer, _, symbol)) = self.target.binding {
            if self.bound.is_none() || state.position(symbol).is_some() {
                return None;
            }
            vec![
                Action::Take(initializer, self.incoming[0].symbol),
                Action::Bind(initializer, symbol),
                Action::Take(payload, symbol),
            ]
        } else {
            vec![Action::Take(payload, self.incoming[0].symbol)]
        };
        if expression != self.target.expression || payload != self.target.payload
            || self.actions != expected
            // Unsupported legacy observation clears its scalar result, never the path carrier.
            || owner.is_some() || state.position(self.incoming[0].symbol).is_some()
            || state.replace_consumer.is_some()
        {
            return None;
        }
        self.actions.push(Action::Return {
            expression,
            payload,
        });
        Some(UnitOwnerReplaceBreakReturnConsumerV1 {
            incoming: [snapshot(&self.incoming[0])?, snapshot(&self.incoming[1])?],
            target: self.target,
            replacement_producers: self.replacement.map(|frame| frame.catalog),
            actions: self.actions,
        })
    }
}
impl DropPlanner<'_, '_> {
    pub(in super::super) fn deliver_replace_return_consumer(
        &self,
        expression: ExpressionId,
        payload: ExpressionId,
        result: EvaluatedOwner,
        state: &ValueState,
    ) -> Option<UnitOwnerReplaceBreakReturnConsumerV1> {
        let Some(ReplaceConsumerCarrier::Return(carrier)) = result.replace_consumer else {
            return None;
        };
        carrier.deliver(
            self.checker.unit_expression(expression),
            self.checker.unit_expression(payload),
            result.owner.as_ref(),
            state,
        )
    }
}
