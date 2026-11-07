//! 循环回边的保守失效。while 条件每轮检查，for source 在进入前已求值。
use super::{flow::FlowKey, *};
use crate::parser::{BinaryOperator, PrefixOperator};
use crate::type_checking::flow_effects::{
    collect_expression_mutation_targets, collect_statement_mutation_targets,
    logical_rhs_is_required,
};

#[derive(Clone, Default)]
pub(super) struct PendingLoopExits {
    breaks: Option<BTreeSet<FlowKey>>,
    continues: BTreeSet<FlowKey>,
}

#[derive(Clone, Copy)]
pub(super) enum LoopJump {
    Break,
    Continue,
}

// Pending reachability follows known Boolean edges. Ordinary completion stays
// conservative for return checking and lowering.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum PendingControlSite {
    Expression(UnitExpressionId),
    Statement(UnitStatementId),
}

#[derive(Clone, Default)]
pub(super) struct PendingBooleanEdges {
    pub(super) yes: Option<BTreeSet<FlowKey>>,
    pub(super) no: Option<BTreeSet<FlowKey>>,
}

impl PendingBooleanEdges {
    fn merge(&mut self, other: Self) {
        if let Some(yes) = other.yes {
            self.yes.get_or_insert_with(BTreeSet::new).extend(yes);
        }
        if let Some(no) = other.no {
            self.no.get_or_insert_with(BTreeSet::new).extend(no);
        }
    }
}

impl BodyChecker<'_> {
    pub(super) fn note_pending_exit(&mut self, site: PendingControlSite, reachable: bool) {
        if reachable {
            self.pending_abrupt_sites.remove(&site);
        } else {
            self.pending_abrupt_sites.insert(site);
        }
    }

    pub(super) fn note_pending_statement_exit(
        &mut self,
        source: SourceUnitId,
        statement: StatementId,
        reachable: bool,
    ) {
        self.note_pending_exit(
            PendingControlSite::Statement(UnitStatementId::new(source, statement)),
            reachable,
        );
    }

    pub(super) fn has_pending_statement_exit(
        &self,
        source: SourceUnitId,
        statement: StatementId,
        result: ExpressionCheck,
    ) -> bool {
        result.falls_through
            && !self
                .pending_abrupt_sites
                .contains(&PendingControlSite::Statement(UnitStatementId::new(
                    source, statement,
                )))
    }

    pub(super) fn has_pending_expression_exit(
        &self,
        source: SourceUnitId,
        mut expression: ExpressionId,
        result: ExpressionCheck,
    ) -> Result<bool, CompilationUnitTypeError> {
        if !result.falls_through {
            return Ok(false);
        }
        let ast = self.file(source).ast();
        while let Expression::Group { expression: inner }
        | Expression::Prefix {
            operand: inner,
            operator: PrefixOperator::Not,
            ..
        } = ast
            .expressions()
            .get(expression)
            .map_err(TypeCheckingError::from)?
            .payload()
        {
            expression = *inner;
        }
        Ok(!self
            .pending_abrupt_sites
            .contains(&PendingControlSite::Expression(UnitExpressionId::new(
                source, expression,
            ))))
    }

    pub(super) fn boolean_pending_edges(
        &self,
        source: SourceUnitId,
        mut expression: ExpressionId,
        flow: &FlowVersions,
    ) -> Result<PendingBooleanEdges, CompilationUnitTypeError> {
        let ast = self.file(source).ast();
        let mut negated = false;
        loop {
            match ast
                .expressions()
                .get(expression)
                .map_err(TypeCheckingError::from)?
                .payload()
            {
                Expression::Group { expression: inner } => expression = *inner,
                Expression::Prefix {
                    operator: PrefixOperator::Not,
                    operand,
                    ..
                } => {
                    expression = *operand;
                    negated = !negated;
                }
                _ => break,
            }
        }
        let mut edges = if let Some(edges) = self
            .pending_boolean_edges
            .get(&UnitExpressionId::new(source, expression))
        {
            edges.clone()
        } else {
            let is_true = logical_rhs_is_required(ast, expression, BinaryOperator::LogicalAnd)
                .map_err(TypeCheckingError::from)?;
            let is_false = logical_rhs_is_required(ast, expression, BinaryOperator::LogicalOr)
                .map_err(TypeCheckingError::from)?;
            PendingBooleanEdges {
                yes: (!is_false).then(|| flow.pending_keys().collect()),
                no: (!is_true).then(|| flow.pending_keys().collect()),
            }
        };
        if negated {
            std::mem::swap(&mut edges.yes, &mut edges.no);
        }
        Ok(edges)
    }

    pub(super) fn merge_boolean_branch(
        &self,
        edges: &mut PendingBooleanEdges,
        source: SourceUnitId,
        body: StatementId,
        flow: &FlowVersions,
    ) -> Result<(), CompilationUnitTypeError> {
        if let Some(tail) =
            crate::type_checking::control_body::tail_expression(self.file(source), body)?
        {
            edges.merge(self.boolean_pending_edges(source, tail, flow)?);
        }
        Ok(())
    }

    pub(super) fn record_pending_loop_jump(&mut self, jump: LoopJump) {
        if self.loop_depth <= self.suppressed_loop_depth {
            return;
        }
        if let Some(exits) = self.pending_loop_exits.last_mut() {
            let pending = self.flow_versions.pending_keys();
            match jump {
                LoopJump::Break => exits
                    .breaks
                    .get_or_insert_with(BTreeSet::new)
                    .extend(pending),
                LoopJump::Continue => exits.continues.extend(pending),
            }
        }
    }

    pub(super) fn check_loop_reachable<T>(
        &mut self,
        reachable: bool,
        check: impl FnOnce(&mut Self) -> Result<T, CompilationUnitTypeError>,
    ) -> Result<T, CompilationUnitTypeError> {
        let previous = self.suppressed_loop_depth;
        let previous_return_reachability = self
            .pending_callable_returns
            .as_ref()
            .map(|frame| frame.reachable);
        if !reachable {
            self.suppressed_loop_depth = self.loop_depth;
            if let Some(frame) = &mut self.pending_callable_returns {
                frame.reachable = false;
            }
        }
        let result = check(self);
        self.suppressed_loop_depth = previous;
        if let (Some(frame), Some(reachable)) = (
            &mut self.pending_callable_returns,
            previous_return_reachability,
        ) {
            frame.reachable = reachable;
        }
        result
    }

    pub(super) fn expression_flow_writes(
        &self,
        source: SourceUnitId,
        expression: ExpressionId,
    ) -> Result<BTreeSet<FlowKey>, CompilationUnitTypeError> {
        Ok(
            collect_expression_mutation_targets(self.file(source).ast(), expression)
                .map_err(TypeCheckingError::from)?
                .into_iter()
                .filter_map(|target| self.stable_flow_key(source, target))
                .collect(),
        )
    }

    pub(super) fn check_repeating_flow(
        &mut self,
        source: SourceUnitId,
        condition: Option<ExpressionId>,
        body: StatementId,
        return_type: UnitTypeId,
        return_span: Option<Span>,
        can_exhaust: bool,
    ) -> Result<bool, CompilationUnitTypeError> {
        let ast = self.file(source).ast();
        let mut targets =
            collect_statement_mutation_targets(ast, body).map_err(TypeCheckingError::from)?;
        if let Some(condition) = condition {
            targets.extend(
                collect_expression_mutation_targets(ast, condition)
                    .map_err(TypeCheckingError::from)?,
            );
        }
        let writes: BTreeSet<_> = targets
            .into_iter()
            .filter_map(|target| self.flow_root(source, target))
            .collect();
        for &key in &writes {
            if self.is_stable_flow_root(key) {
                self.invalidate_flow_key(key);
            }
        }
        let head = self.flow_facts.clone();
        let mut head_versions = self.flow_versions.clone();
        let site = UnitStatementId::new(source, body);
        let replay = self
            .loop_replay_sites
            .contains(&site)
            .then(|| self.trial_state());
        loop {
            let condition_edges = if let Some(condition) = condition {
                let boolean = self.builtin(BuiltinType::Boolean);
                let checked =
                    self.check_expression(source, condition, Some(boolean), None, return_type)?;
                let edges = if self.has_pending_expression_exit(source, condition, checked)? {
                    self.boolean_pending_edges(source, condition, &self.flow_versions)?
                } else {
                    PendingBooleanEdges::default()
                };
                if let Some(yes) = &edges.yes {
                    self.flow_versions.set_pending_keys(yes.iter().copied());
                }
                let (true_facts, _) = self.condition_facts(source, condition)?;
                self.flow_facts = flow::extend_facts(&self.flow_facts, &true_facts);
                Some(edges)
            } else {
                None
            };
            let enters_body = condition_edges
                .as_ref()
                .is_none_or(|edges| edges.yes.is_some());
            self.pending_loop_exits.push(PendingLoopExits::default());
            let result = self.check_loop_body(source, body, return_type, return_span, enters_body);
            let exits = self.pending_loop_exits.pop().expect("current loop frame");
            let result = result?;

            let normal_backedge = (enters_body
                && self.has_pending_statement_exit(source, body, result))
            .then(|| self.flow_versions.pending_keys());
            let backedges = normal_backedge.into_iter().flatten().chain(exits.continues);
            // Recovery writebacks can also come from invoked captures. Only
            // bindings tracked at entry can grow, irrespective of their syntax.
            if head_versions.mark_pending(backedges) {
                if let Some(baseline) = &replay {
                    // Each replay can only add a pending entry binding. All
                    // provisional diagnostics and descriptors are rolled back.
                    self.restore_trial_state(baseline.clone());
                    self.flow_versions.take_pending_from(head_versions.clone());
                    continue;
                }
                self.discovered_loop_heads.insert(site);
            }

            let mut exit_versions = head_versions.clone();
            exit_versions.clear_pending();
            let exhausts = if let Some(edges) = condition_edges {
                if let Some(no) = edges.no {
                    exit_versions.mark_pending(no);
                    true
                } else {
                    false
                }
            } else {
                if can_exhaust || exits.breaks.is_none() {
                    exit_versions.mark_pending(head_versions.pending_keys());
                }
                can_exhaust
            };
            let falls_through = exhausts || exits.breaks.is_some();
            if let Some(breaks) = exits.breaks {
                exit_versions.mark_pending(breaks);
            }
            self.flow_facts = head;
            self.flow_versions = flow::merge_flow_versions(&head_versions, &self.flow_versions);
            self.flow_versions.take_pending_from(exit_versions);
            return Ok(falls_through);
        }
    }
}
