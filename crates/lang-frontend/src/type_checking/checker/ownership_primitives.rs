//! 原生所有权替换原语 `replace` 与 `swap` 的类型检查。

use crate::{
    ast::{ExpressionId, TypeRefId},
    name_resolution::{ExternalSymbolId, Namespace, ReferenceTarget},
    parser::{CallArgument, Expression, ParameterModeMarker},
    source::Span,
    type_checking::{
        BuiltinType, CallArgumentDescriptor, CallDescriptor, CallableTarget, ExpressionCategory,
        ExternalTypeBinding, FunctionParameterType, IntrinsicCallable, ParameterMode, TypeKind,
    },
};

use super::{Checker, ExprCheck, TypeCheckingError};

impl Checker<'_> {
    pub(super) fn check_intrinsic_ownership_primitive_call(
        &mut self,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
    ) -> Result<Option<ExprCheck>, TypeCheckingError> {
        let callee_node = self.ast().expressions().get(callee)?;
        if !matches!(callee_node.payload(), Expression::Name) {
            return Ok(None);
        }
        let callee_span = callee_node.span();
        let (external, callable) = match self.reference(callee_span, Namespace::Value).cloned() {
            Some(ReferenceTarget::External(external)) => match self.environment.binding(external) {
                Some(ExternalTypeBinding::IntrinsicCallable(
                    c @ (IntrinsicCallable::Replace | IntrinsicCallable::Swap),
                )) => (external, *c),
                _ => return Ok(None),
            },
            Some(ReferenceTarget::ExternalOverloadSet(externals)) => {
                let mut found = None;
                for external in externals {
                    if let Some(ExternalTypeBinding::IntrinsicCallable(
                        c @ (IntrinsicCallable::Replace | IntrinsicCallable::Swap),
                    )) = self.environment.binding(external)
                    {
                        found = Some((external, *c));
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
                    expression,
                    call_span,
                    callee,
                    external,
                    type_arguments,
                    arguments,
                )
                .map(Some),
            IntrinsicCallable::Swap => self
                .check_intrinsic_swap_call(
                    expression,
                    call_span,
                    callee,
                    external,
                    type_arguments,
                    arguments,
                )
                .map(Some),
            _ => Ok(None),
        }
    }

    fn check_intrinsic_replace_call(
        &mut self,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        external: ExternalSymbolId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
    ) -> Result<ExprCheck, TypeCheckingError> {
        let mut valid = true;
        if type_arguments.len() > 1 {
            self.emit(
                self.type_argument_arity_code,
                "intrinsic 'replace' accepts at most one type argument",
                call_span,
            )?;
            valid = false;
        }
        if arguments.len() != 2 {
            self.emit(
                self.call_argument_arity_code,
                "intrinsic 'replace' requires exactly 2 arguments",
                call_span,
            )?;
            for argument in arguments {
                self.check_expression(argument.value, None, None)?;
            }
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        for argument in arguments {
            if let Some(prefix) = argument.named_prefix {
                self.emit(
                    self.invalid_named_argument_code,
                    "intrinsic 'replace' does not accept named arguments",
                    prefix.name_span,
                )?;
                valid = false;
            }
        }
        let arg0 = &arguments[0];
        if !matches!(arg0.mode_marker, Some(ParameterModeMarker::Inout(_))) {
            self.emit(
                self.call_argument_mode_code,
                "first argument to 'replace' must be prefixed with '&'",
                arg0.span,
            )?;
            valid = false;
        }
        let arg0_result = self.check_expression(arg0.value, None, None)?;
        if self.is_error(arg0_result.ty) {
            valid = false;
        }
        if self.expression_categories[arg0.value.index()] != ExpressionCategory::Place
            || self.is_mutable_element_place(arg0.value) == Some(false)
            || self.is_read_only_container_size(arg0.value)?
        {
            self.emit(
                self.call_argument_mode_code,
                "first argument to 'replace' must be a mutable place",
                arg0.span,
            )?;
            valid = false;
        }

        let explicit_t = type_arguments
            .first()
            .copied()
            .map(|t| self.resolve_type_ref(t))
            .transpose()?;

        let t = if let Some(explicit) = explicit_t {
            if !self.is_error(arg0_result.ty)
                && (!self.assignable(arg0_result.ty, explicit)
                    || !self.assignable(explicit, arg0_result.ty))
            {
                self.mismatch(
                    self.ast().expressions().get(arg0.value)?.span(),
                    None,
                    arg0_result.ty,
                    explicit,
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
                self.call_argument_mode_code,
                "second argument to 'replace' must be passed by value",
                arg1.span,
            )?;
            valid = false;
        }
        let arg1_result = self.check_expression(arg1.value, Some(t), None)?;
        if self.is_error(arg1_result.ty) {
            valid = false;
        } else if !self.assignable(arg1_result.ty, t) {
            self.mismatch(
                self.ast().expressions().get(arg1.value)?.span(),
                None,
                arg1_result.ty,
                t,
            )?;
            valid = false;
        }

        if !valid || self.is_error(t) {
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }

        let function = self.types.intern(TypeKind::Function {
            move_only: false,
            parameters: vec![
                FunctionParameterType {
                    mode: ParameterMode::Inout,
                    ty: t,
                },
                FunctionParameterType {
                    mode: ParameterMode::Value,
                    ty: t,
                },
            ],
            return_type: t,
        });
        self.set_expression(callee, function);
        self.set_expression_category(callee, ExpressionCategory::Temporary);
        self.calls.push(CallDescriptor::new(
            expression,
            CallableTarget::External(external),
            vec![t],
            t,
            None,
            vec![
                CallArgumentDescriptor::new(
                    0,
                    0,
                    ExpressionCategory::Place,
                    ParameterMode::Inout,
                    t,
                    false,
                ),
                CallArgumentDescriptor::new(
                    1,
                    1,
                    self.expression_categories[arguments[1].value.index()],
                    ParameterMode::Value,
                    t,
                    false,
                ),
            ],
            false,
            false,
        ));
        Ok(ExprCheck {
            ty: t,
            falls_through: true,
        })
    }

    fn check_intrinsic_swap_call(
        &mut self,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        external: ExternalSymbolId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
    ) -> Result<ExprCheck, TypeCheckingError> {
        let mut valid = true;
        if type_arguments.len() > 1 {
            self.emit(
                self.type_argument_arity_code,
                "intrinsic 'swap' accepts at most one type argument",
                call_span,
            )?;
            valid = false;
        }
        if arguments.len() != 2 {
            self.emit(
                self.call_argument_arity_code,
                "intrinsic 'swap' requires exactly 2 arguments",
                call_span,
            )?;
            for argument in arguments {
                self.check_expression(argument.value, None, None)?;
            }
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }
        for argument in arguments {
            if let Some(prefix) = argument.named_prefix {
                self.emit(
                    self.invalid_named_argument_code,
                    "intrinsic 'swap' does not accept named arguments",
                    prefix.name_span,
                )?;
                valid = false;
            }
        }
        let arg0 = &arguments[0];
        if !matches!(arg0.mode_marker, Some(ParameterModeMarker::Inout(_))) {
            self.emit(
                self.call_argument_mode_code,
                "first argument to 'swap' must be prefixed with '&'",
                arg0.span,
            )?;
            valid = false;
        }
        let arg0_result = self.check_expression(arg0.value, None, None)?;
        if self.is_error(arg0_result.ty) {
            valid = false;
        }
        if self.expression_categories[arg0.value.index()] != ExpressionCategory::Place
            || self.is_mutable_element_place(arg0.value) == Some(false)
            || self.is_read_only_container_size(arg0.value)?
        {
            self.emit(
                self.call_argument_mode_code,
                "first argument to 'swap' must be a mutable place",
                arg0.span,
            )?;
            valid = false;
        }

        let arg1 = &arguments[1];
        if !matches!(arg1.mode_marker, Some(ParameterModeMarker::Inout(_))) {
            self.emit(
                self.call_argument_mode_code,
                "second argument to 'swap' must be prefixed with '&'",
                arg1.span,
            )?;
            valid = false;
        }
        let arg1_result = self.check_expression(arg1.value, None, None)?;
        if self.is_error(arg1_result.ty) {
            valid = false;
        }
        if self.expression_categories[arg1.value.index()] != ExpressionCategory::Place
            || self.is_mutable_element_place(arg1.value) == Some(false)
            || self.is_read_only_container_size(arg1.value)?
        {
            self.emit(
                self.call_argument_mode_code,
                "second argument to 'swap' must be a mutable place",
                arg1.span,
            )?;
            valid = false;
        }

        let explicit_t = type_arguments
            .first()
            .copied()
            .map(|t| self.resolve_type_ref(t))
            .transpose()?;

        let t = if let Some(explicit) = explicit_t {
            if !self.is_error(arg0_result.ty)
                && (!self.assignable(arg0_result.ty, explicit)
                    || !self.assignable(explicit, arg0_result.ty))
            {
                self.mismatch(
                    self.ast().expressions().get(arg0.value)?.span(),
                    None,
                    arg0_result.ty,
                    explicit,
                )?;
                valid = false;
            }
            if !self.is_error(arg1_result.ty)
                && (!self.assignable(arg1_result.ty, explicit)
                    || !self.assignable(explicit, arg1_result.ty))
            {
                self.mismatch(
                    self.ast().expressions().get(arg1.value)?.span(),
                    None,
                    arg1_result.ty,
                    explicit,
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
                self.mismatch(
                    self.ast().expressions().get(arg1.value)?.span(),
                    None,
                    arg1_result.ty,
                    arg0_result.ty,
                )?;
                valid = false;
            }
            arg0_result.ty
        };

        let unit = self.builtin(BuiltinType::Unit);
        if !valid || self.is_error(t) {
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }

        let function = self.types.intern(TypeKind::Function {
            move_only: false,
            parameters: vec![
                FunctionParameterType {
                    mode: ParameterMode::Inout,
                    ty: t,
                },
                FunctionParameterType {
                    mode: ParameterMode::Inout,
                    ty: t,
                },
            ],
            return_type: unit,
        });
        self.set_expression(callee, function);
        self.set_expression_category(callee, ExpressionCategory::Temporary);
        self.calls.push(CallDescriptor::new(
            expression,
            CallableTarget::External(external),
            vec![t],
            unit,
            None,
            vec![
                CallArgumentDescriptor::new(
                    0,
                    0,
                    ExpressionCategory::Place,
                    ParameterMode::Inout,
                    t,
                    false,
                ),
                CallArgumentDescriptor::new(
                    1,
                    1,
                    ExpressionCategory::Place,
                    ParameterMode::Inout,
                    t,
                    false,
                ),
            ],
            false,
            false,
        ));
        Ok(ExprCheck {
            ty: unit,
            falls_through: true,
        })
    }
}
