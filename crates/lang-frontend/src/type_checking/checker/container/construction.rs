//! Sequential container construction and descriptor publication.
use super::*;

impl Checker<'_> {
    pub(super) fn check_list_form_construction(
        &mut self,
        call: ContainerCall<'_>,
        kind: SequentialContainerKind,
        expected: Option<TypeId>,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let ContainerCall {
            callee,
            span: call_span,
            type_arguments,
            arguments,
            ..
        } = call;
        if type_arguments.len() > 1 {
            self.emit(
                self.type_argument_arity_code,
                "list-form container construction accepts at most one type argument",
                call_span,
            )?;
            self.check_call_operands(callee, arguments)?;
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        if let Some(invalid) = arguments
            .iter()
            .find(|argument| argument.named_prefix.is_some() || argument.mode_marker.is_some())
        {
            let code = if invalid.named_prefix.is_some() {
                self.invalid_named_argument_code
            } else {
                self.call_argument_mode_code
            };
            self.emit(
                code,
                "list-form elements are repeated Value arguments",
                invalid.span,
            )?;
        }
        let explicit = type_arguments
            .first()
            .copied()
            .map(|type_ref| self.resolve_type_ref(type_ref))
            .transpose()?;
        let contextual = expected.and_then(|ty| {
            self.container_parts(ty)
                .filter(|(expected_kind, _)| *expected_kind == kind)
                .map(|(_, element)| element)
        });
        let mut reachable = true;
        let element = if let Some(element) = explicit.or(contextual) {
            element
        } else if let Some(first) = arguments.first() {
            let first_result = if matches!(
                self.ast().expressions().get(first.value)?.payload(),
                Expression::Literal(LiteralKind::Null)
            ) {
                let nothing = self.builtin(BuiltinType::Nothing);
                let nullable_nothing = self.types.intern(TypeKind::Nullable(nothing));
                self.check_pending_operand(
                    first.value,
                    Some(nullable_nothing),
                    None,
                    &mut reachable,
                )?
            } else {
                self.check_pending_operand(first.value, None, None, &mut reachable)?
            };
            if self.is_builtin(first_result.ty, BuiltinType::Nothing)
                || matches!(self.kind(first_result.ty), TypeKind::Nullable(inner) if self.is_builtin(*inner, BuiltinType::Nothing))
            {
                self.emit(
                    self.cannot_infer_container_element_code,
                    "cannot infer a storable element type from a bottom or null first element",
                    self.ast().expressions().get(first.value)?.span(),
                )?;
                self.error_type()
            } else {
                first_result.ty
            }
        } else {
            self.emit(
                self.cannot_infer_container_element_code,
                "empty list-form construction requires an expected or explicit element type",
                call_span,
            )?;
            self.error_type()
        };
        let valid_element = self.validate_construction_element(element, call_span)?;
        let mut valid = valid_element;
        for (index, argument) in arguments.iter().enumerate() {
            if index == 0 && explicit.is_none() && contextual.is_none() {
                valid &= !self.is_error(element);
                continue;
            }
            valid &= !self
                .check_pending_operand(argument.value, Some(element), None, &mut reachable)
                .map(|result| self.is_error(result.ty))?;
        }
        if arguments
            .iter()
            .any(|argument| argument.named_prefix.is_some() || argument.mode_marker.is_some())
        {
            valid = false;
        }
        self.finish_container_construction(
            call,
            CheckedConstruction {
                container: kind,
                shape: ContainerConstructionKind::ListForm,
                element,
                modes: vec![ParameterMode::Value; arguments.len()],
                valid,
            },
        )
    }

    pub(super) fn check_named_container_construction(
        &mut self,
        call: ContainerCall<'_>,
        kind: SequentialContainerKind,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let ContainerCall {
            callee,
            span: call_span,
            type_arguments,
            arguments,
            ..
        } = call;
        if type_arguments.len() != 1 {
            self.emit(
                self.type_argument_arity_code,
                "core container construction requires exactly one type argument",
                call_span,
            )?;
            self.check_call_operands(callee, arguments)?;
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        let element = self.resolve_type_ref(type_arguments[0])?;
        let mut valid = self.validate_construction_element(element, call_span)?;
        let (shape, modes): (ContainerConstructionKind, Vec<ParameterMode>) = match kind {
            SequentialContainerKind::Array | SequentialContainerKind::List => (
                ContainerConstructionKind::RuntimeLength,
                vec![ParameterMode::Borrow, ParameterMode::Borrow],
            ),
            SequentialContainerKind::MutableList => {
                (ContainerConstructionKind::EmptyMutableList, Vec::new())
            }
        };
        if arguments.len() != modes.len() {
            self.emit(
                self.invalid_container_construction_code,
                "core container construction has the wrong number of arguments",
                call_span,
            )?;
            self.check_call_operands(callee, arguments)?;
            valid = false;
        } else {
            let int = self.builtin(BuiltinType::Int);
            let initializer = self.types.intern(TypeKind::Function {
                move_only: false,
                parameters: vec![FunctionParameterType {
                    mode: ParameterMode::Borrow,
                    ty: int,
                }],
                return_type: element,
            });
            let mut reachable = true;
            for (index, argument) in arguments.iter().enumerate() {
                if argument.named_prefix.is_some()
                    || !matches!(
                        (argument.mode_marker, modes[index]),
                        (None, ParameterMode::Value | ParameterMode::Borrow)
                            | (Some(ParameterModeMarker::Borrow(_)), ParameterMode::Borrow)
                    )
                {
                    self.emit(
                        self.invalid_container_construction_code,
                        "core container argument does not match its fixed parameter contract",
                        argument.span,
                    )?;
                    valid = false;
                }
                let expected = if index == 0 { int } else { initializer };
                valid &= !self
                    .check_pending_operand(argument.value, Some(expected), None, &mut reachable)
                    .map(|result| self.is_error(result.ty))?;
            }
        }
        self.finish_container_construction(
            call,
            CheckedConstruction {
                container: kind,
                shape,
                element,
                modes,
                valid,
            },
        )
    }

    fn finish_container_construction(
        &mut self,
        call: ContainerCall<'_>,
        construction: CheckedConstruction,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let deferred = self.deferred(DeferredReason::Call);
        self.set_expression(call.callee, deferred);
        self.set_expression_category(call.callee, ExpressionCategory::Temporary);
        if !construction.valid
            || self.is_error(construction.element)
            || self.is_deferred(construction.element)
        {
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        let constructor = Self::container_constructor(construction.container);
        let container_type = self.types.intern(TypeKind::Intrinsic {
            constructor,
            arguments: vec![construction.element],
        });
        self.container_constructions
            .push(ContainerConstructionDescriptor::new(
                call.expression,
                construction.shape,
                construction.container,
                container_type,
                construction.element,
                construction.modes,
            ));
        Ok(ExprCheck {
            ty: container_type,
            falls_through: true,
        })
    }
}
