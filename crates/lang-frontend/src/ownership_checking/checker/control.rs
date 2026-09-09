//! 控制表达式先交付分支结果；外围借用不改变分支 tail 的 owner 契约。

use super::{Checker, ExpressionUse, Flows, OwnershipCheckingError, State};
use crate::{
    ast::{ExpressionId, StatementId},
    diagnostic::{Diagnostic, Severity},
    parser::Statement,
    type_checking::{BuiltinType, TypeKind},
};

impl Checker<'_> {
    // Only repeating edges require the owner again; exits keep their ordinary flow checks.
    pub(super) fn check_loop_backedge(
        &mut self,
        body: StatementId,
        flows: &Flows,
    ) -> Result<(), OwnershipCheckingError> {
        let live = &self.statement_live_after[body.index()];
        let mut moved = std::collections::BTreeMap::new();
        for state in [&flows.next, &flows.continues].into_iter().flatten() {
            for (&symbol, &origin) in &state.moved {
                if live.contains(&symbol) {
                    moved.entry(symbol).or_insert(origin);
                }
            }
        }
        for origin in moved.into_values() {
            let mut diagnostic = Diagnostic::new(
                self.sources,
                Severity::Error,
                self.use_after_move_code,
                "moved value may be used again on the next loop iteration",
                origin,
            )?;
            diagnostic.add_label(self.sources, origin, "value was moved here")?;
            self.diagnostics.push(diagnostic);
        }
        Ok(())
    }

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
