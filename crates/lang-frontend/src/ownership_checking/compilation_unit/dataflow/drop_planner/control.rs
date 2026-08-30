//! Control expression 分支 tail 的 usage 与析构规划。

use crate::{
    ast::{ExpressionId, StatementId},
    ownership_checking::OwnershipCheckingError,
    parser::Statement,
};

use super::{
    DropExpressionUse, DropPlanner, PlannerDropFact, PlannerDropPoint, PlannerDropTarget,
    ValueState,
};

impl DropPlanner<'_, '_> {
    /// 让 control result 的内部 owner 交付只作用于分支 body 的最后一个表达式。
    pub(super) fn control_body(
        &mut self,
        id: StatementId,
        usage: DropExpressionUse,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let payload = self
            .checker
            .parsed
            .ast()
            .statements()
            .get(id)?
            .payload()
            .clone();
        let elements = match payload {
            Statement::ControlBody { elements } => elements,
            Statement::Expression { expression } => {
                return self.control_tail(expression, usage, state);
            }
            _ => return self.statement(id, state),
        };
        self.scope_depth += 1;
        let frame = self.scope_depth;
        let Some((&tail, prefix)) = elements.split_last() else {
            self.scope_depth -= 1;
            return Ok(true);
        };
        for &element in prefix {
            if !self.statement(element, state)? {
                self.scope_depth -= 1;
                return Ok(false);
            }
        }
        let tail_node = self.checker.parsed.ast().statements().get(tail)?;
        let continues = match tail_node.payload() {
            Statement::Expression { expression } => self.control_tail(*expression, usage, state)?,
            _ => self.statement(tail, state)?,
        };
        if continues {
            self.drop_scope(frame, PlannerDropPoint::AfterStatement(id), state);
        }
        self.scope_depth -= 1;
        Ok(continues)
    }

    fn control_tail(
        &mut self,
        expression: ExpressionId,
        usage: DropExpressionUse,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let continues = self.expression(expression, usage, state)?;
        if continues && usage == DropExpressionUse::Read && self.is_move_only_temporary(expression)
        {
            let span = self
                .checker
                .parsed
                .ast()
                .expressions()
                .get(expression)?
                .span();
            self.push_fact(PlannerDropFact::new(
                PlannerDropPoint::AfterExpression(expression),
                PlannerDropTarget::Temporary(expression),
                span,
            ));
        }
        Ok(continues)
    }

    pub(super) fn control_result_usage(&self, expression: ExpressionId) -> DropExpressionUse {
        if self.is_move_only_temporary(expression) {
            DropExpressionUse::Consume
        } else {
            DropExpressionUse::Read
        }
    }
}
