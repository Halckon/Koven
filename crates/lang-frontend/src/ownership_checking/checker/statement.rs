//! Statement dispatch and the actual scalar-initialization completion boundary.
use super::*;

impl Checker<'_> {
    pub(super) fn check_statement(
        &mut self,
        id: StatementId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        self.prefix_init_before(id, &state);
        let flows = self.check_statement_inner(id, state)?;
        self.prefix_init_after(id, &flows);
        Ok(flows)
    }
    fn check_statement_inner(
        &mut self,
        id: StatementId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        match self.parsed.ast().statements().get(id)?.payload().clone() {
            Statement::Error => Ok(Flows::next(state)),
            Statement::Block { elements }
            | Statement::LambdaBody { elements }
            | Statement::ControlBody { elements } => self.check_borrow_scope(id, &elements, state),
            Statement::LocalVariable { declaration } => {
                let Item::Variable {
                    kind,
                    name,
                    initializer,
                    ..
                } = self
                    .parsed
                    .ast()
                    .items()
                    .get(declaration)?
                    .payload()
                    .clone()
                else {
                    return Ok(Flows::next(state));
                };
                if let VariableKind::BorrowVal(marker) = kind {
                    return self.check_borrow_binding(name, initializer, marker, state);
                }
                let diagnostic_count = self.diagnostics.len();
                let mut flows = self.check_variable(name, initializer, state)?;
                if matches!(kind, VariableKind::Val | VariableKind::Var)
                    && flows.next.is_some()
                    && self.diagnostics.len() == diagnostic_count
                    && let Some(symbol) = self.marker_symbol(name)
                {
                    self.formed_owned_locals.push(symbol);
                }
                if let Some(next) = flows.next.as_mut()
                    && let Some(symbol) = self.marker_symbol(name)
                    && !self.statement_live_after[id.index()].contains(&symbol)
                {
                    self.release_closure(symbol, next);
                }
                Ok(flows)
            }
            Statement::LocalDestructuring { initializer, .. } => {
                self.check_destructuring(id, initializer, state)
            }
            Statement::While {
                condition, body, ..
            } => {
                let errors = self.diagnostics.len();
                let mut state = state;
                state.origins.attach(&self.callable_sources.arena);
                #[cfg(any(test, feature = "loan-observation-test-support"))]
                let _zero = self.research_zero_header(id, body, &state);
                let headers = state.origins.begin_loop();
                let condition = self.check_expression(condition, state, ExpressionUse::Read)?;
                let mut flows = self.check_maybe_loop_with_headers(
                    condition,
                    body,
                    errors,
                    &headers,
                    Some(id),
                )?;
                self.release_dead_loop_closures(id, &mut flows);
                Ok(flows)
            }
            Statement::For { source, body, .. } => {
                let mut flows = self.check_iteration(id, source, body, state)?;
                self.release_dead_loop_closures(id, &mut flows);
                Ok(flows)
            }
            Statement::Loop { body, .. } => {
                let errors = self.diagnostics.len();
                let body_id = body;
                let mut state = state;
                state.origins.attach(&self.callable_sources.arena);
                let headers = state.origins.begin_loop();
                let body = self.check_statement(body, state)?;
                for state in [&body.next, &body.continues].into_iter().flatten() {
                    state.origins.backedge(&headers);
                }
                if self.diagnostics.len() == errors {
                    self.check_loop_backedge(body_id, &body)?;
                }
                self.loop_has_exit.insert(id.index(), body.breaks.is_some());
                let mut flows = Flows {
                    next: body.breaks,
                    breaks: None,
                    continues: None,
                };
                self.release_dead_loop_closures(id, &mut flows);
                Ok(flows)
            }
            Statement::Expression { expression } => {
                self.check_expression(expression, state, ExpressionUse::Read)
            }
        }
    }
}
