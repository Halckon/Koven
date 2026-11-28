//! Compilation-unit expression checking and expected-type propagation.

use crate::{
    ast::ExpressionId,
    diagnostic::codes,
    name_resolution::SourceUnitId,
    parser::{Expression, LiteralKind, StringPart},
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypeError, TypeCheckingError, UnitCallableSignature,
        UnitDisplayTarget, UnitInterpolationPart, UnitTypeId, UnitTypeKind, display,
    },
};

use super::{
    BodyChecker, ExpressionCheck, ExpressionExpected, UnitExpressionId, assignment,
    call_operand_flow::CallOperandFlow, operators, while_flow::PendingControlSite,
};

impl BodyChecker<'_> {
    pub(super) fn display_requirement(&self) -> Option<&UnitCallableSignature> {
        let declaration = self.names.builtin_display_declaration()?;
        self.signatures
            .declaration(declaration)?
            .nominal()?
            .members()
            .iter()
            .find(|member| member.name() == "toString")
    }

    pub(super) fn is_display_interface(&self, ty: UnitTypeId) -> bool {
        display::is_interface(
            self.signatures.types(),
            ty,
            self.names.builtin_display_declaration(),
        )
    }

    fn display_target(
        &mut self,
        ty: UnitTypeId,
    ) -> Result<Option<UnitDisplayTarget>, CompilationUnitTypeError> {
        if let Some(UnitTypeKind::Builtin(builtin)) = self.signatures.types().get(ty) {
            return Ok(
                display::builtin_witness(*builtin).then_some(UnitDisplayTarget::Builtin(*builtin))
            );
        }
        let Some(signature) = self
            .names
            .builtin_display_declaration()
            .and_then(|id| self.signatures.declaration(id))
        else {
            return Ok(None);
        };
        let interface = signature.ty();
        let requirement = self
            .display_requirement()
            .map(UnitCallableSignature::target);
        if !self.satisfies_interface(ty, interface)? {
            return Ok(None);
        }
        Ok(Some(UnitDisplayTarget::Requirement(requirement.ok_or(
            CompilationUnitTypeError::MissingDeclarationSymbol,
        )?)))
    }

    fn check_interpolated_string(
        &mut self,
        key: UnitExpressionId,
        parts: Vec<StringPart>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let source = key.source_unit();
        let mut flow = CallOperandFlow::new(true);
        let mut plan = Vec::new();
        let mut resolved = true;
        let has_interpolation = parts
            .iter()
            .any(|part| matches!(part, StringPart::Interpolation { .. }));
        for part in parts {
            let reachable = flow.falls_through();
            match part {
                StringPart::Error(_) => resolved = false,
                StringPart::Text(span) => {
                    if has_interpolation && reachable {
                        plan.push(UnitInterpolationPart::Text(span));
                    }
                }
                StringPart::Interpolation { expression, .. } => {
                    let result = flow.check(self, source, expression, None, None, return_type)?;
                    let operand = UnitExpressionId::new(source, expression);
                    let step = if self.is_error(result.ty)
                        || self.contains_unresolved_call_type(result.ty)
                    {
                        None
                    } else if !result.falls_through {
                        Some(UnitInterpolationPart::Exit(operand))
                    } else if let Some(target) = self.display_target(result.ty)? {
                        Some(UnitInterpolationPart::Format { operand, target })
                    } else {
                        let span = self
                            .file(source)
                            .ast()
                            .expressions()
                            .get(expression)
                            .map_err(TypeCheckingError::from)?
                            .span();
                        self.emit(
                            codes::INVALID_OPERAND_TYPES,
                            "string interpolation requires a Display value",
                            span,
                        )?;
                        None
                    };
                    if reachable {
                        if let Some(step) = step {
                            plan.push(step);
                        } else {
                            resolved = false;
                        }
                    }
                }
            }
        }
        if has_interpolation && resolved {
            self.parts.interpolation_plans.insert(key, plan);
        }
        self.note_pending_exit(PendingControlSite::Expression(key), flow.has_pending_exit());
        Ok(ExpressionCheck {
            ty: self.builtin(BuiltinType::String),
            falls_through: flow.falls_through(),
        })
    }

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
        self.check_expression_with_normal_writes(
            source,
            expression,
            expectation,
            expected_span,
            return_type,
            None,
        )
    }

    pub(super) fn check_expression_with_normal_writes(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        expectation: Option<ExpressionExpected<'_>>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
        normal_writes: Option<&mut Option<operators::LogicalNormalWrites>>,
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
        // A grouped operand forwards the same context without keeping another
        // full payload dispatch frame on the stack.
        let mut result = if let Expression::Group { expression: inner } = payload {
            let result = self.check_expression_with_normal_writes(
                source,
                inner,
                expectation,
                expected_span,
                return_type,
                normal_writes,
            )?;
            if self.storage_value_is_pending(UnitExpressionId::new(source, inner)) {
                self.note_pending_storage_value(key, result.ty);
            }
            result
        } else {
            self.check_expression_payload(
                key,
                span,
                payload,
                expectation,
                expected,
                expected_span,
                return_type,
                normal_writes,
            )?
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
        self.record_expression_check(key, result);
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    fn check_expression_payload(
        &mut self,
        key: UnitExpressionId,
        span: Span,
        payload: Expression,
        expectation: Option<ExpressionExpected<'_>>,
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
        normal_writes: Option<&mut Option<operators::LogicalNormalWrites>>,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let source = key.source_unit();
        let expression = key.expression();
        Ok(match payload {
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
                self.check_interpolated_string(key, parts, return_type)?
            }
            Expression::Group { .. } => unreachable!("groups are checked before payload dispatch"),
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
                key,
                keyword_span,
                super::while_flow::LoopJump::Break,
                "break is not inside an enclosing loop in this callable",
            )?,
            Expression::Continue { keyword_span } => self.check_loop_jump(
                key,
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
                normal_writes,
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
        })
    }

    pub(super) fn record_expression_check(
        &mut self,
        key: UnitExpressionId,
        result: ExpressionCheck,
    ) {
        self.record_nullable_facts(key.source_unit(), key.expression(), result.ty);
        self.record_expression(key.source_unit(), key.expression(), result.ty);
        self.parts
            .expression_falls_through
            .insert(key, result.falls_through);
    }
}
