//! 保持分支 tail 交付与词法 scope cleanup 的顺序。

use super::{DropPlanner, DropPoint, ExpressionUse, OwnershipCheckingError, ValueState};
use crate::{ast::StatementId, parser::Statement};

impl DropPlanner<'_, '_> {
    pub(super) fn control_body(
        &mut self,
        id: StatementId,
        usage: ExpressionUse,
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
            Statement::ControlBody { elements } | Statement::LambdaBody { elements } => elements,
            Statement::Expression { expression } => {
                return if usage == ExpressionUse::Read {
                    self.statement(id, state)
                } else {
                    self.expression(expression, usage, state)
                };
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
        let continues = self.control_body(tail, usage, state)?;
        if continues {
            self.drop_scope(frame, DropPoint::AfterStatement(id), state);
        }
        self.scope_depth -= 1;
        Ok(continues)
    }
}
