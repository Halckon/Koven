//! 局部变量建立：owned 义务与显式 shared 结果生命周期。
use super::*;

impl DropPlanner<'_, '_> {
    pub(super) fn local_variable(
        &mut self,
        id: StatementId,
        declaration: ItemId,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let Item::Variable {
            kind,
            name,
            initializer,
            ..
        } = self
            .checker
            .parsed
            .ast()
            .items()
            .get(declaration)?
            .payload()
            .clone()
        else {
            return Ok(true);
        };
        if matches!(kind, crate::parser::VariableKind::BorrowVal(_)) {
            if let Some(binding) = self.checker.marker_symbol(name).copied() {
                self.binding_depths.insert(binding, self.scope_depth);
                state.pending_borrow_results.push(binding);
            }
            let continues = self.expression(initializer, DropExpressionUse::Read, state)?;
            if let Some(binding) = self.checker.marker_symbol(name).copied() {
                state
                    .pending_borrow_results
                    .retain(|pending| *pending != binding);
                if continues {
                    state.borrow_bindings.push(binding);
                }
            }
            if !continues {
                return Ok(false);
            }
            return Ok(true);
        }
        let closure = self.closure_origin(initializer, state)?;
        if !self.expression(initializer, DropExpressionUse::Consume, state)? {
            return Ok(false);
        }
        if let Some(symbol) = self.checker.marker_symbol(name).copied()
            && self.checker.is_move_only_variable(symbol)
        {
            self.binding_depths.insert(symbol, self.scope_depth);
            state.insert(OwnedValue {
                symbol,
                origin: marker_span(name),
                declaration: marker_span(name),
                scope_depth: self.scope_depth,
            });
            if let Some(closure) = closure {
                state.closures.insert(symbol, closure);
            }
            if !self.liveness.statement_after[id.index()].contains(&symbol) {
                self.drop_named_asap(PlannerDropPoint::AfterStatement(id), symbol, state);
            }
        }
        Ok(true)
    }
}
