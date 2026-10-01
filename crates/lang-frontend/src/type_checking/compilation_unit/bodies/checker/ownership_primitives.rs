//! 原生所有权替换原语 `replace` 与 `swap` 的 compilation-unit 类型检查。

use crate::{
    ast::{ExpressionId, TypeRefId},
    diagnostic::codes,
    name_resolution::{ExternalSymbolId, Namespace, SourceUnitId, UnitReferenceTarget},
    parser::{CallArgument, Expression, ParameterModeMarker},
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypeError, ExpressionCategory, ExternalTypeBinding,
        IntrinsicCallable, ParameterMode, TypeCheckingError, UnitCallArgumentDescriptor,
        UnitCallDescriptor, UnitCallTarget, UnitCallableInstanceKey, UnitExpressionId,
        UnitFunctionParameterType, UnitTypeId, UnitTypeKind,
    },
};

use super::{BodyChecker, ExpressionCheck};

impl BodyChecker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_intrinsic_ownership_primitive_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
        return_type: UnitTypeId,
    ) -> Result<Option<ExpressionCheck>, CompilationUnitTypeError> {
        let node = self
            .file(source)
            .ast()
            .expressions()
            .get(callee)
            .map_err(TypeCheckingError::from)?;
        if !matches!(node.payload(), Expression::Name) {
            return Ok(None);
        }
        let callee_span = node.span();
        let value_target = self.reference(source, callee_span, Namespace::Value);
        let (external, callable) = match value_target {
            Some(UnitReferenceTarget::External(external)) => {
                match self.environment.binding(*external) {
                    Some(ExternalTypeBinding::IntrinsicCallable(
                        c @ (IntrinsicCallable::Replace | IntrinsicCallable::Swap),
                    )) => (*external, *c),
                    _ => return Ok(None),
                }
            }
            Some(UnitReferenceTarget::ExternalOverloadSet(externals)) => {
                let mut found = None;
                for external in externals {
                    if let Some(ExternalTypeBinding::IntrinsicCallable(
                        c @ (IntrinsicCallable::Replace | IntrinsicCallable::Swap),
                    )) = self.environment.binding(*external)
                    {
                        found = Some((*external, *c));
                        break;
                    }
                }
                match found {
                    Some(pair) => pair,
                    None => return Ok(None),
                }
            }
            _ => return Ok(None),
        };

        match callable {
            IntrinsicCallable::Replace => self
                .check_intrinsic_replace_call(
                    source,
                    expression,
                    call_span,
                    callee,
                    external,
                    type_arguments,
                    arguments,
                    return_type,
                )
                .map(Some),
            IntrinsicCallable::Swap => self
                .check_intrinsic_swap_call(
                    source,
                    expression,
                    call_span,
                    callee,
                    external,
                    type_arguments,
                    arguments,
                    return_type,
                )
                .map(Some),
            _ => Ok(None),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn check_intrinsic_replace_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        external: ExternalSymbolId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let mut valid = true;
        if type_arguments.len() > 1 {
            self.emit(
                codes::TYPE_ARGUMENT_ARITY,
                "intrinsic 'replace' accepts at most one type argument",
                call_span,
            )?;
            valid = false;
        }
        if arguments.len() != 2 {
            self.emit(
                codes::CALL_ARGUMENT_ARITY,
                "intrinsic 'replace' requires exactly 2 arguments",
                call_span,
            )?;
            for argument in arguments {
                self.check_expression(source, argument.value, None, None, return_type)?;
            }
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        for argument in arguments {
            if let Some(prefix) = argument.named_prefix {
                self.emit(
                    codes::INVALID_NAMED_ARGUMENT,
                    "intrinsic 'replace' does not accept named arguments",
                    prefix.name_span,
                )?;
                valid = false;
            }
        }
        let arg0 = &arguments[0];
        if !matches!(arg0.mode_marker, Some(ParameterModeMarker::Inout(_))) {
            self.emit(
                codes::CALL_ARGUMENT_MODE,
                "first argument to 'replace' must be prefixed with '&'",
                arg0.span,
            )?;
            valid = false;
        }
        let arg0_result = self.check_expression(source, arg0.value, None, None, return_type)?;
        if self.is_error(arg0_result.ty) {
            valid = false;
        }
        if !self.is_mutable_inout_place(source, arg0.value)? {
            self.emit(
                codes::CALL_ARGUMENT_MODE,
                "first argument to 'replace' must be a mutable place",
                arg0.span,
            )?;
            valid = false;
        }

        let explicit_t = type_arguments
            .first()
            .copied()
            .map(|t| self.resolve_body_type_ref(source, t))
            .transpose()?;

        let t = if let Some(explicit) = explicit_t {
            if !self.is_error(arg0_result.ty)
                && (!self.assignable(arg0_result.ty, explicit)
                    || !self.assignable(explicit, arg0_result.ty))
            {
                let primary = self
                    .file(source)
                    .ast()
                    .expressions()
                    .get(arg0.value)
                    .map_err(TypeCheckingError::from)?
                    .span();
                self.emit_maybe_label(
                    codes::TYPE_MISMATCH,
                    "expression type does not match the expected type",
                    primary,
                    None,
                    format!(
                        "expected {}, found {}",
                        self.type_name(explicit),
                        self.type_name(arg0_result.ty)
                    ),
                )?;
                valid = false;
            }
            explicit
        } else {
            arg0_result.ty
        };

        let arg1 = &arguments[1];
        if arg1.mode_marker.is_some() {
            self.emit(
                codes::CALL_ARGUMENT_MODE,
                "second argument to 'replace' must be passed by value",
                arg1.span,
            )?;
            valid = false;
        }
        let arg1_result = self.check_expression(source, arg1.value, Some(t), None, return_type)?;
        if self.is_error(arg1_result.ty) {
            valid = false;
        } else if !self.assignable(arg1_result.ty, t) {
            let primary = self
                .file(source)
                .ast()
                .expressions()
                .get(arg1.value)
                .map_err(TypeCheckingError::from)?
                .span();
            self.emit_maybe_label(
                codes::TYPE_MISMATCH,
                "expression type does not match the expected type",
                primary,
                None,
                format!(
                    "expected {}, found {}",
                    self.type_name(t),
                    self.type_name(arg1_result.ty)
                ),
            )?;
            valid = false;
        }

        if !valid || self.is_error(t) {
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }

        let function_type = self.signatures.types_mut().intern(UnitTypeKind::Function {
            move_only: false,
            parameters: vec![
                UnitFunctionParameterType::new(ParameterMode::Inout, t),
                UnitFunctionParameterType::new(ParameterMode::Value, t),
            ],
            return_type: t,
        });
        let callee_id = UnitExpressionId::new(source, callee);
        self.parts.expression_types.insert(callee_id, function_type);
        self.parts
            .expression_categories
            .insert(callee_id, ExpressionCategory::Temporary);
        let arg1_category = self.expression_category(source, arguments[1].value);
        self.parts.calls.push(UnitCallDescriptor {
            expression: UnitExpressionId::new(source, expression),
            instance: UnitCallableInstanceKey {
                target: UnitCallTarget::External(external),
                type_arguments: vec![t],
            },
            return_type: t,
            receiver: None,
            arguments: vec![
                UnitCallArgumentDescriptor {
                    argument_index: 0,
                    parameter_index: 0,
                    category: ExpressionCategory::Place,
                    mode: ParameterMode::Inout,
                    parameter_type: t,
                    cross_thread: false,
                },
                UnitCallArgumentDescriptor {
                    argument_index: 1,
                    parameter_index: 1,
                    category: arg1_category,
                    mode: ParameterMode::Value,
                    parameter_type: t,
                    cross_thread: false,
                },
            ],
            aborts: false,
            prints_line: false,
        });
        Ok(ExpressionCheck {
            ty: t,
            falls_through: true,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn check_intrinsic_swap_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        external: ExternalSymbolId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let mut valid = true;
        if type_arguments.len() > 1 {
            self.emit(
                codes::TYPE_ARGUMENT_ARITY,
                "intrinsic 'swap' accepts at most one type argument",
                call_span,
            )?;
            valid = false;
        }
        if arguments.len() != 2 {
            self.emit(
                codes::CALL_ARGUMENT_ARITY,
                "intrinsic 'swap' requires exactly 2 arguments",
                call_span,
            )?;
            for argument in arguments {
                self.check_expression(source, argument.value, None, None, return_type)?;
            }
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        for argument in arguments {
            if let Some(prefix) = argument.named_prefix {
                self.emit(
                    codes::INVALID_NAMED_ARGUMENT,
                    "intrinsic 'swap' does not accept named arguments",
                    prefix.name_span,
                )?;
                valid = false;
            }
        }
        let arg0 = &arguments[0];
        if !matches!(arg0.mode_marker, Some(ParameterModeMarker::Inout(_))) {
            self.emit(
                codes::CALL_ARGUMENT_MODE,
                "first argument to 'swap' must be prefixed with '&'",
                arg0.span,
            )?;
            valid = false;
        }
        let arg0_result = self.check_expression(source, arg0.value, None, None, return_type)?;
        if self.is_error(arg0_result.ty) {
            valid = false;
        }
        if !self.is_mutable_inout_place(source, arg0.value)? {
            self.emit(
                codes::CALL_ARGUMENT_MODE,
                "first argument to 'swap' must be a mutable place",
                arg0.span,
            )?;
            valid = false;
        }

        let arg1 = &arguments[1];
        if !matches!(arg1.mode_marker, Some(ParameterModeMarker::Inout(_))) {
            self.emit(
                codes::CALL_ARGUMENT_MODE,
                "second argument to 'swap' must be prefixed with '&'",
                arg1.span,
            )?;
            valid = false;
        }
        let arg1_result = self.check_expression(source, arg1.value, None, None, return_type)?;
        if self.is_error(arg1_result.ty) {
            valid = false;
        }
        if !self.is_mutable_inout_place(source, arg1.value)? {
            self.emit(
                codes::CALL_ARGUMENT_MODE,
                "second argument to 'swap' must be a mutable place",
                arg1.span,
            )?;
            valid = false;
        }

        let explicit_t = type_arguments
            .first()
            .copied()
            .map(|t| self.resolve_body_type_ref(source, t))
            .transpose()?;

        let t = if let Some(explicit) = explicit_t {
            if !self.is_error(arg0_result.ty)
                && (!self.assignable(arg0_result.ty, explicit)
                    || !self.assignable(explicit, arg0_result.ty))
            {
                let primary = self
                    .file(source)
                    .ast()
                    .expressions()
                    .get(arg0.value)
                    .map_err(TypeCheckingError::from)?
                    .span();
                self.emit_maybe_label(
                    codes::TYPE_MISMATCH,
                    "expression type does not match the expected type",
                    primary,
                    None,
                    format!(
                        "expected {}, found {}",
                        self.type_name(explicit),
                        self.type_name(arg0_result.ty)
                    ),
                )?;
                valid = false;
            }
            if !self.is_error(arg1_result.ty)
                && (!self.assignable(arg1_result.ty, explicit)
                    || !self.assignable(explicit, arg1_result.ty))
            {
                let primary = self
                    .file(source)
                    .ast()
                    .expressions()
                    .get(arg1.value)
                    .map_err(TypeCheckingError::from)?
                    .span();
                self.emit_maybe_label(
                    codes::TYPE_MISMATCH,
                    "expression type does not match the expected type",
                    primary,
                    None,
                    format!(
                        "expected {}, found {}",
                        self.type_name(explicit),
                        self.type_name(arg1_result.ty)
                    ),
                )?;
                valid = false;
            }
            explicit
        } else {
            if !self.is_error(arg0_result.ty)
                && !self.is_error(arg1_result.ty)
                && (!self.assignable(arg1_result.ty, arg0_result.ty)
                    || !self.assignable(arg0_result.ty, arg1_result.ty))
            {
                let primary = self
                    .file(source)
                    .ast()
                    .expressions()
                    .get(arg1.value)
                    .map_err(TypeCheckingError::from)?
                    .span();
                self.emit_maybe_label(
                    codes::TYPE_MISMATCH,
                    "expression type does not match the expected type",
                    primary,
                    None,
                    format!(
                        "expected {}, found {}",
                        self.type_name(arg0_result.ty),
                        self.type_name(arg1_result.ty)
                    ),
                )?;
                valid = false;
            }
            arg0_result.ty
        };

        let unit = self.builtin(BuiltinType::Unit);
        if !valid || self.is_error(t) {
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }

        let function_type = self.signatures.types_mut().intern(UnitTypeKind::Function {
            move_only: false,
            parameters: vec![
                UnitFunctionParameterType::new(ParameterMode::Inout, t),
                UnitFunctionParameterType::new(ParameterMode::Inout, t),
            ],
            return_type: unit,
        });
        let callee_id = UnitExpressionId::new(source, callee);
        self.parts.expression_types.insert(callee_id, function_type);
        self.parts
            .expression_categories
            .insert(callee_id, ExpressionCategory::Temporary);
        self.parts.calls.push(UnitCallDescriptor {
            expression: UnitExpressionId::new(source, expression),
            instance: UnitCallableInstanceKey {
                target: UnitCallTarget::External(external),
                type_arguments: vec![t],
            },
            return_type: unit,
            receiver: None,
            arguments: vec![
                UnitCallArgumentDescriptor {
                    argument_index: 0,
                    parameter_index: 0,
                    category: ExpressionCategory::Place,
                    mode: ParameterMode::Inout,
                    parameter_type: t,
                    cross_thread: false,
                },
                UnitCallArgumentDescriptor {
                    argument_index: 1,
                    parameter_index: 1,
                    category: ExpressionCategory::Place,
                    mode: ParameterMode::Inout,
                    parameter_type: t,
                    cross_thread: false,
                },
            ],
            aborts: false,
            prints_line: false,
        });
        Ok(ExpressionCheck {
            ty: unit,
            falls_through: true,
        })
    }
}
