use crate::{
    diagnostic::Diagnostic,
    name_resolution::{Namespace, ReferenceTarget},
    parser::{
        BinaryOperator, Expression, IntegerLiteralKind, LiteralKind, PrefixOperator, StringPart,
    },
    type_checking::RcOperationKind,
};

use super::{flow::extend_facts, *};

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
            Expression::Name => {
                if let Some(result) = self.check_bare_enum_construction(id, expected)? {
                    result
                } else {
                    ExprCheck {
                        ty: self.name_expression_type(id, span)?,
                        falls_through: true,
                    }
                }
            }
            Expression::This => ExprCheck {
                ty: self
                    .flow_facts
                    .get(&FlowKey::This)
                    .copied()
                    .unwrap_or_else(|| {
                        self.classifiers
                            .last()
                            .copied()
                            .unwrap_or_else(|| self.deferred(DeferredReason::ThisType))
                    }),
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
                keyword_span,
                subject,
                entries,
            } => self.check_when(id, keyword_span, subject, entries, expected, expected_span)?,
            Expression::Return {
                keyword_span,
                value,
            } => self.check_return(keyword_span, value)?,
            Expression::Break { keyword_span } => self.check_jump(
                keyword_span,
                "break is not inside an enclosing loop in this callable",
            )?,
            Expression::Continue { keyword_span } => self.check_jump(
                keyword_span,
                "continue is not inside an enclosing loop in this callable",
            )?,
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
            } => {
                self.check_expression(expression, None, None)?;
                self.resolve_type_ref(type_ref)?;
                ExprCheck {
                    ty: self.deferred(DeferredReason::CastOrTypeTest),
                    falls_through: true,
                }
            }
            Expression::TypeTest {
                expression,
                negated,
                operator_span,
                type_ref,
                ..
            } => self.check_type_test(expression, negated, operator_span, type_ref)?,
            Expression::Assignment {
                target,
                operator,
                operator_span,
                value,
            } => {
                let result = if let Some(result) =
                    self.check_container_assignment(target, operator, operator_span, value)?
                {
                    result
                } else {
                    self.check_expression(target, None, None)?;
                    self.check_expression(value, None, None)?;
                    if self.rc_operations.iter().any(|operation| {
                        operation.expression() == target
                            && operation.kind() == RcOperationKind::Value
                    }) {
                        self.emit(
                            self.immutable_container_place_code,
                            "Rc.value is read-only",
                            self.ast().expressions().get(target)?.span(),
                        )?;
                        ExprCheck {
                            ty: self.error_type(),
                            falls_through: true,
                        }
                    } else {
                        ExprCheck {
                            ty: self.deferred(DeferredReason::Assignment),
                            falls_through: true,
                        }
                    }
                };
                if let Some(key) = self.stable_flow_key(target) {
                    self.flow_facts.remove(&key);
                }
                result
            }
            Expression::Member {
                receiver,
                name_span,
                safe,
                ..
            } => {
                if let Some(result) = self.check_bare_enum_construction(id, expected)? {
                    result
                } else {
                    self.check_member(id, receiver, name_span, safe)?
                }
            }
            Expression::Call {
                callee,
                type_arguments,
                arguments,
                ..
            } => self.check_call(id, span, callee, type_arguments, arguments, expected)?,
            Expression::Index { receiver, index } => {
                self.check_container_index(id, receiver, index)?
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
        let category = self.classify_expression_category(id, result.ty);
        self.set_expression_category(id, category);
        self.set_expression(id, result.ty);
        Ok(result)
    }

    fn name_expression_type(
        &mut self,
        id: ExpressionId,
        span: Span,
    ) -> Result<TypeId, TypeCheckingError> {
        if let Some(key) = self.stable_flow_key(id)
            && let Some(ty) = self.flow_facts.get(&key).copied()
        {
            return Ok(ty);
        }
        let target = self.reference(span, Namespace::Value).cloned();
        match target {
            Some(ReferenceTarget::Symbol(symbol)) => Ok(self
                .symbol_type(symbol)
                .unwrap_or_else(|| self.deferred(DeferredReason::ForwardValueType))),
            Some(ReferenceTarget::External(external)) => self.external_type(external),
            Some(ReferenceTarget::OverloadSet(_) | ReferenceTarget::ExternalOverloadSet(_)) => {
                Ok(self.deferred(DeferredReason::OverloadSelection))
            }
            Some(ReferenceTarget::EnumCasePayloadCandidates(candidates)) => {
                self.resolve_payload_candidates(span, &candidates)
            }
            Some(ReferenceTarget::Unresolved | ReferenceTarget::LaterLocal(_)) | None => {
                Ok(self.error_type())
            }
        }
    }

    fn check_type_test(
        &mut self,
        expression: ExpressionId,
        _negated: bool,
        operator_span: Span,
        type_ref: TypeRefId,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let actual = self.check_expression(expression, None, None)?.ty;
        let target = self.resolve_type_test_ref(type_ref)?;
        let valid = if self.is_error(actual) || self.is_error(target) {
            true
        } else {
            self.valid_type_test_relation(actual, target)
        };
        if !valid {
            self.emit_with_label(
                self.invalid_type_test_code,
                "type test target is not runtime-testable from the operand type",
                operator_span,
                self.ast().type_refs().get(type_ref)?.span(),
                "invalid type-test target",
            )?;
        }
        Ok(ExprCheck {
            ty: self.builtin(BuiltinType::Boolean),
            falls_through: true,
        })
    }

    pub(super) fn valid_type_test_relation(&self, actual: TypeId, target: TypeId) -> bool {
        match self.kind(target) {
            TypeKind::EnumCase { root, .. } => match self.kind(actual) {
                TypeKind::EnumCase {
                    root: actual_root, ..
                } => actual_root == root,
                TypeKind::Nominal { .. } => actual == *root,
                TypeKind::Nullable(inner) => *inner == *root,
                _ => false,
            },
            TypeKind::Nominal { nominal, .. } => {
                let is_interface = self.nominals.iter().any(|descriptor| {
                    descriptor.id() == *nominal && descriptor.kind() == NominalKind::Interface
                });
                !is_interface
                    && (actual == target
                        || matches!(self.kind(actual), TypeKind::Nullable(inner) if *inner == target))
            }
            _ => false,
        }
    }

    fn check_member(
        &mut self,
        expression: ExpressionId,
        receiver_id: ExpressionId,
        name_span: Span,
        safe: bool,
    ) -> Result<ExprCheck, TypeCheckingError> {
        if let Some(ReferenceTarget::Symbol(symbol)) =
            self.reference(name_span, Namespace::Value).cloned()
            && self.enum_case_by_value_symbol.contains_key(&symbol)
        {
            let ty = self
                .symbol_type(symbol)
                .ok_or(TypeCheckingError::InvalidExternalBinding)?;
            return Ok(ExprCheck {
                ty,
                falls_through: true,
            });
        }
        let receiver = self.check_expression(receiver_id, None, None)?;
        if let Some(ty) =
            self.check_field_projection(expression, receiver_id, receiver.ty, name_span, safe)?
        {
            return Ok(ExprCheck {
                ty,
                falls_through: receiver.falls_through,
            });
        }
        let name = self.sources.slice(name_span)?;
        if let Some(ty) = self.rc_member_type(expression, receiver_id, receiver.ty, name, safe) {
            return Ok(ExprCheck {
                ty,
                falls_through: receiver.falls_through,
            });
        }
        if let Some(ty) = self.container_member_type(receiver.ty, name, name_span)? {
            return Ok(ExprCheck {
                ty,
                falls_through: receiver.falls_through,
            });
        }
        let candidates = self.payload_candidates_for_type(receiver.ty, name);
        if !candidates.is_empty() {
            self.emit_payload_access(name_span, &candidates)?;
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: receiver.falls_through,
            });
        }
        Ok(ExprCheck {
            ty: self.deferred(DeferredReason::MemberAccess),
            falls_through: receiver.falls_through,
        })
    }

    fn resolve_payload_candidates(
        &mut self,
        span: Span,
        candidates: &[SymbolId],
    ) -> Result<TypeId, TypeCheckingError> {
        let active = self.flow_facts.get(&FlowKey::This).and_then(|ty| {
            if let TypeKind::EnumCase { case, .. } = self.kind(*ty) {
                Some(*case)
            } else {
                None
            }
        });
        if let Some(active) = active
            && let Some(symbol) = candidates
                .iter()
                .find(|symbol| self.enum_case_by_payload_symbol.get(symbol) == Some(&active))
        {
            return self
                .symbol_type(*symbol)
                .ok_or(TypeCheckingError::InvalidExternalBinding);
        }
        self.emit_payload_access(span, candidates)?;
        Ok(self.error_type())
    }

    fn payload_candidates_for_type(&self, ty: TypeId, name: &str) -> Vec<SymbolId> {
        let root = match self.kind(ty) {
            TypeKind::Nominal { nominal, .. } => Some(*nominal),
            TypeKind::Nullable(inner) => match self.kind(*inner) {
                TypeKind::Nominal { nominal, .. } => Some(*nominal),
                _ => None,
            },
            _ => None,
        };
        self.enum_cases
            .iter()
            .filter(|case| Some(case.root()) == root)
            .flat_map(|case| case.payloads())
            .filter_map(|(symbol, _)| {
                (self.sources.slice(self.symbol_spans[symbol.index()]) == Ok(name))
                    .then_some(*symbol)
            })
            .collect()
    }

    fn emit_payload_access(
        &mut self,
        primary: Span,
        candidates: &[SymbolId],
    ) -> Result<(), TypeCheckingError> {
        let mut diagnostic = Diagnostic::new(
            self.sources,
            Severity::Error,
            self.invalid_enum_payload_access_code,
            "enum case payload is not uniquely available in the current flow",
            primary,
        )?;
        for symbol in candidates {
            let case = self
                .enum_case_by_payload_symbol
                .get(symbol)
                .and_then(|case| self.enum_case(*case))
                .ok_or(TypeCheckingError::InvalidExternalBinding)?;
            diagnostic.add_label(
                self.sources,
                self.symbol_spans[case.value_symbol().index()],
                "payload is declared by this case",
            )?;
        }
        self.diagnostics.push(diagnostic);
        Ok(())
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
            let structure_matches =
                move_only == move_span.is_some() && parameters.len() == parameter_spans.len();
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
                    self.set_parameter_mode(symbol, parameter.mode);
                }
            }
            self.callables.push(CallableContext {
                return_type,
                annotation_span: expected_span,
                loop_base: self.loop_depth,
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
                loop_base: self.loop_depth,
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
            loop_base: self.loop_depth,
        });
        self.check_value_body(body, None, None)?;
        self.callables.pop();
        Ok(ExprCheck {
            ty: error,
            falls_through: true,
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

    fn check_jump(
        &mut self,
        keyword_span: Span,
        message: &'static str,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let loop_base = self
            .callables
            .last()
            .map_or(0, |callable| callable.loop_base);
        let ty = if self.loop_depth > loop_base {
            self.builtin(BuiltinType::Nothing)
        } else {
            self.emit(self.jump_outside_loop_code, message, keyword_span)?;
            self.error_type()
        };
        Ok(ExprCheck {
            ty,
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
        if matches!(
            operator,
            BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr
        ) {
            let left_result = self.check_expression(left, None, None)?;
            let (left_true, left_false) = self.condition_facts(left)?;
            let baseline = self.flow_facts.clone();
            let right_entry = if operator == BinaryOperator::LogicalAnd {
                &left_true
            } else {
                &left_false
            };
            self.flow_facts = extend_facts(&baseline, right_entry);
            let right_result = self.check_expression(right, None, None)?;
            self.flow_facts = baseline;
            let valid = self.is_builtin(left_result.ty, BuiltinType::Boolean)
                && self.is_builtin(right_result.ty, BuiltinType::Boolean);
            let ty = if valid {
                self.builtin(BuiltinType::Boolean)
            } else if self.is_error(left_result.ty) || self.is_error(right_result.ty) {
                self.error_type()
            } else if self.is_deferred(left_result.ty) || self.is_deferred(right_result.ty) {
                self.deferred(DeferredReason::ControlJoin)
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
            return Ok(ExprCheck {
                ty,
                falls_through: left_result.falls_through && right_result.falls_through,
            });
        }
        let equality = matches!(operator, BinaryOperator::Equal | BinaryOperator::NotEqual);
        let left_null = equality
            && matches!(
                self.ast().expressions().get(left)?.payload(),
                Expression::Literal(LiteralKind::Null)
            );
        let right_null = equality
            && matches!(
                self.ast().expressions().get(right)?.payload(),
                Expression::Literal(LiteralKind::Null)
            );
        let (left_result, right_result) = if left_null && !right_null {
            let right_result = self.check_expression(right, None, None)?;
            let expected = matches!(self.kind(right_result.ty), TypeKind::Nullable(_))
                .then_some(right_result.ty);
            (self.check_expression(left, expected, None)?, right_result)
        } else if right_null && !left_null {
            let left_result = self.check_expression(left, None, None)?;
            let expected = matches!(self.kind(left_result.ty), TypeKind::Nullable(_))
                .then_some(left_result.ty);
            (left_result, self.check_expression(right, expected, None)?)
        } else {
            (
                self.check_expression(left, None, None)?,
                self.check_expression(right, None, None)?,
            )
        };
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

    pub(super) fn is_builtin(&self, ty: TypeId, expected: BuiltinType) -> bool {
        matches!(self.kind(ty), TypeKind::Builtin(actual) if *actual == expected)
    }

    pub(super) fn is_numeric(&self, ty: TypeId) -> bool {
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
