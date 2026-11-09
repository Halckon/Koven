use crate::{
    ast::{ExpressionId, StatementId},
    diagnostic::codes,
    name_resolution::SourceUnitId,
    parser::Statement,
    source::Span,
    type_checking::{BuiltinType, TypeCheckingError, UnitStatementId, UnitTypeId},
};

use super::flow::{extend_facts, intersect_facts, merge_flow_versions};
use super::while_flow::PendingControlSite;
use super::{BodyChecker, CompilationUnitTypeError, ExpressionCheck, UnitExpressionId};

impl BodyChecker<'_> {
    pub(super) fn check_while_statement(
        &mut self,
        source: SourceUnitId,
        statement: StatementId,
        condition: ExpressionId,
        body: StatementId,
        return_type: UnitTypeId,
        return_span: Option<Span>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let completion = self.check_repeating_flow(
            source,
            Some(condition),
            body,
            return_type,
            return_span,
            true,
        )?;
        self.note_pending_statement_exit(source, statement, completion.pending_exit);
        self.parts.loop_falls_through.insert(
            UnitStatementId::new(source, statement),
            completion.falls_through,
        );
        Ok(ExpressionCheck {
            ty: self.builtin(BuiltinType::Unit),
            falls_through: completion.falls_through,
        })
    }

    pub(super) fn check_loop_statement(
        &mut self,
        source: SourceUnitId,
        statement: StatementId,
        body: StatementId,
        return_type: UnitTypeId,
        return_span: Option<Span>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let completion =
            self.check_repeating_flow(source, None, body, return_type, return_span, false)?;
        self.note_pending_statement_exit(source, statement, completion.pending_exit);
        let statement = UnitStatementId::new(source, statement);
        self.parts
            .loop_falls_through
            .insert(statement, completion.falls_through);
        Ok(ExpressionCheck {
            ty: self.builtin(BuiltinType::Unit),
            falls_through: completion.falls_through,
        })
    }

    pub(super) fn check_return(
        &mut self,
        source: SourceUnitId,
        keyword_span: Span,
        value: Option<ExpressionId>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        if self.callable_loop_bases.is_empty() {
            if let Some(value) = value {
                self.check_expression(source, value, None, None, return_type)?;
            }
            self.emit(
                codes::RETURN_OUTSIDE_CALLABLE,
                "return is not inside a callable",
                keyword_span,
            )?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: false,
            });
        }
        let normal_return = match value {
            Some(value) if self.is_builtin(return_type, BuiltinType::Unit) => {
                self.check_expression(source, value, None, None, return_type)?;
                let primary = self
                    .file(source)
                    .ast()
                    .expressions()
                    .get(value)
                    .map_err(TypeCheckingError::from)?
                    .span();
                self.emit_maybe_label(
                    codes::RETURN_SHAPE_MISMATCH,
                    "return value does not match the callable return contract",
                    primary,
                    self.current_return_span,
                    "function return type declared here",
                )?;
                false
            }
            Some(value) => {
                let expected = (!self.is_deferred(return_type) && !self.is_error(return_type))
                    .then_some(return_type);
                let result = self.check_expression(
                    source,
                    value,
                    expected,
                    self.current_return_span,
                    return_type,
                )?;
                !self.is_error(result.ty)
                    && self.has_pending_expression_exit(source, value, result)?
            }
            None if !self.is_builtin(return_type, BuiltinType::Unit)
                && !self.is_deferred(return_type)
                && !self.is_error(return_type) =>
            {
                self.emit_maybe_label(
                    codes::RETURN_SHAPE_MISMATCH,
                    "return value does not match the callable return contract",
                    keyword_span,
                    self.current_return_span,
                    "function return type declared here",
                )?;
                false
            }
            None => true,
        };
        if normal_return {
            self.record_pending_callable_return();
        }
        Ok(ExpressionCheck {
            ty: self.builtin(BuiltinType::Nothing),
            falls_through: false,
        })
    }

    pub(super) fn check_loop_jump(
        &mut self,
        keyword_span: Span,
        jump: super::while_flow::LoopJump,
        message: &'static str,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let loop_base = self.callable_loop_bases.last().copied().unwrap_or(0);
        let ty = if self.loop_depth > loop_base {
            self.record_pending_loop_jump(jump);
            self.builtin(BuiltinType::Nothing)
        } else {
            self.emit(codes::JUMP_OUTSIDE_LOOP, message, keyword_span)?;
            self.error_type()
        };
        Ok(ExpressionCheck {
            ty,
            falls_through: false,
        })
    }

    pub(super) fn check_loop_body(
        &mut self,
        source: SourceUnitId,
        body: StatementId,
        return_type: UnitTypeId,
        return_span: Option<Span>,
        ordinary: bool,
        pending: bool,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        self.loop_depth += 1;
        let result = self.check_loop_paths(ordinary, pending, |checker| {
            checker.check_statement(source, body, return_type, return_span)
        });
        self.loop_depth -= 1;
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_if(
        &mut self,
        expression: UnitExpressionId,
        condition: ExpressionId,
        then_branch: StatementId,
        else_span: Option<Span>,
        else_branch: Option<StatementId>,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let source = expression.source_unit();
        let boolean = self.builtin(BuiltinType::Boolean);
        let deferred = self.deferred_facts.len();
        let condition_result =
            self.check_expression(source, condition, Some(boolean), None, return_type)?;
        let condition_pending = else_branch.is_some()
            && self.replacement_value_pending(
                UnitExpressionId::new(source, condition),
                condition_result.ty,
                deferred,
            );
        let baseline = self.flow_facts.clone();
        let baseline_versions = self.flow_versions.clone();
        let condition_edges = self.boolean_pending_edges(source, condition, &baseline_versions)?;
        let condition_exits =
            self.has_pending_expression_exit(source, condition, condition_result)?;
        let then_reachable = condition_exits && condition_edges.yes.is_some();
        let else_reachable = condition_exits && condition_edges.no.is_some();
        let (ordinary_then, ordinary_else) =
            self.parts
                .structural_boolean_outcomes_at(source, self.file(source), condition);
        let (true_facts, false_facts) = self.condition_facts(source, condition)?;
        self.flow_facts = extend_facts(&baseline, &true_facts);
        if let Some(pending) = &condition_edges.yes {
            self.flow_versions.set_pending_keys(pending.iter().copied());
        }
        let then_expected = if else_branch.is_some() {
            self.control_branch_expected(source, then_branch, expected)?
        } else {
            expected
        };
        let mut then_pending = false;
        let then_result = self.check_loop_paths(ordinary_then, then_reachable, |checker| {
            checker.check_value_body_with_replacement(
                source,
                then_branch,
                then_expected,
                expected_span,
                return_type,
                else_branch.is_some().then_some(&mut then_pending),
            )
        })?;
        let then_facts = self.flow_facts.clone();
        let then_versions = self.flow_versions.clone();
        let then_falls =
            then_reachable && self.has_pending_statement_exit(source, then_branch, then_result);
        let Some(else_branch) = else_branch else {
            let false_path = extend_facts(&baseline, &false_facts);
            self.flow_facts = if then_result.falls_through {
                intersect_facts(&then_facts, &false_path)
            } else {
                false_path
            };
            self.flow_versions = if then_result.falls_through {
                merge_flow_versions(&then_versions, &baseline_versions)
            } else {
                baseline_versions
            };
            let then_pending = then_falls.then(|| then_versions.pending_keys());
            self.flow_versions.set_pending_keys(
                then_pending.into_iter().flatten().chain(
                    condition_edges
                        .no
                        .into_iter()
                        .filter(|_| else_reachable)
                        .flatten(),
                ),
            );
            self.note_pending_exit(
                PendingControlSite::Expression(expression),
                then_falls || else_reachable,
            );
            return Ok(ExpressionCheck {
                ty: self.builtin(BuiltinType::Unit),
                falls_through: ordinary_else || ordinary_then && then_result.falls_through,
            });
        };
        self.flow_facts = extend_facts(&baseline, &false_facts);
        self.flow_versions = baseline_versions.clone();
        if let Some(pending) = &condition_edges.no {
            self.flow_versions.set_pending_keys(pending.iter().copied());
        }
        let else_expected = self.control_branch_expected(source, else_branch, expected)?;
        let mut else_pending = false;
        let else_result = self.check_loop_paths(ordinary_else, else_reachable, |checker| {
            checker.check_value_body_with_replacement(
                source,
                else_branch,
                else_expected,
                expected_span,
                return_type,
                Some(&mut else_pending),
            )
        })?;
        let else_facts = self.flow_facts.clone();
        let else_versions = self.flow_versions.clone();
        let else_falls =
            else_reachable && self.has_pending_statement_exit(source, else_branch, else_result);
        self.note_pending_exit(
            PendingControlSite::Expression(expression),
            then_falls || else_falls,
        );
        if (then_falls || else_falls)
            && (!then_falls || self.is_builtin(then_result.ty, BuiltinType::Boolean))
            && (!else_falls || self.is_builtin(else_result.ty, BuiltinType::Boolean))
        {
            let mut edges = super::while_flow::PendingBooleanEdges::default();
            if then_falls {
                self.merge_boolean_branch(&mut edges, source, then_branch, &then_versions)?;
            }
            if else_falls {
                self.merge_boolean_branch(&mut edges, source, else_branch, &else_versions)?;
            }
            self.pending_boolean_edges.insert(expression, edges);
        }
        let pending: Vec<_> = then_falls
            .then(|| then_versions.pending_keys())
            .into_iter()
            .flatten()
            .chain(
                else_falls
                    .then(|| else_versions.pending_keys())
                    .into_iter()
                    .flatten(),
            )
            .collect();
        self.flow_facts = match (then_result.falls_through, else_result.falls_through) {
            (true, true) => intersect_facts(&then_facts, &else_facts),
            (true, false) => then_facts,
            (false, true) => else_facts,
            (false, false) => baseline,
        };
        self.flow_versions = match (then_result.falls_through, else_result.falls_through) {
            (true, true) => merge_flow_versions(&then_versions, &else_versions),
            (true, false) => then_versions,
            (false, true) => else_versions,
            (false, false) => baseline_versions,
        };
        self.flow_versions.set_pending_keys(pending);
        let ty = if let Some(join) = self.join_value_branches(
            expression,
            (then_branch, then_result.ty),
            (else_branch, else_result.ty),
            expected,
        )? {
            join
        } else {
            let primary = else_span.unwrap_or(
                self.file(source)
                    .ast()
                    .statements()
                    .get(else_branch)
                    .map_err(TypeCheckingError::from)?
                    .span(),
            );
            let first = self
                .file(source)
                .ast()
                .statements()
                .get(then_branch)
                .map_err(TypeCheckingError::from)?
                .span();
            self.emit_maybe_label(
                codes::NO_COMMON_BRANCH_TYPE,
                "control branches do not have a common type",
                primary,
                Some(first),
                format!("first branch has type {}", self.type_name(then_result.ty)),
            )?;
            self.error_type()
        };
        // Only values on normal exits can be stored by an enclosing assignment.
        self.replacement_values.insert(
            expression,
            condition_pending || then_falls && then_pending || else_falls && else_pending,
        );
        Ok(ExpressionCheck {
            ty,
            falls_through: ordinary_then && then_result.falls_through
                || ordinary_else && else_result.falls_through,
        })
    }

    pub(super) fn check_value_body(
        &mut self,
        source: SourceUnitId,
        statement: StatementId,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        self.check_value_body_with_replacement(
            source,
            statement,
            expected,
            expected_span,
            return_type,
            None,
        )
    }

    pub(super) fn check_value_body_with_replacement(
        &mut self,
        source: SourceUnitId,
        statement: StatementId,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
        replacement_pending: Option<&mut bool>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let node = self
            .file(source)
            .ast()
            .statements()
            .get(statement)
            .map_err(TypeCheckingError::from)?;
        let span = node.span();
        let payload = node.payload().clone();
        let mut result = match payload {
            Statement::LambdaBody { elements } | Statement::ControlBody { elements } => self
                .check_value_elements(
                    source,
                    statement,
                    &elements,
                    span,
                    expected,
                    expected_span,
                    return_type,
                    replacement_pending,
                ),
            Statement::Expression { expression } => {
                let result = self.check_value_tail(
                    source,
                    expression,
                    expected,
                    expected_span,
                    return_type,
                    replacement_pending,
                )?;
                let pending_exit = self.has_pending_expression_exit(source, expression, result)?;
                self.note_pending_statement_exit(source, statement, pending_exit);
                Ok(result)
            }
            _ => self.check_statement(source, statement, return_type, expected_span),
        }?;
        if !result.falls_through && !self.is_error(result.ty) {
            result.ty = self.builtin(BuiltinType::Nothing);
        }
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    fn check_value_elements(
        &mut self,
        source: SourceUnitId,
        statement: StatementId,
        elements: &[StatementId],
        body_span: Span,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
        replacement_pending: Option<&mut bool>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let unit = self.builtin(BuiltinType::Unit);
        let Some((&last, prefix)) = elements.split_last() else {
            let ty = self.check_empty_control_body(unit, body_span, expected, expected_span)?;
            return Ok(ExpressionCheck {
                ty,
                falls_through: true,
            });
        };
        let mut falls_through = true;
        let mut pending_exit = true;
        for &element in prefix {
            let result = self.check_loop_paths(falls_through, pending_exit, |checker| {
                checker.check_statement(source, element, return_type, expected_span)
            })?;
            falls_through &= result.falls_through;
            pending_exit &= self.has_pending_statement_exit(source, element, result);
        }
        let payload = self
            .file(source)
            .ast()
            .statements()
            .get(last)
            .map_err(TypeCheckingError::from)?
            .payload()
            .clone();
        let result = self.check_loop_paths(falls_through, pending_exit, |checker| {
            if let Statement::Expression { expression } = payload {
                checker.check_value_tail(
                    source,
                    expression,
                    expected,
                    expected_span,
                    return_type,
                    replacement_pending,
                )
            } else {
                checker.check_statement(source, last, return_type, expected_span)
            }
        })?;
        if pending_exit {
            pending_exit = if let Statement::Expression { expression } = payload {
                self.has_pending_expression_exit(source, expression, result)?
            } else {
                self.has_pending_statement_exit(source, last, result)
            };
        }
        self.note_pending_statement_exit(source, statement, pending_exit);
        falls_through &= result.falls_through;
        Ok(ExpressionCheck {
            ty: result.ty,
            falls_through,
        })
    }

    fn check_value_tail(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
        replacement_pending: Option<&mut bool>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let deferred = self.deferred_facts.len();
        let result =
            self.check_expression(source, expression, expected, expected_span, return_type)?;
        if let Some(pending) = replacement_pending {
            *pending = self.replacement_value_pending(
                UnitExpressionId::new(source, expression),
                result.ty,
                deferred,
            );
        }
        Ok(result)
    }

    fn check_empty_control_body(
        &mut self,
        unit: UnitTypeId,
        body_span: Span,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let Some(expected) = expected else {
            return Ok(unit);
        };
        if self.assignable(unit, expected) {
            return Ok(unit);
        }
        self.emit_maybe_label(
            codes::TYPE_MISMATCH,
            "expression type does not match the expected type",
            body_span,
            expected_span,
            format!(
                "expected {}, found {}",
                self.type_name(expected),
                self.type_name(unit)
            ),
        )?;
        Ok(self.error_type())
    }
}
