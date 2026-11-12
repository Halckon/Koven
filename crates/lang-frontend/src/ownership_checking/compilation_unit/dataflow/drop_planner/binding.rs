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
            self.reject_owner_expression(
                initializer,
                crate::ownership_checking::UnitOwnerUnsupportedReason::Syntax,
            );
            if let Some(binding) = self.checker.marker_symbol(name).copied() {
                self.binding_depths.insert(binding, self.scope_depth);
                state.pending_borrow_results.push(binding);
            }
            let observation_depth = state.borrow_initializer_sources.len();
            if let Some(source) = self.borrow_initializer_candidate(initializer) {
                state.borrow_initializer_sources.push(source);
            }
            let continues = self.expression(initializer, DropExpressionUse::Read, state)?;
            state.borrow_initializer_sources.truncate(observation_depth);
            if let Some(binding) = self.checker.marker_symbol(name).copied() {
                state
                    .pending_borrow_results
                    .retain(|pending| *pending != binding);
                self.commit_borrow_call_handoff(binding, continues, state);
                if continues {
                    state.borrow_bindings.push(binding);
                    self.commit_borrow_binding_child(binding, initializer, state);
                }
            }
            if !continues {
                return Ok(false);
            }
            return Ok(true);
        }
        let closure = self.closure_origin(initializer, state)?;
        let closure_asap = self.closure_value_asap(initializer, state)?;
        let result =
            self.evaluate_owner_expression(initializer, DropExpressionUse::Consume, state)?;
        if !result.continues {
            return Ok(false);
        }
        if let Some(symbol) = self.checker.marker_symbol(name).copied()
            && !self.checker.is_move_only_variable(symbol)
        {
            self.owner_loop.bridge_scalar_completed(
                UnitStatementId::new(self.checker.source_unit, id),
                symbol,
                self.checker.unit_expression(initializer),
            );
        }
        if let Some(symbol) = self.checker.marker_symbol(name).copied()
            && self.checker.is_move_only_variable(symbol)
        {
            self.binding_depths.insert(symbol, self.scope_depth);
            state.insert(OwnedValue {
                owner: self.bound_owner(symbol, initializer, result.owner),
                symbol,
                origin: marker_span(name),
                declaration: marker_span(name),
                scope_depth: self.scope_depth,
            });
            self.bind_replace_consumer(result.replace_consumer, initializer, symbol, state);
            self.bind_normal_header(initializer, symbol, state);
            self.inserted_owner(symbol, initializer);
            if let Some(closure) = closure {
                state.closures.insert(symbol, closure);
                state
                    .closure_asap
                    .insert(symbol, closure_asap == Some(true));
            }
            if !self.liveness.statement_after[id.index()].contains(&symbol) {
                self.drop_named_asap(PlannerDropPoint::AfterStatement(id), symbol, state);
            }
        }
        self.finish_nested_resume_statement(id, state);
        self.finish_root_suffix_statement(id, state);
        Ok(true)
    }
}
