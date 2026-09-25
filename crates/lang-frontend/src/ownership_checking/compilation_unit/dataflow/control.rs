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
        condition: ExpressionId,
        then_branch: StatementId,
        else_branch: Option<StatementId>,
        state: State,
        branch_usage: ExpressionUse,
        escaping: bool,
    ) -> Result<Flows, OwnershipCheckingError> {
        let mut prefix = self.check_expression(condition, state, ExpressionUse::Read)?;
        let Some(base) = prefix.next.take() else {
            return Ok(prefix);
        };
        let mut branches =
            self.check_control_body(then_branch, base.clone(), branch_usage, escaping)?;
        branches.merge(if let Some(else_branch) = else_branch {
            self.check_control_body(else_branch, base, branch_usage, escaping)?
        } else {
            Flows::next(base)
        });
        prefix.merge(branches);
        Ok(prefix)
    }

    pub(super) fn check_when(
        &mut self,
        subject: Option<ExpressionId>,
        entries: &[WhenEntry],
        state: State,
        branch_usage: ExpressionUse,
        escaping: bool,
    ) -> Result<Flows, OwnershipCheckingError> {
        let mut prefix = Flows::next(state);
        if let Some(subject) = subject {
            prefix = self.chain_expression(prefix, subject, ExpressionUse::Read)?;
        }
        let Some(mut unmatched) = prefix.next.take() else {
            return Ok(prefix);
        };
        let mut branches = Flows::default();
        for entry in entries {
            if entry.else_span.is_some() {
                branches.merge(self.check_control_body(
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
            for condition in &entry.conditions {
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
                    merge_optional_state(&mut body_state, flows.next.clone());
                    condition_state = flows.next.take();
                    prefix.merge(flows);
                } else {
                    merge_optional_state(&mut body_state, Some(current.clone()));
                    condition_state = Some(current);
                }
            }
            if let Some(body_state) = body_state {
                branches.merge(self.check_control_body(
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
        branches.merge(Flows::next(unmatched));
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
        match self.typed.copyability(ty) {
            Copyability::MoveOnly => Ok(ExpressionUse::Consume {
                parameter_span: None,
            }),
            Copyability::Copyable => Ok(ExpressionUse::Read),
            Copyability::Unknown | Copyability::Error => {
                Err(OwnershipCheckingError::InvalidUnitArgumentType {
                    source_unit: expression.source_unit().index(),
                    expression: expression.expression().index(),
                })
            }
        }
    }

    pub(super) fn check_return_expression(
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
                self.check_return_expression(expression, state, usage)
            }
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.check_if(condition, then_branch, else_branch, state, usage, true),
            Expression::When {
                subject, entries, ..
            } => self.check_when(subject, &entries, state, usage, true),
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

    fn check_control_body(
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
            Statement::ControlBody { elements } => elements,
            Statement::Expression { expression } => {
                return if escaping {
                    self.check_return_expression(expression, state, usage)
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
        let tail_payload = self.parsed.ast().statements().get(tail)?.payload().clone();
        let tail_flows = match tail_payload {
            Statement::Expression { expression } => {
                if escaping {
                    self.check_return_expression(expression, next, usage)?
                } else {
                    self.check_expression(expression, next, usage)?
                }
            }
            _ => self.check_statement(tail, next)?,
        };
        flows.merge(tail_flows);
        Ok(flows)
    }
}
