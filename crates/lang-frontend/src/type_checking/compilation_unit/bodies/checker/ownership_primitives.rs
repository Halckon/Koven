//! 原生所有权替换原语 `replace` 与 `swap` 的 compilation-unit 类型检查。

use crate::{
    ast::{ExpressionId, TypeRefId},
    diagnostic::codes,
    name_resolution::{ExternalSymbolId, Namespace, SourceUnitId, UnitReferenceTarget},
    parser::{CallArgument, Expression, ParameterModeMarker},
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypeError, DeferredReason, ExpressionCategory,
        ExternalTypeBinding, IntrinsicCallable, OwnershipPrimitiveKind, ParameterMode,
        TypeCheckingError, UnitCallArgumentDescriptor, UnitCallDescriptor, UnitCallTarget,
        UnitCallableInstanceKey, UnitExpressionId, UnitFunctionParameterType,
        UnitOwnershipPrimitiveDescriptor, UnitTypeId, UnitTypeKind,
    },
};

use super::{BodyChecker, ExpressionCheck, while_flow::PendingControlSite};

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

    // A member of a temporary has no stable owner for replace's exclusive place.
    fn primitive_field_has_temporary_receiver(
        &self,
        source: SourceUnitId,
        mut expression: ExpressionId,
    ) -> Result<bool, CompilationUnitTypeError> {
        let mut field = false;
        loop {
            match self
                .file(source)
                .ast()
                .expressions()
                .get(expression)
                .map_err(TypeCheckingError::from)?
                .payload()
            {
                Expression::Group { expression: inner } => expression = *inner,
                Expression::Member { receiver, .. } => {
                    field = true;
                    expression = *receiver;
                }
                _ => {
                    return Ok(field
                        && self.expression_category(source, expression)
                            == ExpressionCategory::Temporary);
                }
            }
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
        let deferred = self.deferred_facts.len();
        let arg0_result = self.check_expression(source, arg0.value, None, None, return_type)?;
        let first_exits = self.has_pending_expression_exit(source, arg0.value, arg0_result)?;
        let arg0_id = UnitExpressionId::new(source, arg0.value);
        let old_pending = self.replacement_value_pending(arg0_id, arg0_result.ty, deferred);
        if self.is_error(arg0_result.ty) {
            valid = false;
        }
        if self.primitive_field_has_temporary_receiver(source, arg0.value)?
            || !self.is_mutable_inout_place(source, arg0.value)?
        {
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
                && !self.is_deferred(arg0_result.ty)
                && !self.is_deferred(explicit)
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
        let before_new = self.deferred_facts.len();
        let expected = (!self.is_deferred(t)).then_some(t);
        let arg1_result = self.check_loop_reachable(first_exits, |checker| {
            checker.check_expression(source, arg1.value, expected, None, return_type)
        })?;
        if self.is_error(arg1_result.ty) {
            valid = false;
        } else if !self.is_deferred(arg1_result.ty)
            && !self.is_deferred(t)
            && !self.assignable(arg1_result.ty, t)
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
                    self.type_name(t),
                    self.type_name(arg1_result.ty)
                ),
            )?;
            valid = false;
        }

        let expression_id = UnitExpressionId::new(source, expression);
        let normal_exit =
            first_exits && self.has_pending_expression_exit(source, arg1.value, arg1_result)?;
        self.note_pending_exit(PendingControlSite::Expression(expression_id), normal_exit);
        let deferred_operand = self.is_deferred(t)
            || self.is_deferred(arg0_result.ty)
            || self.is_deferred(arg1_result.ty);
        let valid = valid && !self.is_error(t);
        let pending_operands = deferred_operand
            || valid && self.primitive_operands_pending(source, arg0, arg1, deferred);
        if valid
            && normal_exit
            && pending_operands
            && self.call_argument_could_match(arg1_result.ty, arg0_result.ty, true)
        {
            let new_pending = self.replacement_value_pending(
                UnitExpressionId::new(source, arg1.value),
                arg1_result.ty,
                before_new,
            ) || self.contains_unresolved_call_type(arg0_result.ty);
            if let Some(key) = self.pending_flow_key(source, arg0.value) {
                self.flow_versions.replace_pending(key, new_pending);
            }
            // The old value is the result. Evaluating the new value can still
            // block publication of the call even when the old value is known.
            self.replacement_values.insert(expression_id, old_pending);
        }
        if deferred_operand || !valid {
            let ty = if deferred_operand {
                self.deferred_type(DeferredReason::Call)
            } else {
                self.error_type()
            };
            return Ok(ExpressionCheck {
                ty,
                falls_through: true,
            });
        }
        if pending_operands {
            self.note_pending_storage_value(expression_id, t);
            return Ok(ExpressionCheck {
                ty: t,
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
            text_primitive: None,
            expression: UnitExpressionId::new(source, expression),
            instance: UnitCallableInstanceKey {
                target: UnitCallTarget::External(external),
                type_arguments: vec![t],
            },
            return_type: t,
            receiver: None,
            result_source: crate::type_checking::CallableResultSource::Owned,
            range_construction: None,
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
        self.parts
            .ownership_primitives
            .push(UnitOwnershipPrimitiveDescriptor::new(
                UnitExpressionId::new(source, expression),
                OwnershipPrimitiveKind::Replace,
                t,
                [
                    UnitExpressionId::new(source, arg0.value),
                    UnitExpressionId::new(source, arg1.value),
                ],
            ));
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
        let deferred = self.deferred_facts.len();
        let arg0_result = self.check_expression(source, arg0.value, None, None, return_type)?;
        let first_exits = self.has_pending_expression_exit(source, arg0.value, arg0_result)?;
        let arg0_pending = self.replacement_value_pending(
            UnitExpressionId::new(source, arg0.value),
            arg0_result.ty,
            deferred,
        );
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
        let before_second = self.deferred_facts.len();
        let arg1_result = self.check_loop_reachable(first_exits, |checker| {
            checker.check_expression(source, arg1.value, None, None, return_type)
        })?;
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
                && !self.is_deferred(arg0_result.ty)
                && !self.is_deferred(explicit)
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
                && !self.is_deferred(arg1_result.ty)
                && !self.is_deferred(explicit)
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
                && !self.is_deferred(arg0_result.ty)
                && !self.is_error(arg1_result.ty)
                && !self.is_deferred(arg1_result.ty)
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
        let expression_id = UnitExpressionId::new(source, expression);
        let normal_exit =
            first_exits && self.has_pending_expression_exit(source, arg1.value, arg1_result)?;
        self.note_pending_exit(PendingControlSite::Expression(expression_id), normal_exit);
        let deferred_operand = self.is_deferred(t)
            || self.is_deferred(arg0_result.ty)
            || self.is_deferred(arg1_result.ty);
        let valid = valid && !self.is_error(t);
        let pending_operands = deferred_operand
            || valid && self.primitive_operands_pending(source, arg0, arg1, deferred);
        if valid
            && normal_exit
            && pending_operands
            && (self.types_could_be_equal(arg0_result.ty, arg1_result.ty)
                || self.assignable(arg0_result.ty, arg1_result.ty)
                    && self.assignable(arg1_result.ty, arg0_result.ty))
        {
            let arg1_pending = self.replacement_value_pending(
                UnitExpressionId::new(source, arg1.value),
                arg1_result.ty,
                before_second,
            );
            if let Some(first) = self.pending_flow_key(source, arg0.value) {
                let pending = arg1_pending || self.contains_unresolved_call_type(arg0_result.ty);
                self.flow_versions.replace_pending(first, pending);
            }
            if let Some(second) = self.pending_flow_key(source, arg1.value) {
                let pending = arg0_pending || self.contains_unresolved_call_type(arg1_result.ty);
                self.flow_versions.replace_pending(second, pending);
            }
        }
        if deferred_operand || !valid {
            let ty = if deferred_operand {
                self.deferred_type(DeferredReason::Call)
            } else {
                self.error_type()
            };
            return Ok(ExpressionCheck {
                ty,
                falls_through: true,
            });
        }
        if pending_operands {
            self.note_pending_storage_value(expression_id, unit);
            return Ok(ExpressionCheck {
                ty: unit,
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
            text_primitive: None,
            expression: UnitExpressionId::new(source, expression),
            instance: UnitCallableInstanceKey {
                target: UnitCallTarget::External(external),
                type_arguments: vec![t],
            },
            return_type: unit,
            receiver: None,
            result_source: crate::type_checking::CallableResultSource::Owned,
            range_construction: None,
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
        self.parts
            .ownership_primitives
            .push(UnitOwnershipPrimitiveDescriptor::new(
                UnitExpressionId::new(source, expression),
                OwnershipPrimitiveKind::Swap,
                t,
                [
                    UnitExpressionId::new(source, arg0.value),
                    UnitExpressionId::new(source, arg1.value),
                ],
            ));
        Ok(ExpressionCheck {
            ty: unit,
            falls_through: true,
        })
    }

    fn primitive_operands_pending(
        &self,
        source: SourceUnitId,
        first: &CallArgument,
        second: &CallArgument,
        deferred: usize,
    ) -> bool {
        [first, second].iter().any(|operand| {
            self.storage_value_is_pending(UnitExpressionId::new(source, operand.value))
        }) || self.has_unresolved_argument_facts_since(deferred)
    }
}
