//! 循环回边的保守失效。while 条件每轮检查，for source 在进入前已求值。
use super::{flow::FlowKey, *};
use crate::parser::{Item, Statement};
use crate::type_checking::flow_effects::{
    collect_expression_mutation_targets, collect_expression_mutation_targets_with_inout_filter,
    collect_statement_mutation_targets,
};

#[derive(Clone, Default)]
pub(super) struct PendingLoopExits {
    breaks: Option<BTreeSet<SymbolId>>,
    continues: BTreeSet<SymbolId>,
}

impl Checker<'_> {
    pub(super) fn pending_expression_exit(
        &self,
        expression: ExpressionId,
        falls_through: bool,
    ) -> bool {
        falls_through
            && !self
                .iteration_recovery
                .abrupt_expressions
                .contains(&expression.index())
    }

    pub(super) fn pending_statement_exit(
        &self,
        statement: StatementId,
        falls_through: bool,
    ) -> Result<bool, TypeCheckingError> {
        if !falls_through
            || self
                .iteration_recovery
                .abrupt_statements
                .contains(&statement.index())
        {
            return Ok(false);
        }
        if self.iteration_recovery.abrupt_expressions.is_empty() {
            return Ok(true);
        }
        let expression = match self.ast().statements().get(statement)?.payload() {
            Statement::Expression { expression } => *expression,
            Statement::LocalDestructuring { initializer, .. } => *initializer,
            Statement::LocalVariable { declaration } => {
                match self.ast().items().get(*declaration)?.payload() {
                    Item::Variable { initializer, .. } => *initializer,
                    _ => return Ok(true),
                }
            }
            _ => return Ok(true),
        };
        Ok(self.pending_expression_exit(expression, true))
    }

    /// Still check unreachable syntax for diagnostics, while isolating its writebacks and jumps.
    pub(super) fn check_pending_path<T>(
        &mut self,
        reachable: bool,
        check: impl FnOnce(&mut Self) -> Result<T, TypeCheckingError>,
    ) -> Result<T, TypeCheckingError> {
        if reachable {
            return check(self);
        }
        let bindings = self.iteration_recovery.bindings.clone();
        let revision = self.iteration_recovery.revision;
        let previous = std::mem::replace(&mut self.iteration_recovery.suppressed, true);
        let result = check(self);
        self.iteration_recovery.bindings = bindings;
        self.iteration_recovery.revision = revision;
        self.iteration_recovery.suppressed = previous;
        result
    }

    pub(super) fn note_pending_loop_jump(&mut self, breaks: bool) {
        if !self.iteration_recovery.suppressed
            && let Some(exits) = self.iteration_recovery.loop_exits.last_mut()
        {
            let pending = &self.iteration_recovery.bindings;
            if breaks {
                exits
                    .breaks
                    .get_or_insert_with(BTreeSet::new)
                    .extend(pending);
            } else {
                exits.continues.extend(pending);
            }
        }
    }

    pub(super) fn expression_flow_writes(
        &self,
        expression: ExpressionId,
    ) -> Result<BTreeSet<FlowKey>, TypeCheckingError> {
        Ok(collect_expression_mutation_targets_with_inout_filter(
            self.syntactic_boolean_outcomes,
            expression,
            |call| {
                self.inout_call_may_write
                    .get(&call.index())
                    .copied()
                    .unwrap_or(true)
            },
        )?
        .into_iter()
        .filter_map(|target| self.stable_flow_key(target))
        .collect())
    }

    pub(super) fn check_repeating_flow(
        &mut self,
        condition: Option<ExpressionId>,
        body: StatementId,
        can_exhaust: bool,
    ) -> Result<bool, TypeCheckingError> {
        let mut targets =
            collect_statement_mutation_targets(self.syntactic_boolean_outcomes, body)?;
        if let Some(condition) = condition {
            targets.extend(collect_expression_mutation_targets(
                self.syntactic_boolean_outcomes,
                condition,
            )?);
        }
        let mut pending_writes = BTreeSet::new();
        let mut writes = BTreeSet::new();
        for target in targets {
            writes.extend(self.stable_flow_key(target));
            if let Some(symbol) = self.assignment_variable(target)?
                && self.symbol_type(symbol).is_some()
            {
                pending_writes.insert(symbol);
            }
        }
        for key in writes {
            self.flow_facts.remove(&key);
            *self.flow_versions.entry(key).or_default() += 1;
        }
        let head = self.flow_facts.clone();
        let mut pending_head = self.iteration_recovery.bindings.clone();
        let replay = self
            .iteration_replay_sites
            .contains(&body.index())
            .then(|| self.trial_state());
        loop {
            let mut enters = true;
            let mut exhaustion = can_exhaust.then(|| self.iteration_recovery.bindings.clone());
            if let Some(condition) = condition {
                let boolean = self.builtin(BuiltinType::Boolean);
                let result = self.check_expression(condition, Some(boolean), None)?;
                let edges = self.pending_boolean_edges(condition, result.falls_through)?;
                enters = edges.yes.is_some();
                exhaustion = edges.no;
                self.iteration_recovery.bindings = edges.yes.unwrap_or_default();
                let (true_facts, _) = self.condition_facts(condition)?;
                self.flow_facts = flow::extend_facts(&self.flow_facts, &true_facts);
            }
            self.iteration_recovery
                .loop_exits
                .push(PendingLoopExits::default());
            let result = self.check_pending_path(enters, |checker| checker.check_loop_body(body));
            let mut exits = self
                .iteration_recovery
                .loop_exits
                .pop()
                .expect("loop frame");
            let result = result?;
            if enters
                && !self.iteration_recovery.suppressed
                && self.pending_statement_exit(body, result.falls_through)?
            {
                exits.continues.extend(&self.iteration_recovery.bindings);
            }
            let previous = pending_head.len();
            pending_head.extend(exits.continues.intersection(&pending_writes));
            if pending_head.len() != previous {
                if let Some(baseline) = &replay {
                    // Only pending entry bindings can grow. Roll back all provisional facts.
                    self.restore_trial_state(baseline.clone());
                    self.iteration_recovery.bindings = pending_head.clone();
                    continue;
                }
                self.iteration_recovery
                    .discovered_loops
                    .insert(body.index());
            }
            let has_exit = exhaustion.is_some() || exits.breaks.is_some();
            let mut exit_pending = exhaustion.unwrap_or_default();
            exit_pending.extend(exits.breaks.into_iter().flatten());
            self.iteration_recovery.bindings = exit_pending;
            self.flow_facts = head;
            return Ok(has_exit);
        }
    }
}
