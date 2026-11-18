use super::*;

impl Checker<'_, '_> {
    /// Resolves the concrete SSA type of a callable value expression when all continuing
    /// paths share the same representation (`FunctionPointer` or `ConcreteClosure`).
    pub(super) fn callable_ssa_type(
        &self,
        expression: ExpressionId,
    ) -> Option<crate::ssa::model::SsaTypeId> {
        let root = self.ungrouped(expression);
        let node = self.inputs.parsed.ast().expressions().get(root).ok()?;
        match node.payload() {
            Expression::Lambda { .. } => self
                .inputs
                .callables
                .get(&self.id(root))
                .map(|callable| callable.callable),
            Expression::Name => {
                if let Some(symbol) = self.reference(root)
                    && self.callables.contains(&symbol)
                {
                    return self.callable_ssa_types.get(&symbol).copied();
                }
                if self.known_function(root)
                    && let Some(fact) = self.inputs.owned.callable_origin(self.id(root))
                    && let lang_frontend::ownership_checking::UnitCallableOrigin::KnownFunction(
                        target,
                    ) = fact.origin()
                {
                    return self.inputs.known_functions.get(&target).map(|(_, ty)| *ty);
                }
                None
            }
            Expression::Call { .. } => self
                .inputs
                .source_plan
                .call_site(self.inputs.source_token, self.id(root))
                .and_then(|site| self.inputs.function_ids.get(site.key()))
                .and_then(|callee| self.inputs.callable_returns.get(callee).copied()),
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let (reaches_then, reaches_else) = self.condition_outcomes(*condition);
                let mut unified = None;
                if reaches_then && self.statement_may_continue(*then_branch) {
                    let ty = self
                        .callable_branch_tail(*then_branch)
                        .and_then(|tail| self.callable_ssa_type(tail))?;
                    unified = Some(ty);
                }
                if reaches_else
                    && let Some(branch) = *else_branch
                    && self.statement_may_continue(branch)
                {
                    let ty = self
                        .callable_branch_tail(branch)
                        .and_then(|tail| self.callable_ssa_type(tail))?;
                    if unified.is_some_and(|prev| prev != ty) {
                        return None;
                    }
                    unified = Some(ty);
                }
                unified
            }
            Expression::When { entries, .. } => {
                let reachable = self
                    .inputs
                    .typed
                    .when_closed_after(self.id(root))
                    .unwrap_or(entries.len());
                let mut unified = None;
                for entry in entries.iter().take(reachable) {
                    if self.statement_may_continue(entry.body) {
                        let ty = self
                            .callable_branch_tail(entry.body)
                            .and_then(|tail| self.callable_ssa_type(tail))?;
                        if unified.is_some_and(|prev| prev != ty) {
                            return None;
                        }
                        unified = Some(ty);
                    }
                }
                unified
            }
            _ => None,
        }
    }

    fn callable_branch_tail(
        &self,
        statement: lang_frontend::ast::StatementId,
    ) -> Option<ExpressionId> {
        let node = self.inputs.parsed.ast().statements().get(statement).ok()?;
        match node.payload() {
            lang_frontend::parser::Statement::Expression { expression } => Some(*expression),
            lang_frontend::parser::Statement::Block { elements }
            | lang_frontend::parser::Statement::ControlBody { elements } => {
                let last = *elements.last()?;
                match self
                    .inputs
                    .parsed
                    .ast()
                    .statements()
                    .get(last)
                    .ok()?
                    .payload()
                {
                    lang_frontend::parser::Statement::Expression { expression } => {
                        Some(*expression)
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn callable_branch_statement(&mut self, statement: lang_frontend::ast::StatementId) -> bool {
        let Ok(node) = self.inputs.parsed.ast().statements().get(statement) else {
            return false;
        };
        match node.payload().clone() {
            lang_frontend::parser::Statement::Expression { expression } => {
                self.callable_value_expr(expression)
            }
            lang_frontend::parser::Statement::Block { elements }
            | lang_frontend::parser::Statement::ControlBody { elements } => {
                let Some((&last, prefix)) = elements.split_last() else {
                    return false;
                };
                for &stmt in prefix {
                    if !self.statement(stmt) {
                        return false;
                    }
                    if !self.statement_may_continue(stmt) {
                        return true;
                    }
                }
                if let Ok(last_node) = self.inputs.parsed.ast().statements().get(last)
                    && let lang_frontend::parser::Statement::Expression { expression } =
                        last_node.payload()
                {
                    return self.callable_value_expr(*expression);
                }
                self.statement(last) && !self.statement_may_continue(last)
            }
            _ => self.statement(statement) && !self.statement_may_continue(statement),
        }
    }

    /// Validates a callable value expression (lambda, local callable, known function,
    /// callable-returning call, or `if`/`when` joining matching callable SSA types).
    pub(super) fn callable_value_expr(&mut self, expression: ExpressionId) -> bool {
        let root = self.ungrouped(expression);
        let Ok(node) = self.inputs.parsed.ast().expressions().get(root) else {
            return false;
        };
        match node.payload().clone() {
            Expression::Lambda { .. } => {
                self.lambda_formation(root) && self.callable_ssa_type(root).is_some()
            }
            Expression::Name | Expression::Call { .. } => {
                self.callable_value(root) && self.expression(root)
            }
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let Some(else_branch) = else_branch else {
                    return false;
                };
                let saved = self.view_symbols.clone();
                let cond_ok = self.condition(condition, true);
                let (reaches_then, reaches_else) = self.condition_outcomes(condition);
                let then_ok =
                    cond_ok && (!reaches_then || self.callable_branch_statement(then_branch));
                let then_views = self.view_symbols.clone();
                self.view_symbols = saved.clone();
                self.note_else_view(condition);
                let else_ok = !reaches_else || self.callable_branch_statement(else_branch);
                let else_views = self.view_symbols.clone();
                let then_continues = reaches_then && self.statement_may_continue(then_branch);
                let else_continues = reaches_else && self.statement_may_continue(else_branch);
                self.view_symbols = saved
                    .into_iter()
                    .filter(|symbol| {
                        (!then_continues || then_views.contains(symbol))
                            && (!else_continues || else_views.contains(symbol))
                    })
                    .collect();
                let ok = then_ok && else_ok && self.callable_ssa_type(root).is_some();
                self.migrated |= ok;
                ok
            }
            Expression::When {
                subject: None,
                entries,
                ..
            } => {
                let closed_after = self.inputs.typed.when_closed_after(self.id(root));
                let reachable = closed_after.unwrap_or(entries.len());
                let mut has_else = closed_after.is_some();
                for entry in entries.iter().take(reachable) {
                    if entry.else_span.is_some() {
                        has_else = true;
                    }
                    for condition in &entry.conditions {
                        match condition {
                            lang_frontend::parser::WhenCondition::Expression(expr) => {
                                if !self.condition(*expr, false) {
                                    return false;
                                }
                            }
                            _ => return false,
                        }
                    }
                    if !self.callable_branch_statement(entry.body) {
                        return false;
                    }
                }
                let ok = has_else && self.callable_ssa_type(root).is_some();
                self.migrated |= ok;
                ok
            }
            _ => false,
        }
    }

    /// A non-lambda expression whose result is a concrete callable value (a moved local
    /// callable binding or a direct call to a function returning a concrete callable).
    pub(super) fn callable_value(&self, expression: ExpressionId) -> bool {
        let root = self.ungrouped(expression);
        let Ok(node) = self.inputs.parsed.ast().expressions().get(root) else {
            return false;
        };
        match node.payload() {
            Expression::Name => {
                self.reference(root)
                    .is_some_and(|symbol| self.callables.contains(&symbol))
                    || self.known_function(root)
            }
            Expression::Call { .. } => self
                .inputs
                .source_plan
                .call_site(self.inputs.source_token, self.id(root))
                .and_then(|site| self.inputs.function_ids.get(site.key()))
                .is_some_and(|callee| self.inputs.callable_returns.contains_key(callee)),
            _ => false,
        }
    }

}
