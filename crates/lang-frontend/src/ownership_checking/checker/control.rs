//! 控制表达式先交付分支结果；外围借用不改变分支 tail 的 owner 契约。

use super::{Checker, ExpressionUse, Flows, OwnershipCheckingError, State, merge_optional_state};
use crate::{
    ast::{ExpressionId, StatementId},
    diagnostic::{Diagnostic, Severity},
    parser::{BinaryOperator, Expression, Statement, WhenCondition},
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
        escaping: bool,
    ) -> Result<Flows, OwnershipCheckingError> {
        let payload = self
            .parsed
            .ast()
            .statements()
            .get(statement)?
            .payload()
            .clone();
        let elements = match payload {
            Statement::ControlBody { elements } | Statement::LambdaBody { elements } => elements,
            Statement::Expression { expression } => {
                return if escaping {
                    self.check_escaping_expression(expression, state, usage)
                } else {
                    self.check_expression(expression, state, usage)
                };
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
                if escaping {
                    self.check_escaping_expression(expression, next, usage)?
                } else {
                    self.check_expression(expression, next, usage)?
                }
            }
            _ => self.check_statement(tail, next)?,
        };
        flows.merge(tail);
        Ok(flows)
    }
    /// 只在可继续的路径交付逃逸值，保留先前实参的提前退出路径。
    pub(super) fn chain_escaping_expression(
        &mut self,
        mut flows: Flows,
        id: ExpressionId,
        usage: ExpressionUse,
    ) -> Result<Flows, OwnershipCheckingError> {
        if let Some(next) = flows.next.take() {
            flows.merge(self.check_escaping_expression(id, next, usage)?);
        }
        Ok(flows)
    }

    /// 在交付分支检查逃逸；普通实参或存储交付不计为当前函数的 return。
    pub(super) fn check_escaping_expression(
        &mut self,
        id: ExpressionId,
        state: State,
        usage: ExpressionUse,
    ) -> Result<Flows, OwnershipCheckingError> {
        match self.parsed.ast().expressions().get(id)?.payload().clone() {
            Expression::Group { expression } => {
                self.check_escaping_expression(expression, state, usage)
            }
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.check_if(id, condition, then_branch, else_branch, state, true),
            Expression::When {
                subject, entries, ..
            } => self.check_when(id, subject, &entries, state, true),
            Expression::Binary {
                left,
                operator: BinaryOperator::Elvis,
                right,
                ..
            } => self.check_elvis(id, left, right, state, true),
            _ => {
                self.reject_borrowed_closure_escape(id, &state)?;
                self.check_expression(id, state, usage)
            }
        }
    }

    /// 保存成功分支实际交付的 closure 候选；被移动的分支 binding 不再拥有 capture loan。
    fn record_control_closures(
        &self,
        control: ExpressionId,
        mut body: StatementId,
        flows: &mut Flows,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(state) = flows.next.as_mut() else {
            return Ok(());
        };
        state.closure_results.remove(&control.index());
        if self.control_result_usage(control) != ExpressionUse::Consume {
            return Ok(());
        }
        loop {
            match self.parsed.ast().statements().get(body)?.payload() {
                Statement::ControlBody { elements } | Statement::LambdaBody { elements } => {
                    let Some(&tail) = elements.last() else {
                        return Ok(());
                    };
                    body = tail;
                }
                Statement::Expression { expression } => {
                    let origins = self.closure_origins(*expression, state)?;
                    if !origins.is_empty() {
                        state.closure_results.insert(control.index(), origins);
                        if let Some(symbol) = self.expression_root_symbol(*expression)? {
                            state.closures.remove(&symbol);
                        }
                    }
                    return Ok(());
                }
                _ => return Ok(()),
            }
        }
    }

    pub(super) fn check_if(
        &mut self,
        id: ExpressionId,
        condition: ExpressionId,
        then_branch: StatementId,
        else_branch: Option<StatementId>,
        state: State,
        escaping: bool,
    ) -> Result<Flows, OwnershipCheckingError> {
        let mut prefix = self.check_expression(condition, state, ExpressionUse::Read)?;
        let Some(base) = prefix.next.take() else {
            return Ok(prefix);
        };
        let usage = self.control_result_usage(id);
        let mut branches = self.check_control_body(then_branch, base.clone(), usage, escaping)?;
        self.record_control_closures(id, then_branch, &mut branches)?;
        branches.merge(if let Some(else_branch) = else_branch {
            let mut flows = self.check_control_body(else_branch, base, usage, escaping)?;
            self.record_control_closures(id, else_branch, &mut flows)?;
            flows
        } else {
            Flows::next(base)
        });
        prefix.merge(branches);
        self.release_dead_control_closures(id, &mut prefix);
        Ok(prefix)
    }

    pub(super) fn check_when(
        &mut self,
        id: ExpressionId,
        subject: Option<ExpressionId>,
        entries: &[crate::parser::WhenEntry],
        state: State,
        escaping: bool,
    ) -> Result<Flows, OwnershipCheckingError> {
        let mut prefix = Flows::next(state);
        if let Some(subject) = subject {
            prefix = self.check_nullable_subject(id, subject, prefix)?;
        }
        let plan = self.typed.nullable_when(id).cloned();
        self.begin_nullable_when(id)?;
        if let Some(state) = prefix.next.as_mut() {
            self.register_nullable_subject(id, state)?;
        }
        let mut remaining = prefix.next.take();
        for (entry_index, entry) in entries.iter().enumerate() {
            let descriptor = plan
                .as_ref()
                .and_then(|plan| plan.entries().get(entry_index));
            let mut matched = None;
            if entry.else_span.is_some() {
                matched = remaining.take();
            }
            // Only the unmatched edge evaluates the next alternative or entry.
            for (alternative_index, condition) in entry.conditions.iter().enumerate() {
                let Some(mut input) = remaining.take() else {
                    break;
                };
                self.enter_nullable_edge(id, entry_index, Some(alternative_index), &mut input);
                let expression = match condition {
                    WhenCondition::Expression(expression)
                    | WhenCondition::Contains { expression, .. } => Some(*expression),
                    WhenCondition::TypeTest { .. } => None,
                };
                let mut flows = if let Some(expression) = expression {
                    self.check_expression(expression, input, ExpressionUse::Read)?
                } else {
                    Flows::next(input)
                };
                let mut next = flows.next.take();
                if let Some(state) = next.as_mut() {
                    self.enter_nullable_edge(id, entry_index, None, state);
                }
                prefix.merge(flows);
                let alternative =
                    descriptor.and_then(|entry| entry.alternatives().get(alternative_index));
                if alternative.is_none_or(|alternative| !alternative.match_domain().is_empty()) {
                    merge_optional_state(&mut matched, next.clone());
                }
                if alternative
                    .is_none_or(|alternative| !alternative.fallthrough_domain().is_empty())
                {
                    remaining = next;
                }
            }
            if let Some(mut matched) = matched {
                self.enter_nullable_edge(id, entry_index, None, &mut matched);
                let mut flows = self.check_control_body(
                    entry.body,
                    matched,
                    self.control_result_usage(id),
                    escaping,
                )?;
                self.record_control_closures(id, entry.body, &mut flows)?;
                self.finish_nullable_branch(id, entry_index, &flows);
                prefix.merge(flows);
            }
        }
        merge_optional_state(&mut prefix.next, remaining);
        for state in [&mut prefix.next, &mut prefix.breaks, &mut prefix.continues]
            .into_iter()
            .flatten()
        {
            state.nullable_views.retain(|_, proof| proof.control != id);
        }
        self.release_dead_control_closures(id, &mut prefix);
        Ok(prefix)
    }
}
