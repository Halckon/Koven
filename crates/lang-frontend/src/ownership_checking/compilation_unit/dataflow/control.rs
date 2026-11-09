//! Control result 的 ownership dataflow 与 branch usage 规范化。

use crate::{
    ast::{ExpressionId, StatementId},
    ownership_checking::OwnershipCheckingError,
    parser::{BinaryOperator, Expression, Statement, WhenCondition, WhenEntry},
    type_checking::Copyability,
};

use super::{Checker, ExpressionUse, Flows, State, flow::merge_optional_state};

impl Checker<'_> {
    pub(super) fn check_if(
        &mut self,
        control: ExpressionId,
        condition: ExpressionId,
        arms: (StatementId, Option<StatementId>),
        state: State,
        branch_usage: ExpressionUse,
        escaping: bool,
    ) -> Result<Flows, OwnershipCheckingError> {
        let mut prefix = self.check_expression(condition, state, ExpressionUse::Read)?;
        let Some(mut base) = prefix.next.take() else {
            return Ok(prefix);
        };
        let (then_branch, else_branch) = arms;
        let (reaches_then, reaches_else) = self.ordinary_condition_outcomes(condition);
        if reaches_then {
            let mut then_state = base.clone();
            self.release_exclusive_branch(control, 0, &mut then_state);
            prefix.merge(self.check_control_body(
                then_branch,
                then_state,
                branch_usage,
                escaping,
            )?);
        }
        if reaches_else {
            self.release_exclusive_branch(control, 1, &mut base);
            prefix.merge(if let Some(else_branch) = else_branch {
                self.check_control_body(else_branch, base, branch_usage, escaping)?
            } else {
                Flows::next(base)
            });
        }
        Ok(prefix)
    }

    pub(super) fn check_when(
        &mut self,
        control: ExpressionId,
        subject: Option<ExpressionId>,
        entries: &[WhenEntry],
        state: State,
        branch_usage: ExpressionUse,
        escaping: bool,
    ) -> Result<Flows, OwnershipCheckingError> {
        let mut prefix = Flows::next(state);
        if let Some(subject) = subject {
            prefix = self.check_nullable_when_subject(control, subject, prefix)?;
        }
        let Some(mut unmatched) = prefix.next.take() else {
            return Ok(prefix);
        };
        self.begin_nullable_when(control)?;
        let mut branches = Flows::default();
        let control_id = self.unit_expression(control);
        let closed_after = self.typed.when_closed_after(control_id);
        for (entry_index, entry) in entries
            .iter()
            .take(closed_after.unwrap_or(entries.len()))
            .enumerate()
        {
            if entry.else_span.is_some() {
                branches.merge(self.check_nullable_when_body(
                    control,
                    entry_index,
                    entry.body,
                    unmatched,
                    branch_usage,
                    escaping,
                )?);
                prefix.merge(branches);
                return Ok(prefix);
            }
            let mut condition_state = Some(unmatched);
            let mut body_state = None;
            let selector_count = self
                .typed
                .when_final_selector_count(control_id, entry_index)
                .unwrap_or(entry.conditions.len());
            for condition in entry.conditions.iter().take(selector_count) {
                let Some(current) = condition_state.take() else {
                    break;
                };
                let expression = match condition {
                    WhenCondition::Expression(expression)
                    | WhenCondition::Contains { expression, .. } => Some(*expression),
                    WhenCondition::TypeTest { .. } => None,
                };
                if let Some(expression) = expression {
                    let mut flows =
                        self.check_expression(expression, current, ExpressionUse::Read)?;
                    let (can_match, can_miss) = if subject.is_none() {
                        self.ordinary_condition_outcomes(expression)
                    } else {
                        (true, true)
                    };
                    let next = flows.next.take();
                    if can_match {
                        merge_optional_state(&mut body_state, next.clone());
                    }
                    if can_miss {
                        condition_state = next;
                    }
                    prefix.merge(flows);
                } else {
                    merge_optional_state(&mut body_state, Some(current.clone()));
                    condition_state = Some(current);
                }
            }
            if let Some(body_state) = body_state {
                branches.merge(self.check_nullable_when_body(
                    control,
                    entry_index,
                    entry.body,
                    body_state,
                    branch_usage,
                    escaping,
                )?);
            }
            let Some(next) = condition_state else {
                prefix.merge(branches);
                return Ok(prefix);
            };
            unmatched = next;
        }
        if closed_after.is_none() {
            self.release_exclusive_branch(control, entries.len(), &mut unmatched);
            branches.merge(Flows::next(unmatched));
        }
        prefix.merge(branches);
        Ok(prefix)
    }

    pub(super) fn control_result_usage(
        &self,
        expression: ExpressionId,
    ) -> Result<ExpressionUse, OwnershipCheckingError> {
        let expression = self.unit_expression(expression);
        let ty = self.typed.expression_type(expression).ok_or(
            OwnershipCheckingError::InvalidUnitArgumentType {
                source_unit: expression.source_unit().index(),
                expression: expression.expression().index(),
            },
        )?;
        if self.typed.is_diagnostic_error_type(ty) {
            return Ok(ExpressionUse::Read);
        }
        match self.typed.copyability(ty) {
            Copyability::MoveOnly => Ok(ExpressionUse::Consume {
                parameter_span: None,
            }),
            Copyability::Copyable => Ok(ExpressionUse::Read),
            Copyability::Error if self.typed.copyability_error_is_diagnostic(ty) => {
                Ok(ExpressionUse::Read)
            }
            Copyability::Unknown | Copyability::Error => {
                Err(OwnershipCheckingError::InvalidUnitArgumentType {
                    source_unit: expression.source_unit().index(),
                    expression: expression.expression().index(),
                })
            }
        }
    }

    /// Check escaping values without classifying ordinary control tails as factory returns.
    pub(super) fn check_escaping_expression(
        &mut self,
        expression: ExpressionId,
        state: State,
        usage: ExpressionUse,
    ) -> Result<Flows, OwnershipCheckingError> {
        match self
            .parsed
            .ast()
            .expressions()
            .get(expression)?
            .payload()
            .clone()
        {
            Expression::Group { expression } => {
                self.check_escaping_expression(expression, state, usage)
            }
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.check_if(
                expression,
                condition,
                (then_branch, else_branch),
                state,
                usage,
                true,
            ),
            Expression::When {
                subject, entries, ..
            } => self.check_when(expression, subject, &entries, state, usage, true),
            Expression::Binary {
                left,
                operator: BinaryOperator::Elvis,
                right,
                ..
            } => {
                self.reject_borrowed_closure_escape(left, &state)?;
                self.reject_borrowed_closure_escape(right, &state)?;
                self.check_expression(expression, state, usage)
            }
            _ => {
                self.reject_borrowed_closure_escape(expression, &state)?;
                self.check_expression(expression, state, usage)
            }
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
        let parents = state
            .borrow_bindings
            .keys()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let Some((&tail, prefix)) = elements.split_last() else {
            return Ok(Flows::next(state));
        };
        let locals = self.scope_locals(&elements)?;
        let mut flows = self.check_element_prefix(&elements, prefix.len(), state)?;
        let Some(next) = flows.next.take() else {
            self.check_scope_exit_loans(&locals, None, &mut flows)?;
            self.restore_borrow_scope(&parents, &mut flows);
            return Ok(flows);
        };
        let tail_payload = self.parsed.ast().statements().get(tail)?.payload().clone();
        let tail_expression = match tail_payload {
            Statement::Expression { expression } => Some(expression),
            _ => None,
        };
        let diagnostic_count = self.diagnostics.len();
        let tail_flows = match tail_payload {
            Statement::Expression { expression } => {
                if escaping {
                    self.check_escaping_expression(expression, next, usage)?
                } else {
                    self.check_expression(expression, next, usage)?
                }
            }
            _ => self.check_statement(tail, next)?,
        };
        flows.merge(tail_flows);
        if !escaping || self.diagnostics.len() == diagnostic_count {
            self.check_scope_exit_loans(&locals, tail_expression, &mut flows)?;
        }
        self.restore_borrow_scope(&parents, &mut flows);
        Ok(flows)
    }
}
