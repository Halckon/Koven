//! SPEC-0197 compilation-unit 基础运算表达式类型检查。

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    diagnostic::{Diagnostic, Severity, codes},
    name_resolution::SourceUnitId,
    parser::{BinaryOperator, Expression, IntegerLiteralKind, LiteralKind, PrefixOperator},
    source::Span,
    type_checking::{BuiltinType, CompilationUnitTypeError, TypeCheckingError, UnitTypeId},
};

use crate::type_checking::flow_effects::{
    logical_rhs_is_required,
    result_edges::{BooleanResultWrites, splits_boolean_writes},
};

use super::{
    BodyChecker, ExpressionCheck, UnitExpressionId,
    flow::{
        ConditionFacts, FlowKey, FlowVersions, extend_facts, intersect_facts, merge_flow_versions,
    },
    while_flow::{PendingBooleanEdges, PendingControlSite},
};

pub(super) struct BinaryExpression {
    pub(super) expression: crate::ast::ExpressionId,
    pub(super) left: crate::ast::ExpressionId,
    pub(super) operator: BinaryOperator,
    pub(super) operator_span: Span,
    pub(super) right: crate::ast::ExpressionId,
}

struct LogicalLhsSnapshot {
    facts: BTreeMap<FlowKey, UnitTypeId>,
    versions: FlowVersions,
}

struct BypassedWrites<'a> {
    before: &'a LogicalLhsSnapshot,
    keys: BTreeSet<FlowKey>,
}

pub(super) struct LogicalNormalWrites {
    body_path_reachable: bool,
    keys: BTreeSet<FlowKey>,
}

impl BodyChecker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_prefix(
        &mut self,
        source: SourceUnitId,
        operator: PrefixOperator,
        operator_span: Span,
        operand: crate::ast::ExpressionId,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        if operator == PrefixOperator::Minus {
            let operand_node = self
                .file(source)
                .ast()
                .expressions()
                .get(operand)
                .map_err(TypeCheckingError::from)?;
            if let Expression::Literal(LiteralKind::Integer(kind)) = operand_node.payload() {
                let operand_span = operand_node.span();
                if matches!(
                    kind,
                    IntegerLiteralKind::Unsigned | IntegerLiteralKind::UnsignedLong
                ) {
                    let ty = self.literal_type(
                        operand_span,
                        LiteralKind::Integer(*kind),
                        expected,
                        expected_span,
                        false,
                    )?;
                    self.record_expression(source, operand, ty);
                    self.emit_operand_error(operator_span, operand_span)?;
                    return Ok(ExpressionCheck {
                        ty: self.error_type(),
                        falls_through: true,
                    });
                }
                let ty = self.literal_type(
                    operand_span,
                    LiteralKind::Integer(*kind),
                    expected,
                    expected_span,
                    true,
                )?;
                self.record_expression(source, operand, ty);
                return Ok(ExpressionCheck {
                    ty,
                    falls_through: true,
                });
            }
        }
        let operand_result = self.check_expression(source, operand, None, None, return_type)?;
        let valid = match operator {
            PrefixOperator::Not => self.is_builtin(operand_result.ty, BuiltinType::Boolean),
            PrefixOperator::Plus | PrefixOperator::Minus => self.is_numeric(operand_result.ty),
        };
        if !valid && !self.is_error(operand_result.ty) {
            let operand_span = self
                .file(source)
                .ast()
                .expressions()
                .get(operand)
                .map_err(TypeCheckingError::from)?
                .span();
            self.emit_operand_error(operator_span, operand_span)?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: operand_result.falls_through,
            });
        }
        Ok(operand_result)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_binary(
        &mut self,
        source: SourceUnitId,
        expression: BinaryExpression,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
        normal_writes: Option<&mut Option<LogicalNormalWrites>>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let BinaryExpression {
            expression,
            left,
            operator,
            operator_span,
            right,
        } = expression;
        if operator == BinaryOperator::Elvis {
            return self.check_elvis(
                source,
                left,
                operator_span,
                right,
                expected,
                expected_span,
                return_type,
            );
        }
        if matches!(
            operator,
            BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr
        ) {
            return self.check_logical_binary(
                source,
                BinaryExpression {
                    expression,
                    left,
                    operator,
                    operator_span,
                    right,
                },
                return_type,
                normal_writes,
            );
        }
        self.check_value_binary(
            source,
            BinaryExpression {
                expression,
                left,
                operator,
                operator_span,
                right,
            },
            return_type,
        )
    }

    fn check_logical_binary(
        &mut self,
        source: SourceUnitId,
        expression: BinaryExpression,
        return_type: UnitTypeId,
        mut normal_writes: Option<&mut Option<LogicalNormalWrites>>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let ast = self.file(source).ast();
        let root_has_split =
            splits_boolean_writes(ast, expression.left).map_err(TypeCheckingError::from)?;
        let mut pending = Vec::new();
        let mut left = expression.left;
        while !self
            .parts
            .expression_types
            .contains_key(&UnitExpressionId::new(source, left))
        {
            let node = ast
                .expressions()
                .get(left)
                .map_err(TypeCheckingError::from)?;
            let &Expression::Binary {
                left: child,
                operator,
                operator_span,
                right,
            } = node.payload()
            else {
                break;
            };
            if !matches!(
                operator,
                BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr
            ) {
                break;
            }
            let has_split = splits_boolean_writes(ast, child).map_err(TypeCheckingError::from)?;
            pending.push((
                BinaryExpression {
                    expression: left,
                    left: child,
                    operator,
                    operator_span,
                    right,
                },
                has_split,
            ));
            left = child;
        }
        // Descending through a direct left spine performs no flow transitions.
        let before = (root_has_split || pending.iter().any(|(_, split)| *split)).then(|| {
            LogicalLhsSnapshot {
                facts: self.flow_facts.clone(),
                versions: self.flow_versions.clone(),
            }
        });
        let mut left_result = self.check_expression(source, left, None, None, return_type)?;
        // Keep the write summary only for this spine. Each completed node
        // becomes the left child of the next node in the fold.
        let mut prefix = None;
        let mut left_facts = self.condition_facts(source, left)?;
        let root = expression.expression;
        for (node, has_split) in pending
            .into_iter()
            .rev()
            .chain([(expression, root_has_split)])
        {
            let is_root = node.expression == root;
            let outcomes = &self.syntactic_boolean_outcomes[source.index()];
            let (yes, no) = outcomes.possible_results(node.left);
            let has_both_results = yes && no;
            let bypassed = if (has_both_results || prefix.is_some())
                && let Some(before) = before.as_ref().filter(|_| has_split)
                && self.is_builtin(left_result.ty, BuiltinType::Boolean)
                && self.flow_versions.versions_changed(&before.versions)
            {
                let flow_key = |target| self.stable_flow_key(source, target);
                let writes = match prefix.take() {
                    Some(writes) => writes,
                    None => BooleanResultWrites::collect(outcomes, node.left, flow_key)
                        .map_err(TypeCheckingError::from)?,
                };
                let keys = if has_both_results {
                    writes.bypassed(node.operator == BinaryOperator::LogicalAnd)
                } else {
                    BTreeSet::new()
                };
                if !is_root {
                    prefix = Some(
                        writes
                            .extend_prefix(outcomes, node.left, node.operator, node.right, flow_key)
                            .map_err(TypeCheckingError::from)?,
                    );
                }
                Some(BypassedWrites { before, keys })
            } else {
                prefix = None;
                None
            };
            let (id, operator, right) = (node.expression, node.operator, node.right);
            left_result = self.check_logical_binary_after_left(
                source,
                node,
                left_result,
                &left_facts,
                return_type,
                bypassed,
                if is_root { normal_writes.take() } else { None },
            )?;
            if !is_root {
                self.record_expression_check(UnitExpressionId::new(source, id), left_result);
                // The next node consumes this completed prefix as its left operand.
                let right_facts = self.condition_facts(source, right)?;
                left_facts = self.join_logical_condition_facts(
                    source,
                    operator,
                    left_facts,
                    right,
                    right_facts,
                )?;
            }
        }
        Ok(left_result)
    }

    #[allow(clippy::too_many_arguments)]
    fn check_logical_binary_after_left(
        &mut self,
        source: SourceUnitId,
        expression: BinaryExpression,
        left_result: ExpressionCheck,
        (left_true, left_false): &ConditionFacts,
        return_type: UnitTypeId,
        bypassed: Option<BypassedWrites<'_>>,
        normal_writes: Option<&mut Option<LogicalNormalWrites>>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let BinaryExpression {
            expression,
            left,
            operator,
            operator_span,
            right,
        } = expression;
        let baseline = self.flow_facts.clone();
        let baseline_versions = self.flow_versions.clone();
        let ast = self.file(source).ast();
        let rhs_required =
            logical_rhs_is_required(ast, left, operator).map_err(TypeCheckingError::from)?;
        let rhs_bypassed =
            self.syntactic_boolean_outcomes[source.index()].rhs_is_bypassed(left, operator);
        let left_edges = if self.has_pending_expression_exit(source, left, left_result)? {
            self.boolean_pending_edges(source, left, &baseline_versions)?
        } else {
            PendingBooleanEdges::default()
        };
        let (right_entry, rhs_pending, bypass) = if operator == BinaryOperator::LogicalAnd {
            (left_true, left_edges.yes, left_edges.no)
        } else {
            (left_false, left_edges.no, left_edges.yes)
        };
        self.flow_facts = extend_facts(&baseline, right_entry);
        if let Some(BypassedWrites { before, keys }) = bypassed {
            self.flow_versions.restore_bypassed_writes(
                &mut self.flow_facts,
                &before.facts,
                &before.versions,
                &keys,
            );
        }
        if let Some(pending) = &rhs_pending {
            self.flow_versions.set_pending_keys(pending.iter().copied());
        }
        let mut right_writes = None;
        let right_result = self.check_loop_paths(
            left_result.falls_through,
            rhs_pending.is_some(),
            |checker| {
                checker.check_expression_with_normal_writes(
                    source,
                    right,
                    None,
                    None,
                    return_type,
                    Some(&mut right_writes),
                )
            },
        );
        let right_versions = std::mem::replace(&mut self.flow_versions, baseline_versions);
        self.flow_facts = baseline;
        let writes = if rhs_bypassed {
            BTreeSet::new()
        } else if let Some(writes) =
            right_writes.take_if(|writes| writes.body_path_reachable == self.body_path_reachable)
        {
            // The suffix contains only effects from a normally completing RHS.
            // Its enclosing expression must also complete after typing it.
            let completes = self
                .normal_flow_expression_completes(source, right)
                .map_err(TypeCheckingError::from)?;
            #[cfg(test)]
            crate::type_checking::short_circuit_cost_tests::record_rhs_summary(true, completes);
            if completes {
                writes.keys
            } else {
                BTreeSet::new()
            }
        } else {
            #[cfg(test)]
            if right_writes.is_some() {
                crate::type_checking::short_circuit_cost_tests::record_rhs_summary(false, false);
            }
            self.expression_flow_writes(source, right)?
        };
        for &key in &writes {
            self.invalidate_flow_key(key);
        }
        let right_result = right_result?;
        let mut edges = if rhs_pending.is_some()
            && self.has_pending_expression_exit(source, right, right_result)?
        {
            self.boolean_pending_edges(source, right, &right_versions)?
        } else {
            PendingBooleanEdges::default()
        };
        if let Some(bypass) = bypass {
            let edge = if operator == BinaryOperator::LogicalAnd {
                &mut edges.no
            } else {
                &mut edges.yes
            };
            edge.get_or_insert_with(Default::default).extend(bypass);
        }
        self.flow_versions.set_pending_keys(
            edges
                .yes
                .iter()
                .chain(&edges.no)
                .flat_map(|keys| keys.iter().copied()),
        );
        let boolean = self.builtin(BuiltinType::Boolean);
        let ty = if self.is_builtin(left_result.ty, BuiltinType::Boolean)
            && self.is_builtin(right_result.ty, BuiltinType::Boolean)
        {
            boolean
        } else if self.is_error(left_result.ty) || self.is_error(right_result.ty) {
            self.error_type()
        } else if self.is_deferred(left_result.ty) || self.is_deferred(right_result.ty) {
            self.deferred_type(crate::type_checking::DeferredReason::ControlJoin)
        } else {
            self.emit_binary_operand_error(
                source,
                operator_span,
                left,
                left_result.ty,
                right,
                right_result.ty,
            )?;
            self.error_type()
        };
        let expression = UnitExpressionId::new(source, expression);
        self.note_pending_exit(
            PendingControlSite::Expression(expression),
            edges.yes.is_some() || edges.no.is_some(),
        );
        if ty == boolean {
            self.pending_boolean_edges.insert(expression, edges);
            if let Some(output) = normal_writes
                && !self.syntactic_boolean_outcomes[source.index()].may_write(left)
            {
                // A standalone scan starts outside any loop frame. With a
                // write-free left operand its only writes are from the RHS.
                *output = Some(LogicalNormalWrites {
                    body_path_reachable: self.body_path_reachable,
                    keys: writes,
                });
            }
        }
        Ok(ExpressionCheck {
            ty,
            falls_through: left_result.falls_through
                && (right_result.falls_through || !rhs_required),
        })
    }

    fn check_value_binary(
        &mut self,
        source: SourceUnitId,
        expression: BinaryExpression,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let BinaryExpression {
            expression,
            left,
            operator,
            operator_span,
            right,
        } = expression;
        let equality = matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual);
        let left_null = equality && self.is_null_comparison_literal(source, left)?;
        let right_null = equality && self.is_null_comparison_literal(source, right)?;
        let (left_result, right_result) = if left_null && !right_null {
            let right_result = self.check_expression(source, right, None, None, return_type)?;
            let expected = self.null_comparison_expected(source, right, right_result.ty);
            (
                self.check_expression(source, left, expected, None, return_type)?,
                right_result,
            )
        } else {
            let left_result = self.check_expression(source, left, None, None, return_type)?;
            let left_exits = self.has_pending_expression_exit(source, left, left_result)?;
            let expected = if right_null && !left_null {
                self.null_comparison_expected(source, left, left_result.ty)
            } else {
                None
            };
            let right_result =
                self.check_loop_paths(left_result.falls_through, left_exits, |checker| {
                    checker.check_expression(source, right, expected, None, return_type)
                })?;
            (left_result, right_result)
        };
        let pending_exit = self.has_pending_expression_exit(source, left, left_result)?
            && self.has_pending_expression_exit(source, right, right_result)?;
        self.note_pending_exit(
            PendingControlSite::Expression(UnitExpressionId::new(source, expression)),
            pending_exit,
        );
        if self.is_deferred(left_result.ty) || self.is_deferred(right_result.ty) {
            return Ok(ExpressionCheck {
                ty: self.deferred_type(crate::type_checking::DeferredReason::ControlJoin),
                falls_through: left_result.falls_through && right_result.falls_through,
            });
        }
        if let Some(result) = self.finish_integer_range_binary(
            source,
            expression,
            left,
            right,
            operator,
            operator_span,
            left_result,
            right_result,
        )? {
            return Ok(result);
        }
        let boolean = self.builtin(BuiltinType::Boolean);
        let result = match operator {
            BinaryOperator::Multiply
            | BinaryOperator::Divide
            | BinaryOperator::Remainder
            | BinaryOperator::Subtract => (left_result.ty == right_result.ty
                && self.is_numeric(left_result.ty))
            .then_some(left_result.ty),
            BinaryOperator::Add => (left_result.ty == right_result.ty
                && (self.is_numeric(left_result.ty)
                    || self.is_builtin(left_result.ty, BuiltinType::String)))
            .then_some(left_result.ty),
            BinaryOperator::BitwiseAnd
            | BinaryOperator::BitwiseXor
            | BinaryOperator::BitwiseOr
            | BinaryOperator::Shl
            | BinaryOperator::Shr
            | BinaryOperator::Ushr => (left_result.ty == right_result.ty
                && self.is_integer(left_result.ty))
            .then_some(left_result.ty),
            BinaryOperator::Less
            | BinaryOperator::Greater
            | BinaryOperator::LessEqual
            | BinaryOperator::GreaterEqual => (left_result.ty == right_result.ty
                && (self.is_numeric(left_result.ty)
                    || self.is_builtin(left_result.ty, BuiltinType::Char)))
            .then_some(boolean),
            BinaryOperator::Equal | BinaryOperator::NotEqual => (self
                .assignable(left_result.ty, right_result.ty)
                || self.assignable(right_result.ty, left_result.ty))
            .then_some(boolean),
            BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr => (self
                .is_builtin(left_result.ty, BuiltinType::Boolean)
                && self.is_builtin(right_result.ty, BuiltinType::Boolean))
            .then_some(boolean),
            BinaryOperator::InclusiveRange
            | BinaryOperator::ExclusiveRange
            | BinaryOperator::To
            | BinaryOperator::In
            | BinaryOperator::NotIn
            | BinaryOperator::Elvis => {
                Some(self.deferred_type(crate::type_checking::DeferredReason::Call))
            }
        };
        let ty = if let Some(result) = result {
            result
        } else if self.is_error(left_result.ty) || self.is_error(right_result.ty) {
            self.error_type()
        } else {
            self.emit_binary_operand_error(
                source,
                operator_span,
                left,
                left_result.ty,
                right,
                right_result.ty,
            )?;
            self.error_type()
        };
        Ok(ExpressionCheck {
            ty,
            falls_through: left_result.falls_through && right_result.falls_through,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn check_elvis(
        &mut self,
        source: SourceUnitId,
        left: crate::ast::ExpressionId,
        operator_span: Span,
        right: crate::ast::ExpressionId,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let left_result = self.check_expression(source, left, None, None, return_type)?;
        let (inner, left_can_bypass_right) = match self.signatures.types().get(left_result.ty) {
            Some(crate::type_checking::UnitTypeKind::Nullable(inner)) => {
                (Some(*inner), !self.is_builtin(*inner, BuiltinType::Nothing))
            }
            Some(crate::type_checking::UnitTypeKind::Error) => (None, false),
            Some(crate::type_checking::UnitTypeKind::Deferred(_)) => (None, true),
            _ => {
                let right_result =
                    self.check_expression(source, right, expected, expected_span, return_type)?;
                self.emit_binary_operand_error(
                    source,
                    operator_span,
                    left,
                    left_result.ty,
                    right,
                    right_result.ty,
                )?;
                return Ok(ExpressionCheck {
                    ty: self.error_type(),
                    falls_through: left_result.falls_through,
                });
            }
        };
        let right_expected = inner.filter(|inner| !self.is_builtin(*inner, BuiltinType::Nothing));
        let bypass =
            left_can_bypass_right.then(|| (self.flow_facts.clone(), self.flow_versions.clone()));
        let right_result = self.check_expression(
            source,
            right,
            right_expected.or(expected),
            expected_span,
            return_type,
        )?;
        if let Some((facts, versions)) = bypass {
            if self.has_pending_expression_exit(source, right, right_result)? {
                self.flow_facts = intersect_facts(&facts, &self.flow_facts);
                self.flow_versions = merge_flow_versions(&versions, &self.flow_versions);
            } else {
                self.flow_facts = facts;
                self.flow_versions = versions;
            }
        }
        let ty = if let Some(inner) = inner {
            if self.is_builtin(inner, BuiltinType::Nothing) {
                right_result.ty
            } else {
                inner
            }
        } else {
            self.deferred_type(crate::type_checking::DeferredReason::ControlJoin)
        };
        Ok(ExpressionCheck {
            ty,
            falls_through: left_result.falls_through
                && (left_can_bypass_right || right_result.falls_through),
        })
    }

    fn emit_operand_error(
        &mut self,
        operator_span: Span,
        operand_span: Span,
    ) -> Result<(), CompilationUnitTypeError> {
        self.emit_with_label_from_operator(
            codes::INVALID_OPERAND_TYPES,
            "operator does not accept this operand type",
            operator_span,
            operand_span,
            "invalid operand",
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_binary_operand_error(
        &mut self,
        source: SourceUnitId,
        operator_span: Span,
        left: crate::ast::ExpressionId,
        left_ty: UnitTypeId,
        right: crate::ast::ExpressionId,
        right_ty: UnitTypeId,
    ) -> Result<(), CompilationUnitTypeError> {
        let code = codes::catalog()?.resolve(codes::INVALID_OPERAND_TYPES)?;
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            code,
            "operator does not accept these operand types",
            operator_span,
        )?;
        diagnostic.add_label(
            self.sources,
            self.file(source)
                .ast()
                .expressions()
                .get(left)
                .map_err(TypeCheckingError::from)?
                .span(),
            format!("left operand has type {}", self.type_name(left_ty)),
        )?;
        diagnostic.add_label(
            self.sources,
            self.file(source)
                .ast()
                .expressions()
                .get(right)
                .map_err(TypeCheckingError::from)?
                .span(),
            format!("right operand has type {}", self.type_name(right_ty)),
        )?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    pub(super) fn is_integer(&self, ty: UnitTypeId) -> bool {
        matches!(
            self.signatures.types().get(ty),
            Some(crate::type_checking::UnitTypeKind::Builtin(
                BuiltinType::Byte
                    | BuiltinType::Short
                    | BuiltinType::Int
                    | BuiltinType::Long
                    | BuiltinType::UByte
                    | BuiltinType::UShort
                    | BuiltinType::UInt
                    | BuiltinType::ULong
            ))
        )
    }

    pub(super) fn is_numeric(&self, ty: UnitTypeId) -> bool {
        matches!(
            self.signatures.types().get(ty),
            Some(crate::type_checking::UnitTypeKind::Builtin(
                BuiltinType::Byte
                    | BuiltinType::Short
                    | BuiltinType::Int
                    | BuiltinType::Long
                    | BuiltinType::UByte
                    | BuiltinType::UShort
                    | BuiltinType::UInt
                    | BuiltinType::ULong
                    | BuiltinType::Float
                    | BuiltinType::Double
            ))
        )
    }

    fn emit_with_label_from_operator(
        &mut self,
        code: &str,
        message: &str,
        primary: Span,
        label: Span,
        label_message: &str,
    ) -> Result<(), CompilationUnitTypeError> {
        let code = codes::catalog()?.resolve(code)?;
        let mut diagnostic =
            Diagnostic::new(self.sources, Severity::Error, code, message, primary)?;
        diagnostic.add_label(self.sources, label, label_message)?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }
}
