//! Compilation-unit expression checking and expected-type propagation.

use crate::{
    ast::ExpressionId,
    name_resolution::SourceUnitId,
    parser::{Expression, LiteralKind, StringPart},
    source::Span,
    type_checking::{BuiltinType, CompilationUnitTypeError, TypeCheckingError, UnitTypeId},
};

use super::{
    BodyChecker, ExpressionCheck, ExpressionExpected, UnitExpressionId, assignment, operators,
};

impl BodyChecker<'_> {
    pub(super) fn check_expression(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        self.check_expression_with_expectation(
            source,
            expression,
            expected.map(ExpressionExpected::Canonical),
            expected_span,
            return_type,
        )
    }

    pub(super) fn check_expression_with_expectation(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        expectation: Option<ExpressionExpected<'_>>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let key = UnitExpressionId::new(source, expression);
        if let Some(ty) = self.parts.expression_types.get(&key).copied() {
            let falls_through = self
                .parts
                .expression_falls_through
                .get(&key)
                .copied()
                .unwrap_or_else(|| !self.is_builtin(ty, BuiltinType::Nothing));
            return Ok(ExpressionCheck { ty, falls_through });
        }
        let node = self
            .file(source)
            .ast()
            .expressions()
            .get(expression)
            .map_err(TypeCheckingError::from)?;
        let span = node.span();
        let payload = node.payload().clone();
        let expected = match expectation {
            Some(expectation) => expectation.context_for(self, &payload)?,
            None => None,
        };
        let mut result = match payload {
            Expression::Error => ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            },
            Expression::Name => {
                match self.check_bare_enum_construction(
                    source,
                    expression,
                    span,
                    expectation,
                    expected_span,
                )? {
                    Some(result) => result,
                    None => ExpressionCheck {
                        ty: self.name_type(source, expression, span)?,
                        falls_through: true,
                    },
                }
            }
            Expression::This => ExpressionCheck {
                ty: self.this_type(span)?,
                falls_through: true,
            },
            Expression::Member {
                receiver,
                name_span,
                safe,
                ..
            } => {
                match self.check_bare_enum_construction(
                    source,
                    expression,
                    name_span,
                    expectation,
                    expected_span,
                )? {
                    Some(result) => result,
                    None => self.check_member(
                        source,
                        expression,
                        receiver,
                        name_span,
                        safe,
                        return_type,
                    )?,
                }
            }
            Expression::Literal(LiteralKind::Null) => ExpressionCheck {
                ty: self.null_literal_type(span, expectation)?,
                falls_through: true,
            },
            Expression::Literal(literal) => ExpressionCheck {
                ty: self.literal_type(span, literal, expected, expected_span, false)?,
                falls_through: true,
            },
            Expression::String { parts } => {
                if let Some(diagnostic) =
                    crate::type_checking::string_length::check_literal(self.sources, span, &parts)?
                {
                    self.diagnostics.push(diagnostic);
                }
                for part in parts {
                    if let StringPart::Interpolation { expression, .. } = part {
                        self.check_expression(source, expression, None, None, return_type)?;
                    }
                }
                ExpressionCheck {
                    ty: self.builtin(BuiltinType::String),
                    falls_through: true,
                }
            }
            Expression::Group { expression } => {
                let result = self.check_expression_with_expectation(
                    source,
                    expression,
                    expectation,
                    expected_span,
                    return_type,
                )?;
                if self.storage_value_is_pending(UnitExpressionId::new(source, expression)) {
                    self.note_pending_storage_value(key, result.ty);
                }
                result
            }
            Expression::Call {
                callee,
                type_arguments,
                arguments,
                ..
            } => self.check_call_expression(
                source,
                expression,
                span,
                callee,
                &type_arguments,
                &arguments,
                expectation,
                expected_span,
                return_type,
            )?,
            Expression::Return {
                keyword_span,
                value,
            } => self.check_return(source, keyword_span, value, return_type)?,
            Expression::Break { keyword_span } => self.check_loop_jump(
                keyword_span,
                super::while_flow::LoopJump::Break,
                "break is not inside an enclosing loop in this callable",
            )?,
            Expression::Continue { keyword_span } => self.check_loop_jump(
                keyword_span,
                super::while_flow::LoopJump::Continue,
                "continue is not inside an enclosing loop in this callable",
            )?,
            Expression::Prefix {
                operator,
                operator_span,
                operand,
            } => self.check_prefix(
                source,
                operator,
                operator_span,
                operand,
                expected,
                expected_span,
                return_type,
            )?,
            Expression::Binary {
                left,
                operator,
                operator_span,
                right,
            } => self.check_binary(
                source,
                operators::BinaryExpression {
                    expression,
                    left,
                    operator,
                    operator_span,
                    right,
                },
                expected,
                expected_span,
                return_type,
            )?,
            Expression::If {
                condition,
                then_branch,
                else_span,
                else_branch,
                ..
            } => self.check_if(
                key,
                condition,
                then_branch,
                else_span,
                else_branch,
                expected,
                expected_span,
                return_type,
            )?,
            Expression::When {
                keyword_span,
                subject,
                entries,
            } => self.check_when(
                source,
                expression,
                keyword_span,
                subject,
                &entries,
                expected,
                expected_span,
                return_type,
            )?,
            Expression::Lambda {
                move_span,
                opener_span,
                parameters,
                arrow_span,
                body,
            } => self.check_lambda(
                key,
                span,
                move_span,
                opener_span,
                &parameters,
                arrow_span,
                body,
                expectation,
                expected_span,
            )?,
            Expression::TypeTest {
                expression,
                operator_span,
                type_ref,
                ..
            } => self.check_type_test(source, expression, operator_span, type_ref, return_type)?,
            Expression::SuperMember { interface, .. } => {
                self.check_super_member(source, interface)?
            }
            Expression::NonNullAssert {
                operand,
                operator_span,
            } => {
                self.check_non_null_assert(source, expression, operand, operator_span, return_type)?
            }
            Expression::Cast {
                expression,
                type_ref,
                ..
            } => self.check_cast(source, expression, type_ref, return_type)?,
            Expression::Propagate { value, .. } => {
                self.check_propagate(source, value, return_type)?
            }
            Expression::CallableReference { receiver, .. } => {
                self.check_callable_reference(source, receiver, return_type)?
            }
            Expression::Assignment {
                target,
                operator,
                operator_span,
                value,
            } => self.check_assignment(
                source,
                assignment::AssignmentExpression {
                    expression,
                    target,
                    operator,
                    operator_span,
                    value,
                },
                return_type,
            )?,
            Expression::Index { receiver, index } => {
                self.check_index(key, receiver, index, return_type)?
            }
        };
        // A non-completing expression retains its static shape but delivers no value.
        if result.falls_through
            && let Some(expectation) = expected.map(ExpressionExpected::Canonical).or(expectation)
        {
            result.ty = self.apply_expression_expectation(
                key,
                span,
                result.ty,
                expectation,
                expected_span,
            )?;
        }
        self.record_nullable_facts(source, expression, result.ty);
        self.record_expression(source, expression, result.ty);
        self.parts
            .expression_falls_through
            .insert(key, result.falls_through);
        Ok(result)
    }
}
