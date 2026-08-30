//! Lambda callable body 的独立 liveness 消费与析构规划。

use crate::{
    ast::{ExpressionId, StatementId},
    parser::Statement,
    source::Span,
};

use super::{DropExpressionUse, DropPlanner, OwnedValue, PlannerDropPoint, ValueState};
use crate::ownership_checking::OwnershipCheckingError;

impl DropPlanner<'_, '_> {
    pub(super) fn plan_lambda_body(
        &mut self,
        lambda: ExpressionId,
        parameters: &[Span],
        body: StatementId,
    ) -> Result<(), OwnershipCheckingError> {
        self.plan_lambda_body_inner(lambda, parameters, body)
    }

    fn plan_lambda_body_inner(
        &mut self,
        lambda: ExpressionId,
        parameters: &[Span],
        body: StatementId,
    ) -> Result<(), OwnershipCheckingError> {
        let Statement::LambdaBody { elements } = self
            .checker
            .parsed
            .ast()
            .statements()
            .get(body)?
            .payload()
            .clone()
        else {
            return Ok(());
        };
        self.scope_depth += 1;
        let frame = self.scope_depth;
        let mut state = ValueState::default();
        for &parameter in parameters {
            if let Some(symbol) = self
                .checker
                .symbols_by_span
                .get(&super::super::span_key(parameter))
                .copied()
                && self.checker.is_move_only_variable(symbol)
            {
                self.binding_depths.insert(symbol, frame);
                state.insert(OwnedValue {
                    symbol,
                    origin: parameter,
                    scope_depth: frame,
                });
            }
        }
        let live_in = self
            .liveness
            .lambda_live_in
            .get(&lambda.index())
            .cloned()
            .unwrap_or_default();
        let unused_parameters = state
            .values
            .iter()
            .filter(|value| !live_in.contains(&value.symbol))
            .map(|value| value.symbol)
            .collect::<Vec<_>>();
        for symbol in unused_parameters.into_iter().rev() {
            self.drop_named(PlannerDropPoint::LambdaEntry(lambda), symbol, &mut state);
        }
        let Some((&tail, prefix)) = elements.split_last() else {
            self.scope_depth -= 1;
            return Ok(());
        };
        for &element in prefix {
            if !self.statement(element, &mut state)? {
                self.scope_depth -= 1;
                return Ok(());
            }
        }
        let tail_node = self.checker.parsed.ast().statements().get(tail)?;
        let continues = match tail_node.payload() {
            Statement::Expression { expression } => {
                self.expression(*expression, DropExpressionUse::Consume, &mut state)?
            }
            _ => self.statement(tail, &mut state)?,
        };
        if continues {
            self.drop_scope(frame, PlannerDropPoint::AfterStatement(body), &mut state);
        }
        self.scope_depth -= 1;
        Ok(())
    }
}
