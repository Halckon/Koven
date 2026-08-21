use crate::{
    diagnostic::Diagnostic,
    name_resolution::{Namespace, ReferenceTarget},
    parser::{
        BinaryOperator, Expression, FloatLiteralKind, IntegerLiteralKind, LiteralKind,
        PrefixOperator, StringPart, WhenCondition,
    },
};

use super::*;

struct LambdaSyntax {
    span: Span,
    move_span: Option<Span>,
    parameter_spans: Vec<Span>,
    arrow_span: Option<Span>,
    body: StatementId,
}

impl Checker<'_> {
    pub(super) fn check_expression(
        &mut self,
        id: ExpressionId,
        expected: Option<TypeId>,
        expected_span: Option<Span>,
    ) -> Result<ExprCheck, TypeCheckingError> {
        if let Some(ty) = self.expression_types[id.index()] {
            return Ok(ExprCheck {
                ty,
                falls_through: !matches!(self.kind(ty), TypeKind::Builtin(BuiltinType::Nothing)),
            });
        }
        let node = self.ast().expressions().get(id)?;
        let span = node.span();
        let payload = node.payload().clone();
        let mut result = match payload {
            Expression::Error => ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            },
            Expression::Name => ExprCheck {
                ty: self.name_expression_type(span)?,
                falls_through: true,
            },
            Expression::This => ExprCheck {
                ty: self.deferred(DeferredReason::ThisType),
                falls_through: true,
            },
            Expression::Literal(literal) => {
                self.check_literal(id, span, literal, expected, expected_span)?
            }
            Expression::Group { expression } => {
                self.check_expression(expression, expected, expected_span)?
            }
            Expression::String { parts } => {
                for part in parts {
                    if let StringPart::Interpolation { expression, .. } = part {
                        self.check_expression(expression, None, None)?;
                    }
                }
                ExprCheck {
                    ty: self.builtin(BuiltinType::String),
                    falls_through: true,
                }
            }
            Expression::Lambda {
                move_span,
                parameters,
                arrow_span,
                body,
            } => self.check_lambda(
                LambdaSyntax {
                    span,
                    move_span,
                    parameter_spans: parameters,
                    arrow_span,
                    body,
                },
                expected,
                expected_span,
            )?,
            Expression::If {
                condition,
                then_branch,
                else_span,
                else_branch,
                ..
            } => self.check_if(
                condition,
                then_branch,
                else_span,
                else_branch,
                expected,
                expected_span,
            )?,
            Expression::When {
                subject, entries, ..
            } => {
                if let Some(subject) = subject {
                    self.check_expression(subject, None, None)?;
                }
                for entry in entries {
                    for condition in entry.conditions {
                        match condition {
                            WhenCondition::Expression(expression)
                            | WhenCondition::Contains { expression, .. } => {
                                self.check_expression(expression, None, None)?;
                            }
                            WhenCondition::TypeTest { type_ref, .. } => {
                                self.resolve_type_ref(type_ref)?;
                            }
                        }
                    }
                    self.check_value_body(entry.body, expected, expected_span)?;
                }
                ExprCheck {
                    ty: self.deferred(DeferredReason::WhenTyping),
                    falls_through: true,
                }
            }
            Expression::Return {
                keyword_span,
                value,
            } => self.check_return(keyword_span, value)?,
            Expression::Break { .. } | Expression::Continue { .. } => ExprCheck {
                ty: self.builtin(BuiltinType::Nothing),
                falls_through: false,
            },
            Expression::SuperMember { interface, .. } => {
                self.resolve_type_ref(interface)?;
                ExprCheck {
                    ty: self.deferred(DeferredReason::MemberAccess),
                    falls_through: true,
                }
            }
            Expression::Prefix {
                operator,
                operator_span,
                operand,
            } => self.check_prefix(operator, operator_span, operand, expected, expected_span)?,
            Expression::Binary {
                left,
                operator,
                operator_span,
                right,
            } => self.check_binary(
                left,
                operator,
                operator_span,
                right,
                expected,
                expected_span,
            )?,
            Expression::NonNullAssert {
                operand,
                operator_span,
            } => {
                let operand = self.check_expression(operand, None, None)?;
                let ty = match self.kind(operand.ty) {
                    TypeKind::Nullable(inner) => *inner,
                    TypeKind::Error => self.error_type(),
                    TypeKind::Deferred(_) => self.deferred(DeferredReason::MemberAccess),
                    _ => {
                        self.emit(
                            self.operands_code,
                            "non-null assertion requires a nullable operand",
                            operator_span,
                        )?;
                        self.error_type()
                    }
                };
                ExprCheck {
                    ty,
                    falls_through: operand.falls_through,
                }
            }
            Expression::Cast {
                expression,
                type_ref,
                ..
            }
            | Expression::TypeTest {
                expression,
                type_ref,
                ..
            } => {
                self.check_expression(expression, None, None)?;
                self.resolve_type_ref(type_ref)?;
                ExprCheck {
                    ty: self.deferred(DeferredReason::CastOrTypeTest),
                    falls_through: true,
                }
            }
            Expression::Assignment { target, value, .. } => {
                self.check_expression(target, None, None)?;
                self.check_expression(value, None, None)?;
                ExprCheck {
                    ty: self.deferred(DeferredReason::Assignment),
                    falls_through: true,
                }
            }
            Expression::Member { receiver, .. } => {
                self.check_expression(receiver, None, None)?;
                ExprCheck {
                    ty: self.deferred(DeferredReason::MemberAccess),
                    falls_through: true,
                }
            }
            Expression::Call {
                callee,
                type_arguments,
                arguments,
                ..
            } => {
                self.check_expression(callee, None, None)?;
                for type_argument in type_arguments {
                    self.resolve_type_ref(type_argument)?;
                }
                for argument in arguments {
                    self.check_expression(argument.value, None, None)?;
                }
                ExprCheck {
                    ty: self.deferred(DeferredReason::Call),
                    falls_through: true,
                }
            }
            Expression::Index { receiver, index } => {
                self.check_expression(receiver, None, None)?;
                self.check_expression(index, None, None)?;
                ExprCheck {
                    ty: self.deferred(DeferredReason::Index),
                    falls_through: true,
                }
            }
            Expression::Propagate { value, .. } => {
                self.check_expression(value, None, None)?;
                ExprCheck {
                    ty: self.deferred(DeferredReason::ErrorPropagation),
                    falls_through: true,
                }
            }
            Expression::CallableReference { receiver, .. } => {
                if let Some(receiver) = receiver {
                    self.check_expression(receiver, None, None)?;
                }
                ExprCheck {
                    ty: self.deferred(DeferredReason::OverloadSelection),
                    falls_through: true,
                }
            }
        };
        if let Some(expected) = expected
            && !self.assignable(result.ty, expected)
            && !self.is_deferred(result.ty)
        {
            self.mismatch(span, expected_span, result.ty, expected)?;
            result.ty = self.error_type();
        }
        self.set_expression(id, result.ty);
        Ok(result)
    }

    fn name_expression_type(&mut self, span: Span) -> Result<TypeId, TypeCheckingError> {
        let target = self.reference(span, Namespace::Value).cloned();
        match target {
            Some(ReferenceTarget::Symbol(symbol)) => Ok(self
                .symbol_type(symbol)
                .unwrap_or_else(|| self.deferred(DeferredReason::ForwardValueType))),
            Some(ReferenceTarget::External(external)) => self.external_type(external),
            Some(ReferenceTarget::OverloadSet(_) | ReferenceTarget::ExternalOverloadSet(_)) => {
                Ok(self.deferred(DeferredReason::OverloadSelection))
            }
            Some(ReferenceTarget::Unresolved | ReferenceTarget::LaterLocal(_)) | None => {
                Ok(self.error_type())
            }
        }
    }

    fn check_literal(
        &mut self,
        id: ExpressionId,
        span: Span,
        literal: LiteralKind,
        expected: Option<TypeId>,
        expected_span: Option<Span>,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let ty = match literal {
            LiteralKind::Integer(kind) => {
                self.check_integer_literal(id, span, kind, expected, expected_span, false)?
            }
            LiteralKind::Float(kind) => self.check_float_literal(span, kind, expected_span)?,
            LiteralKind::Char => self.builtin(BuiltinType::Char),
            LiteralKind::Boolean(_) => self.builtin(BuiltinType::Boolean),
            LiteralKind::Null => match expected.map(|ty| self.kind(ty).clone()) {
                Some(TypeKind::Nullable(_)) => expected.expect("matched Some"),
                Some(_) => {
                    let nothing = self.builtin(BuiltinType::Nothing);
                    self.types.intern(TypeKind::Nullable(nothing))
                }
                None => {
                    self.emit(
                        self.cannot_infer_code,
                        "cannot infer the type of null without a nullable expected type",
                        span,
                    )?;
                    self.error_type()
                }
            },
        };
        Ok(ExprCheck {
            ty,
            falls_through: true,
        })
    }

    fn check_integer_literal(
        &mut self,
        id: ExpressionId,
        span: Span,
        kind: IntegerLiteralKind,
        expected: Option<TypeId>,
        expected_span: Option<Span>,
        negative: bool,
    ) -> Result<TypeId, TypeCheckingError> {
        let Some(magnitude) = self.integer_magnitude(span, kind)? else {
            self.numeric_range_error(span, expected_span)?;
            let error = self.error_type();
            self.set_expression(id, error);
            return Ok(error);
        };
        let expected_builtin = expected.and_then(|ty| match self.kind(ty) {
            TypeKind::Builtin(builtin) => Some(*builtin),
            _ => None,
        });
        let selected = match kind {
            IntegerLiteralKind::Unsuffixed => {
                if let Some(expected) = expected_builtin.filter(|ty| is_signed_integer(*ty)) {
                    fits_signed(magnitude, expected, negative).then_some(expected)
                } else if fits_signed(magnitude, BuiltinType::Int, negative) {
                    Some(BuiltinType::Int)
                } else if fits_signed(magnitude, BuiltinType::Long, negative) {
                    Some(BuiltinType::Long)
                } else {
                    None
                }
            }
            IntegerLiteralKind::Long => {
                fits_signed(magnitude, BuiltinType::Long, negative).then_some(BuiltinType::Long)
            }
            IntegerLiteralKind::Unsigned => {
                if negative {
                    None
                } else if let Some(expected) =
                    expected_builtin.filter(|ty| is_unsigned_integer(*ty))
                {
                    fits_unsigned(magnitude, expected).then_some(expected)
                } else if fits_unsigned(magnitude, BuiltinType::UInt) {
                    Some(BuiltinType::UInt)
                } else if fits_unsigned(magnitude, BuiltinType::ULong) {
                    Some(BuiltinType::ULong)
                } else {
                    None
                }
            }
            IntegerLiteralKind::UnsignedLong => (!negative
                && fits_unsigned(magnitude, BuiltinType::ULong))
            .then_some(BuiltinType::ULong),
        };
        let ty = if let Some(selected) = selected {
            self.builtin(selected)
        } else {
            self.numeric_range_error(span, expected_span)?;
            self.error_type()
        };
        self.set_expression(id, ty);
        Ok(ty)
    }

    fn check_float_literal(
        &mut self,
        span: Span,
        kind: FloatLiteralKind,
        expected_span: Option<Span>,
    ) -> Result<TypeId, TypeCheckingError> {
        let text = self.sources.slice(span)?;
        let number = match kind {
            FloatLiteralKind::Double => text,
            FloatLiteralKind::Float => &text[..text.len() - 1],
        };
        let finite = match kind {
            FloatLiteralKind::Double => number.parse::<f64>().is_ok_and(f64::is_finite),
            FloatLiteralKind::Float => number.parse::<f32>().is_ok_and(f32::is_finite),
        };
        if !finite {
            self.numeric_range_error(span, expected_span)?;
            return Ok(self.error_type());
        }
        Ok(self.builtin(match kind {
            FloatLiteralKind::Double => BuiltinType::Double,
            FloatLiteralKind::Float => BuiltinType::Float,
        }))
    }

    fn integer_magnitude(
        &self,
        span: Span,
        kind: IntegerLiteralKind,
    ) -> Result<Option<u128>, TypeCheckingError> {
        let text = self.sources.slice(span)?;
        let suffix_len = match kind {
            IntegerLiteralKind::Unsuffixed => 0,
            IntegerLiteralKind::Long | IntegerLiteralKind::Unsigned => 1,
            IntegerLiteralKind::UnsignedLong => 2,
        };
        Ok(text[..text.len() - suffix_len].parse::<u128>().ok())
    }

    fn numeric_range_error(
        &mut self,
        span: Span,
        expected_span: Option<Span>,
    ) -> Result<(), TypeCheckingError> {
        if let Some(expected) = expected_span {
            self.emit_with_label(
                self.numeric_range_code,
                "numeric literal is outside the representable range",
                span,
                expected,
                "expected type introduced here",
            )
        } else {
            self.emit(
                self.numeric_range_code,
                "numeric literal is outside the representable range",
                span,
            )
        }
    }

    fn check_lambda(
        &mut self,
        syntax: LambdaSyntax,
        expected: Option<TypeId>,
        expected_span: Option<Span>,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let LambdaSyntax {
            span,
            move_span,
            parameter_spans,
            arrow_span,
            body,
        } = syntax;
        let expected_function = expected.and_then(|ty| match self.kind(ty).clone() {
            TypeKind::Function {
                move_only,
                parameters,
                return_type,
            } => Some((ty, move_only, parameters, return_type)),
            _ => None,
        });
        if let Some((function, move_only, parameters, return_type)) = expected_function {
            let structure_matches = move_only == move_span.is_some()
                && parameters.len() == parameter_spans.len()
                && parameters
                    .iter()
                    .all(|parameter| parameter.mode == ParameterMode::Value);
            if !structure_matches {
                self.emit_with_label(
                    self.mismatch_code,
                    "lambda structure does not match the expected function type",
                    arrow_span.or(move_span).unwrap_or(span),
                    expected_span.unwrap_or(span),
                    "expected function type introduced here",
                )?;
                self.check_value_body(body, None, None)?;
                return Ok(ExprCheck {
                    ty: self.error_type(),
                    falls_through: true,
                });
            }
            for (&parameter_span, parameter) in parameter_spans.iter().zip(&parameters) {
                if let Some(symbol) = self.symbol_at(parameter_span) {
                    self.set_symbol(symbol, parameter.ty);
                }
            }
            self.callables.push(CallableContext {
                return_type,
                annotation_span: expected_span,
            });
            let body_result = self.check_value_body(body, Some(return_type), expected_span)?;
            self.callables.pop();
            return Ok(ExprCheck {
                ty: function,
                falls_through: body_result.falls_through,
            });
        }
        if parameter_spans.is_empty() {
            let return_type = self.deferred(DeferredReason::ControlJoin);
            self.callables.push(CallableContext {
                return_type,
                annotation_span: None,
            });
            let body = self.check_value_body(body, None, None)?;
            self.callables.pop();
            let ty = if self.is_error(body.ty) || self.is_deferred(body.ty) {
                body.ty
            } else {
                self.types.intern(TypeKind::Function {
                    move_only: move_span.is_some(),
                    parameters: Vec::new(),
                    return_type: body.ty,
                })
            };
            return Ok(ExprCheck {
                ty,
                falls_through: body.falls_through,
            });
        }
        self.emit(
            self.cannot_infer_code,
            "lambda parameter types require an expected function type",
            arrow_span.unwrap_or(span),
        )?;
        let error = self.error_type();
        for parameter_span in parameter_spans {
            if let Some(symbol) = self.symbol_at(parameter_span) {
                self.set_symbol(symbol, error);
            }
        }
        self.callables.push(CallableContext {
            return_type: error,
            annotation_span: None,
        });
        self.check_value_body(body, None, None)?;
        self.callables.pop();
        Ok(ExprCheck {
            ty: error,
            falls_through: true,
        })
    }

    fn check_if(
        &mut self,
        condition: ExpressionId,
        then_branch: StatementId,
        else_span: Option<Span>,
        else_branch: Option<StatementId>,
        expected: Option<TypeId>,
        expected_span: Option<Span>,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let boolean = self.builtin(BuiltinType::Boolean);
        self.check_expression(condition, Some(boolean), None)?;
        let then_result = self.check_value_body(then_branch, expected, expected_span)?;
        let Some(else_branch) = else_branch else {
            return Ok(ExprCheck {
                ty: self.builtin(BuiltinType::Unit),
                falls_through: true,
            });
        };
        let else_result = self.check_value_body(else_branch, expected, expected_span)?;
        let ty = if self.is_deferred(then_result.ty) || self.is_deferred(else_result.ty) {
            self.deferred(DeferredReason::ControlJoin)
        } else if let Some(join) = self.join(then_result.ty, else_result.ty) {
            join
        } else {
            let primary = else_span.unwrap_or(self.ast().statements().get(else_branch)?.span());
            let first = self.ast().statements().get(then_branch)?.span();
            self.emit_with_label(
                self.branch_type_code,
                "control branches do not have a common type",
                primary,
                first,
                format!("first branch has type {}", self.type_name(then_result.ty)),
            )?;
            self.error_type()
        };
        Ok(ExprCheck {
            ty,
            falls_through: then_result.falls_through || else_result.falls_through,
        })
    }

    fn check_return(
        &mut self,
        keyword_span: Span,
        value: Option<ExpressionId>,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let Some(callable) = self.callables.last().copied() else {
            if let Some(value) = value {
                self.check_expression(value, None, None)?;
            }
            self.emit(
                self.return_outside_code,
                "return is not inside a callable",
                keyword_span,
            )?;
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: false,
            });
        };
        let unit = self.builtin(BuiltinType::Unit);
        match value {
            None if !self.assignable(unit, callable.return_type)
                && !self.is_deferred(callable.return_type) =>
            {
                self.return_shape_error(keyword_span, callable)?;
            }
            Some(value) if self.is_unit(callable.return_type) => {
                self.check_expression(value, None, None)?;
                let primary = self.ast().expressions().get(value)?.span();
                self.return_shape_error(primary, callable)?;
            }
            Some(value) => {
                let expected = (!self.is_deferred(callable.return_type)
                    && !self.is_error(callable.return_type))
                .then_some(callable.return_type);
                self.check_expression(value, expected, callable.annotation_span)?;
            }
            None => {}
        }
        Ok(ExprCheck {
            ty: self.builtin(BuiltinType::Nothing),
            falls_through: false,
        })
    }

    fn return_shape_error(
        &mut self,
        primary: Span,
        callable: CallableContext,
    ) -> Result<(), TypeCheckingError> {
        if let Some(label) = callable.annotation_span {
            self.emit_with_label(
                self.return_shape_code,
                "return value presence does not match the callable return type",
                primary,
                label,
                format!("callable returns {}", self.type_name(callable.return_type)),
            )
        } else {
            self.emit(
                self.return_shape_code,
                "return value presence does not match the callable return type",
                primary,
            )
        }
    }

    fn check_prefix(
        &mut self,
        operator: PrefixOperator,
        operator_span: Span,
        operand: ExpressionId,
        expected: Option<TypeId>,
        expected_span: Option<Span>,
    ) -> Result<ExprCheck, TypeCheckingError> {
        if operator == PrefixOperator::Minus {
            let operand_node = self.ast().expressions().get(operand)?;
            let operand_span = operand_node.span();
            let payload = operand_node.payload().clone();
            if let Expression::Literal(LiteralKind::Integer(kind)) = payload {
                if matches!(
                    kind,
                    IntegerLiteralKind::Unsigned | IntegerLiteralKind::UnsignedLong
                ) {
                    self.check_integer_literal(
                        operand,
                        operand_span,
                        kind,
                        expected,
                        expected_span,
                        false,
                    )?;
                    self.emit_operand_error(operator_span, operand_span)?;
                    return Ok(ExprCheck {
                        ty: self.error_type(),
                        falls_through: true,
                    });
                }
                let ty = self.check_integer_literal(
                    operand,
                    operand_span,
                    kind,
                    expected,
                    expected_span,
                    true,
                )?;
                return Ok(ExprCheck {
                    ty,
                    falls_through: true,
                });
            }
        }
        let operand_result = self.check_expression(operand, None, None)?;
        let valid = match operator {
            PrefixOperator::Not => self.is_builtin(operand_result.ty, BuiltinType::Boolean),
            PrefixOperator::Plus | PrefixOperator::Minus => self.is_numeric(operand_result.ty),
        };
        if !valid && !self.is_error(operand_result.ty) && !self.is_deferred(operand_result.ty) {
            let operand_span = self.ast().expressions().get(operand)?.span();
            self.emit_operand_error(operator_span, operand_span)?;
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: operand_result.falls_through,
            });
        }
        Ok(ExprCheck {
            ty: operand_result.ty,
            falls_through: operand_result.falls_through,
        })
    }

    fn check_binary(
        &mut self,
        left: ExpressionId,
        operator: BinaryOperator,
        operator_span: Span,
        right: ExpressionId,
        expected: Option<TypeId>,
        expected_span: Option<Span>,
    ) -> Result<ExprCheck, TypeCheckingError> {
        if operator == BinaryOperator::Elvis {
            return self.check_elvis(left, operator_span, right, expected, expected_span);
        }
        let left_result = self.check_expression(left, None, None)?;
        let right_result = self.check_expression(right, None, None)?;
        if self.is_deferred(left_result.ty) || self.is_deferred(right_result.ty) {
            return Ok(ExprCheck {
                ty: self.deferred(DeferredReason::ControlJoin),
                falls_through: left_result.falls_through && right_result.falls_through,
            });
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
            | BinaryOperator::Elvis => Some(self.deferred(DeferredReason::Call)),
        };
        let ty = if let Some(result) = result {
            result
        } else if self.is_error(left_result.ty) || self.is_error(right_result.ty) {
            self.error_type()
        } else {
            self.emit_binary_operand_error(
                operator_span,
                left,
                left_result.ty,
                right,
                right_result.ty,
            )?;
            self.error_type()
        };
        Ok(ExprCheck {
            ty,
            falls_through: left_result.falls_through && right_result.falls_through,
        })
    }

    fn check_elvis(
        &mut self,
        left: ExpressionId,
        operator_span: Span,
        right: ExpressionId,
        expected: Option<TypeId>,
        expected_span: Option<Span>,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let left_result = self.check_expression(left, None, None)?;
        let inner = match self.kind(left_result.ty) {
            TypeKind::Nullable(inner) => Some(*inner),
            TypeKind::Error | TypeKind::Deferred(_) => None,
            _ => {
                let right_result = self.check_expression(right, expected, expected_span)?;
                self.emit_binary_operand_error(
                    operator_span,
                    left,
                    left_result.ty,
                    right,
                    right_result.ty,
                )?;
                return Ok(ExprCheck {
                    ty: self.error_type(),
                    falls_through: true,
                });
            }
        };
        let right_expected = inner
            .filter(|inner| !matches!(self.kind(*inner), TypeKind::Builtin(BuiltinType::Nothing)));
        let right_result =
            self.check_expression(right, right_expected.or(expected), expected_span)?;
        let ty = if let Some(inner) = inner {
            if matches!(self.kind(inner), TypeKind::Builtin(BuiltinType::Nothing)) {
                right_result.ty
            } else {
                inner
            }
        } else {
            self.deferred(DeferredReason::ControlJoin)
        };
        Ok(ExprCheck {
            ty,
            falls_through: left_result.falls_through && right_result.falls_through,
        })
    }

    fn emit_operand_error(
        &mut self,
        operator_span: Span,
        operand_span: Span,
    ) -> Result<(), TypeCheckingError> {
        self.emit_with_label(
            self.operands_code,
            "operator does not accept this operand type",
            operator_span,
            operand_span,
            "invalid operand",
        )
    }

    fn emit_binary_operand_error(
        &mut self,
        operator_span: Span,
        left: ExpressionId,
        left_ty: TypeId,
        right: ExpressionId,
        right_ty: TypeId,
    ) -> Result<(), TypeCheckingError> {
        let mut diagnostic = Diagnostic::new(
            self.sources,
            crate::diagnostic::Severity::Error,
            self.operands_code,
            "operator does not accept these operand types",
            operator_span,
        )?;
        diagnostic.add_label(
            self.sources,
            self.ast().expressions().get(left)?.span(),
            format!("left operand has type {}", self.type_name(left_ty)),
        )?;
        diagnostic.add_label(
            self.sources,
            self.ast().expressions().get(right)?.span(),
            format!("right operand has type {}", self.type_name(right_ty)),
        )?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    fn is_builtin(&self, ty: TypeId, expected: BuiltinType) -> bool {
        matches!(self.kind(ty), TypeKind::Builtin(actual) if *actual == expected)
    }

    fn is_numeric(&self, ty: TypeId) -> bool {
        matches!(
            self.kind(ty),
            TypeKind::Builtin(
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
            )
        )
    }
}

fn is_signed_integer(ty: BuiltinType) -> bool {
    matches!(
        ty,
        BuiltinType::Byte | BuiltinType::Short | BuiltinType::Int | BuiltinType::Long
    )
}

fn is_unsigned_integer(ty: BuiltinType) -> bool {
    matches!(
        ty,
        BuiltinType::UByte | BuiltinType::UShort | BuiltinType::UInt | BuiltinType::ULong
    )
}

fn fits_signed(magnitude: u128, ty: BuiltinType, negative: bool) -> bool {
    let (positive_max, negative_max) = match ty {
        BuiltinType::Byte => (i8::MAX as u128, (i8::MAX as u128) + 1),
        BuiltinType::Short => (i16::MAX as u128, (i16::MAX as u128) + 1),
        BuiltinType::Int => (i32::MAX as u128, (i32::MAX as u128) + 1),
        BuiltinType::Long => (i64::MAX as u128, (i64::MAX as u128) + 1),
        _ => return false,
    };
    magnitude <= if negative { negative_max } else { positive_max }
}

fn fits_unsigned(magnitude: u128, ty: BuiltinType) -> bool {
    magnitude
        <= match ty {
            BuiltinType::UByte => u8::MAX as u128,
            BuiltinType::UShort => u16::MAX as u128,
            BuiltinType::UInt => u32::MAX as u128,
            BuiltinType::ULong => u64::MAX as u128,
            _ => return false,
        }
}
