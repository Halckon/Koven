//! 控制表达式先交付分支结果；外围借用不改变分支 tail 的 owner 契约。

use super::{Checker, ExpressionUse, Flows, OwnershipCheckingError, State};
use crate::{
    ast::{ExpressionId, StatementId},
    parser::Statement,
    type_checking::{BuiltinType, TypeKind},
};

impl Checker<'_> {
    pub(super) fn control_result_usage(&self, expression: ExpressionId) -> ExpressionUse {
        if self
            .typed
            .expression_type(expression)
            .and_then(|ty| self.typed.types().get(ty))
            != Some(&TypeKind::Builtin(BuiltinType::Unit))
        {
            ExpressionUse::Consume
        } else {
            ExpressionUse::Read
        }
    }

    pub(super) fn check_control_body(
        &mut self,
        statement: StatementId,
        state: State,
        usage: ExpressionUse,
    ) -> Result<Flows, OwnershipCheckingError> {
        let payload = self
            .parsed
            .ast()
            .statements()
            .get(statement)?
            .payload()
            .clone();
        let elements = match payload {
            Statement::ControlBody { elements } => elements,
            Statement::Expression { expression } => {
                return self.check_expression(expression, state, usage);
            }
            _ => return self.check_statement(statement, state),
        };
        let Some((&tail, prefix)) = elements.split_last() else {
            return Ok(Flows::next(state));
        };
        let mut flows = self.check_elements(prefix, state)?;
        let Some(next) = flows.next.take() else {
            return Ok(flows);
        };
        let tail = match self.parsed.ast().statements().get(tail)?.payload().clone() {
            Statement::Expression { expression } => {
                self.check_expression(expression, next, usage)?
            }
            _ => self.check_statement(tail, next)?,
        };
        flows.merge(tail);
        Ok(flows)
    }
}
