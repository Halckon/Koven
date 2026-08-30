//! Lambda callable body 的独立 liveness 消费与析构规划。

use crate::{ast::StatementId, parser::Statement, source::Span};

use super::{DropExpressionUse, DropPlanner, PlannerDropPoint, ValueState};
use crate::ownership_checking::OwnershipCheckingError;

impl DropPlanner<'_, '_> {
    pub(super) fn plan_lambda_body(
        &mut self,
        body: StatementId,
    ) -> Result<(), OwnershipCheckingError> {
        self.plan_lambda_body_inner(body)
    }

    fn plan_lambda_body_inner(&mut self, body: StatementId) -> Result<(), OwnershipCheckingError> {
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

    pub(super) fn is_move_only_lambda_parameter(&self, span: Span) -> bool {
        self.checker
            .symbols_by_span
            .get(&super::super::span_key(span))
            .is_some_and(|symbol| self.checker.is_move_only_variable(*symbol))
    }
}
