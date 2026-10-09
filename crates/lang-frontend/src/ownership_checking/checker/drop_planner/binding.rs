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
            if let Some(binding) = self.checker.marker_symbol(name) {
                self.binding_depths.insert(binding, self.scope_depth);
                state.pending_borrow_results.push(binding);
            }
            let continues = self.expression(initializer, ExpressionUse::Read, state)?;
            if let Some(binding) = self.checker.marker_symbol(name) {
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
            state.result_owners.clear();
            state.result_closures.clear();
            return Ok(true);
        }
        if !self.expression(initializer, ExpressionUse::Consume, state)? {
            return Ok(false);
        }
        let snapshot = self.save_result_snapshot(initializer, state)?;
        let closures = std::mem::take(&mut state.result_closures);
        if let Some(symbol) = self.checker.marker_symbol(name)
            && self.checker.is_move_only_variable(symbol)
        {
            self.binding_depths.insert(symbol, self.scope_depth);
            let versions = self.bind_result_owners(marker_span(name), state);
            state.insert(OwnedValue {
                versions,
                condition: state.path,
                symbol,
                origin: marker_span(name),
                declaration: marker_span(name),
                scope_depth: self.scope_depth,
            });
            if !closures.is_empty() {
                state.closures.insert(symbol, closures);
            }
            self.commit_snapshot(snapshot, initializer, symbol);
            if !self.liveness.statement_after[id.index()].contains(&symbol) {
                self.drop_named_asap(DropPoint::AfterStatement(id), symbol, state);
            }
        }
        Ok(true)
    }
}
