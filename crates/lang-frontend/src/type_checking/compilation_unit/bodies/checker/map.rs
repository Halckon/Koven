#[path = "map_construction.rs"]
mod construction;
#[path = "map_consume.rs"]
mod map_consume;
#[path = "map_require.rs"]
mod map_require;
#[path = "map_with.rs"]
mod map_with;
use crate::{
    ast::{ExpressionId, TypeRefId},
    diagnostic::codes,
    name_resolution::{SourceUnitId, UnitReferenceTarget},
    parser::{AssignmentOperator, CallArgument, Expression},
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypeError, Copyability, IntrinsicTypeConstructor,
        TypeCheckingError, UnitExpressionId, UnitTypeId, UnitTypeKind,
    },
};

use super::{BodyChecker, ExpressionCheck};

impl BodyChecker<'_> {
    /// 若类型是 `Map<K, V>` 或 `MutableMap<K, V>`，返回 `(constructor, key_type, value_type)`。
    pub(super) fn map_parts(
        &self,
        ty: UnitTypeId,
    ) -> Option<(IntrinsicTypeConstructor, UnitTypeId, UnitTypeId)> {
        let Some(UnitTypeKind::Intrinsic {
            constructor,
            arguments,
        }) = self.signatures.types().get(ty)
        else {
            return None;
        };
        if matches!(
            *constructor,
            IntrinsicTypeConstructor::Map | IntrinsicTypeConstructor::MutableMap
        ) && arguments.len() == 2
        {
            Some((*constructor, arguments[0], arguments[1]))
        } else {
            None
        }
    }

    /// 检查 Map 相关的成员属性访问（如 `map.size`）。
    pub(super) fn check_map_member_type(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        receiver_expression: ExpressionId,
        receiver: UnitTypeId,
        name_span: Span,
        receiver_pending: bool,
    ) -> Result<Option<UnitTypeId>, CompilationUnitTypeError> {
        let Some((_constructor, _key_type, _value_type)) = self.map_parts(receiver) else {
            return Ok(None);
        };

        let name = self
            .sources
            .slice(name_span)
            .map_err(TypeCheckingError::from)?;
        if name == "size" {
            let result = self.builtin(BuiltinType::Int);
            if receiver_pending || self.contains_unresolved_call_type(receiver) {
                self.note_pending_storage_value(UnitExpressionId::new(source, expression), result);
                return Ok(Some(result));
            }
            self.parts
                .map_descriptors
                .sizes
                .push(super::super::map::UnitMapSizeDescriptor::new(
                    super::super::super::UnitExpressionId::new(source, expression),
                    super::super::super::UnitExpressionId::new(source, receiver_expression),
                    receiver,
                ));
            return Ok(Some(result));
        }

        if matches!(
            name,
            "get" | "put" | "remove" | "contains" | "requireValue" | "withValue" | "consume"
        ) {
            self.emit(
                codes::INVALID_CONTAINER_MEMBER,
                &format!("Map.{name} must be called as a method"),
                name_span,
            )?;
            return Ok(Some(self.error_type()));
        }

        Ok(None)
    }

    /// 检查 Map 相关的成员方法调用（`get`, `put`, `remove`, `contains`）。
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_map_method_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
        return_type: UnitTypeId,
    ) -> Result<Option<ExpressionCheck>, CompilationUnitTypeError> {
        let callee_node = self
            .file(source)
            .ast()
            .expressions()
            .get(callee)
            .map_err(TypeCheckingError::from)?;
        let Expression::Member {
            receiver,
            name_span,
            safe,
            ..
        } = callee_node.payload().clone()
        else {
            return Ok(None);
        };
        if safe {
            return Ok(None);
        }

        let name = self
            .sources
            .slice(name_span)
            .map_err(TypeCheckingError::from)?;
        if !matches!(
            name,
            "get" | "put" | "remove" | "contains" | "requireValue" | "withValue" | "consume"
        ) {
            return Ok(None);
        }

        let receiver_node = self
            .file(source)
            .ast()
            .expressions()
            .get(receiver)
            .map_err(TypeCheckingError::from)?;
        let receiver_name_span = match receiver_node.payload() {
            Expression::Name => Some(receiver_node.span()),
            Expression::Member {
                name_span,
                safe: false,
                ..
            } => Some(*name_span),
            _ => None,
        };
        // A resolved package path is a namespace, never a runtime Map receiver.
        if receiver_name_span.is_some_and(|span| {
            self.names.references().iter().any(|reference| {
                reference.source_unit() == source
                    && reference.span() == span
                    && reference.namespace().is_none()
                    && matches!(reference.target(), UnitReferenceTarget::Package(_))
            })
        }) {
            return Ok(None);
        }

        let deferred = self.deferred_facts.len();
        let receiver_result = self.check_expression(source, receiver, None, None, return_type)?;
        let Some((constructor, key_type, value_type)) = self.map_parts(receiver_result.ty) else {
            return Ok(None);
        };

        if !type_arguments.is_empty() {
            self.emit(
                codes::TYPE_ARGUMENT_ARITY,
                "Map methods do not accept type arguments",
                call_span,
            )?;
            self.check_construction_operands(source, arguments, return_type)?;
            return Ok(Some(ExpressionCheck {
                ty: self.error_type(),
                falls_through: receiver_result.falls_through,
            }));
        }

        match name {
            "consume" => Ok(Some(self.check_map_consume_call(
                source,
                expression,
                receiver,
                receiver_result,
                constructor,
                key_type,
                value_type,
                arguments,
                call_span,
                name_span,
                return_type,
            )?)),
            "withValue" => Ok(Some(self.check_map_with_call(
                source,
                expression,
                receiver,
                receiver_result.falls_through,
                key_type,
                value_type,
                arguments,
                call_span,
                return_type,
            )?)),
            "requireValue" => Ok(Some(
                self.check_map_require_call(
                    source,
                    expression,
                    receiver,
                    receiver_result.falls_through,
                    key_type,
                    value_type,
                    arguments,
                    call_span,
                    name_span,
                    self.file(source)
                        .ast()
                        .expressions()
                        .get(receiver)
                        .map_err(TypeCheckingError::from)?
                        .span(),
                    return_type,
                )?,
            )),

            "get" => {
                let check = self.check_map_get_call(
                    source,
                    expression,
                    receiver,
                    receiver_result.falls_through,
                    key_type,
                    value_type,
                    arguments,
                    call_span,
                    return_type,
                    deferred,
                )?;
                Ok(Some(check))
            }
            "contains" => {
                let check = self.check_map_contains_call(
                    source,
                    expression,
                    receiver,
                    receiver_result.falls_through,
                    key_type,
                    arguments,
                    call_span,
                    return_type,
                )?;
                Ok(Some(check))
            }
            "put" => {
                if constructor != IntrinsicTypeConstructor::MutableMap {
                    self.emit(
                        codes::INVALID_CONTAINER_MEMBER,
                        "read-only Map does not support 'put'; use MutableMap",
                        name_span,
                    )?;
                    self.check_construction_operands(source, arguments, return_type)?;
                    return Ok(Some(ExpressionCheck {
                        ty: self.error_type(),
                        falls_through: receiver_result.falls_through,
                    }));
                }
                let check = self.check_map_put_call(
                    source,
                    expression,
                    receiver,
                    receiver_result.falls_through,
                    key_type,
                    value_type,
                    arguments,
                    call_span,
                    return_type,
                    deferred,
                )?;
                Ok(Some(check))
            }
            "remove" => {
                if constructor != IntrinsicTypeConstructor::MutableMap {
                    self.emit(
                        codes::INVALID_CONTAINER_MEMBER,
                        "read-only Map does not support 'remove'; use MutableMap",
                        name_span,
                    )?;
                    self.check_construction_operands(source, arguments, return_type)?;
                    return Ok(Some(ExpressionCheck {
                        ty: self.error_type(),
                        falls_through: receiver_result.falls_through,
                    }));
                }
                let check = self.check_map_remove_call(
                    source,
                    expression,
                    receiver,
                    receiver_result.falls_through,
                    key_type,
                    value_type,
                    arguments,
                    call_span,
                    return_type,
                )?;
                Ok(Some(check))
            }
            _ => Ok(None),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn check_map_get_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        receiver: ExpressionId,
        receiver_falls_through: bool,
        key_type: UnitTypeId,
        value_type: UnitTypeId,
        arguments: &[CallArgument],
        call_span: Span,
        return_type: UnitTypeId,
        deferred: usize,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        if arguments.len() != 1 {
            self.emit(
                codes::CALL_ARGUMENT_ARITY,
                "Map.get expects exactly 1 argument",
                call_span,
            )?;
            self.check_construction_operands(source, arguments, return_type)?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: receiver_falls_through,
            });
        }

        let arg = &arguments[0];
        let arg_result =
            self.check_expression(source, arg.value, Some(key_type), None, return_type)?;
        if !self.is_error(arg_result.ty)
            && !self.is_deferred(arg_result.ty)
            && !self.assignable(arg_result.ty, key_type)
        {
            self.mismatch(
                self.file(source)
                    .ast()
                    .expressions()
                    .get(arg.value)
                    .map_err(TypeCheckingError::from)?
                    .span(),
                None,
                arg_result.ty,
                key_type,
            )?;
        }

        if self.copyability_of(value_type) == Copyability::MoveOnly {
            self.emit(
                codes::MOVE_FROM_CONTAINER_ELEMENT,
                "cannot move MoveOnly map value by get; use requireValue or withValue",
                call_span,
            )?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: receiver_falls_through && arg_result.falls_through,
            });
        }

        if matches!(
            self.signatures.types().get(value_type),
            Some(UnitTypeKind::Nullable(_))
        ) {
            self.emit(
                codes::INVALID_CONTAINER_MEMBER,
                "nullable map values require withValue to distinguish Missing from Found(null)",
                call_span,
            )?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: receiver_falls_through && arg_result.falls_through,
            });
        }
        let falls_through = receiver_falls_through && arg_result.falls_through;
        if self.is_error(arg_result.ty) {
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through,
            });
        }
        let nullable_val = self
            .signatures
            .types_mut()
            .intern(UnitTypeKind::Nullable(value_type));
        if self.has_pending_storage_operands(source, receiver, arg.value, deferred)
            || self.is_unresolved_call_type(arg_result.ty)
            || self.contains_unresolved_call_type(key_type)
            || self.contains_unresolved_call_type(value_type)
        {
            self.note_pending_storage_value(
                UnitExpressionId::new(source, expression),
                nullable_val,
            );
            return Ok(ExpressionCheck {
                ty: nullable_val,
                falls_through,
            });
        }
        self.parts
            .map_descriptors
            .gets
            .push(super::super::map::UnitMapGetDescriptor::new(
                super::super::super::UnitExpressionId::new(source, expression),
                super::super::super::UnitExpressionId::new(source, receiver),
                super::super::super::UnitExpressionId::new(source, arg.value),
                key_type,
                value_type,
                nullable_val,
            ));
        Ok(ExpressionCheck {
            ty: nullable_val,
            falls_through,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn check_map_contains_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        receiver: ExpressionId,
        receiver_falls_through: bool,
        key_type: UnitTypeId,
        arguments: &[CallArgument],
        call_span: Span,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        if arguments.len() != 1 {
            self.emit(
                codes::CALL_ARGUMENT_ARITY,
                "Map.contains expects exactly 1 argument",
                call_span,
            )?;
            self.check_construction_operands(source, arguments, return_type)?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: receiver_falls_through,
            });
        }

        let arg = &arguments[0];
        let arg_result =
            self.check_expression(source, arg.value, Some(key_type), None, return_type)?;
        if !self.is_error(arg_result.ty)
            && !self.is_deferred(arg_result.ty)
            && !self.assignable(arg_result.ty, key_type)
        {
            self.mismatch(
                self.file(source)
                    .ast()
                    .expressions()
                    .get(arg.value)
                    .map_err(TypeCheckingError::from)?
                    .span(),
                None,
                arg_result.ty,
                key_type,
            )?;
        }

        self.parts.map_descriptors.contains_calls.push(
            super::super::map::UnitMapContainsDescriptor::new(
                super::super::super::UnitExpressionId::new(source, expression),
                super::super::super::UnitExpressionId::new(source, receiver),
                super::super::super::UnitExpressionId::new(source, arg.value),
                key_type,
            ),
        );
        Ok(ExpressionCheck {
            ty: self.builtin(BuiltinType::Boolean),
            falls_through: receiver_falls_through && arg_result.falls_through,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn check_map_put_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        receiver: ExpressionId,
        receiver_falls_through: bool,
        key_type: UnitTypeId,
        value_type: UnitTypeId,
        arguments: &[CallArgument],
        call_span: Span,
        return_type: UnitTypeId,
        deferred: usize,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        if arguments.len() != 2 {
            self.emit(
                codes::CALL_ARGUMENT_ARITY,
                "MutableMap.put expects exactly 2 arguments (key, value)",
                call_span,
            )?;
            self.check_construction_operands(source, arguments, return_type)?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: receiver_falls_through,
            });
        }

        let key_arg = &arguments[0];
        let val_arg = &arguments[1];
        let key_result =
            self.check_expression(source, key_arg.value, Some(key_type), None, return_type)?;
        let val_result =
            self.check_expression(source, val_arg.value, Some(value_type), None, return_type)?;

        if !self.is_error(key_result.ty)
            && !self.is_deferred(key_result.ty)
            && !self.assignable(key_result.ty, key_type)
        {
            self.mismatch(
                self.file(source)
                    .ast()
                    .expressions()
                    .get(key_arg.value)
                    .map_err(TypeCheckingError::from)?
                    .span(),
                None,
                key_result.ty,
                key_type,
            )?;
        }

        if !self.is_error(val_result.ty)
            && !self.is_deferred(val_result.ty)
            && !self.assignable(val_result.ty, value_type)
        {
            self.mismatch(
                self.file(source)
                    .ast()
                    .expressions()
                    .get(val_arg.value)
                    .map_err(TypeCheckingError::from)?
                    .span(),
                None,
                val_result.ty,
                value_type,
            )?;
        }

        let falls_through =
            receiver_falls_through && key_result.falls_through && val_result.falls_through;
        if self.is_error(key_result.ty) || self.is_error(val_result.ty) {
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through,
            });
        }
        let ty = self.builtin(BuiltinType::Unit);
        let put = super::super::map::UnitMapPutDescriptor::new(
            UnitExpressionId::new(source, expression),
            UnitExpressionId::new(source, receiver),
            UnitExpressionId::new(source, key_arg.value),
            UnitExpressionId::new(source, val_arg.value),
            key_type,
            value_type,
        );
        if self.map_write_has_pending_operands(&put, deferred) {
            self.note_pending_storage_value(put.expression(), ty);
        } else {
            self.parts.map_descriptors.puts.push(put);
        }
        Ok(ExpressionCheck { ty, falls_through })
    }

    #[allow(clippy::too_many_arguments)]
    fn check_map_remove_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        receiver: ExpressionId,
        receiver_falls_through: bool,
        key_type: UnitTypeId,
        value_type: UnitTypeId,
        arguments: &[CallArgument],
        call_span: Span,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        if arguments.len() != 1 {
            self.emit(
                codes::CALL_ARGUMENT_ARITY,
                "MutableMap.remove expects exactly 1 argument",
                call_span,
            )?;
            self.check_construction_operands(source, arguments, return_type)?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: receiver_falls_through,
            });
        }

        let key_arg = &arguments[0];
        let key_result =
            self.check_expression(source, key_arg.value, Some(key_type), None, return_type)?;
        if !self.is_error(key_result.ty)
            && !self.is_deferred(key_result.ty)
            && !self.assignable(key_result.ty, key_type)
        {
            self.mismatch(
                self.file(source)
                    .ast()
                    .expressions()
                    .get(key_arg.value)
                    .map_err(TypeCheckingError::from)?
                    .span(),
                None,
                key_result.ty,
                key_type,
            )?;
        }

        let nullable_val = if matches!(
            self.signatures.types().get(value_type),
            Some(UnitTypeKind::Nullable(_))
        ) {
            value_type
        } else {
            self.signatures
                .types_mut()
                .intern(UnitTypeKind::Nullable(value_type))
        };
        self.parts
            .map_descriptors
            .removes
            .push(super::super::map::UnitMapRemoveDescriptor::new(
                super::super::super::UnitExpressionId::new(source, expression),
                super::super::super::UnitExpressionId::new(source, receiver),
                super::super::super::UnitExpressionId::new(source, key_arg.value),
                key_type,
                value_type,
                nullable_val,
            ));
        Ok(ExpressionCheck {
            ty: nullable_val,
            falls_through: receiver_falls_through && key_result.falls_through,
        })
    }

    /// 检查 Map 下标查询 `map[key]`。
    pub(super) fn check_map_index(
        &mut self,
        expression: UnitExpressionId,
        receiver: ExpressionId,
        receiver_result: ExpressionCheck,
        index: ExpressionId,
        return_type: UnitTypeId,
        deferred: usize,
    ) -> Result<Option<ExpressionCheck>, CompilationUnitTypeError> {
        let source = expression.source_unit();
        let Some((_constructor, key_type, value_type)) = self.map_parts(receiver_result.ty) else {
            return Ok(None);
        };

        let index_result =
            self.check_expression(source, index, Some(key_type), None, return_type)?;
        if !self.assignable(index_result.ty, key_type)
            && !self.is_error(index_result.ty)
            && !self.is_deferred(index_result.ty)
        {
            self.emit(
                codes::INVALID_CONTAINER_INDEX,
                "map index key type does not match map key type",
                self.file(source)
                    .ast()
                    .expressions()
                    .get(index)
                    .map_err(TypeCheckingError::from)?
                    .span(),
            )?;
            return Ok(Some(ExpressionCheck {
                ty: self.error_type(),
                falls_through: receiver_result.falls_through && index_result.falls_through,
            }));
        }

        if self.is_error(index_result.ty) {
            return Ok(Some(ExpressionCheck {
                ty: self.error_type(),
                falls_through: receiver_result.falls_through && index_result.falls_through,
            }));
        }

        if self.copyability_of(value_type) == Copyability::MoveOnly {
            self.emit(
                codes::MOVE_FROM_CONTAINER_ELEMENT,
                "cannot move MoveOnly map value by index; use requireValue or withValue",
                self.file(source)
                    .ast()
                    .expressions()
                    .get(expression.expression())
                    .map_err(TypeCheckingError::from)?
                    .span(),
            )?;
            return Ok(Some(ExpressionCheck {
                ty: self.error_type(),
                falls_through: receiver_result.falls_through && index_result.falls_through,
            }));
        }

        if matches!(
            self.signatures.types().get(value_type),
            Some(UnitTypeKind::Nullable(_))
        ) {
            self.emit(
                codes::INVALID_CONTAINER_MEMBER,
                "nullable map values require withValue to distinguish Missing from Found(null)",
                self.file(source)
                    .ast()
                    .expressions()
                    .get(expression.expression())
                    .map_err(TypeCheckingError::from)?
                    .span(),
            )?;
            return Ok(Some(ExpressionCheck {
                ty: self.error_type(),
                falls_through: receiver_result.falls_through && index_result.falls_through,
            }));
        }
        let nullable_val = self
            .signatures
            .types_mut()
            .intern(UnitTypeKind::Nullable(value_type));
        if self.has_pending_storage_operands(source, receiver, index, deferred)
            || self.is_unresolved_call_type(index_result.ty)
            || self.contains_unresolved_call_type(receiver_result.ty)
        {
            self.note_pending_storage_value(expression, nullable_val);
            return Ok(Some(ExpressionCheck {
                ty: nullable_val,
                falls_through: receiver_result.falls_through && index_result.falls_through,
            }));
        }
        self.parts
            .map_descriptors
            .gets
            .push(super::super::map::UnitMapGetDescriptor::new(
                expression,
                super::super::super::UnitExpressionId::new(source, receiver),
                super::super::super::UnitExpressionId::new(source, index),
                key_type,
                value_type,
                nullable_val,
            ));
        Ok(Some(ExpressionCheck {
            ty: nullable_val,
            falls_through: receiver_result.falls_through && index_result.falls_through,
        }))
    }

    /// 检查 Map 下标赋值 `mutableMap[key] = value`。
    pub(super) fn check_map_assignment(
        &mut self,
        source: SourceUnitId,
        target: ExpressionId,
        operator: AssignmentOperator,
        operator_span: Span,
        value: ExpressionId,
        return_type: UnitTypeId,
    ) -> Result<Option<ExpressionCheck>, CompilationUnitTypeError> {
        let Expression::Index { receiver, index } = self
            .file(source)
            .ast()
            .expressions()
            .get(target)
            .map_err(TypeCheckingError::from)?
            .payload()
            .clone()
        else {
            return Ok(None);
        };

        let deferred = self.deferred_facts.len();
        let receiver_result = self.check_expression(source, receiver, None, None, return_type)?;
        let Some((constructor, key_type, value_type)) = self.map_parts(receiver_result.ty) else {
            // The sequential assignment path reuses this checked receiver.
            // Retain pending facts that were nested inside its expression.
            let receiver = UnitExpressionId::new(source, receiver);
            if self.container_parts(receiver_result.ty).is_some()
                && !self.storage_value_is_pending(receiver)
                && self.has_pending_storage_value_since(deferred)
            {
                self.note_pending_storage_value(receiver, receiver_result.ty);
            }
            return Ok(None);
        };

        let target = UnitExpressionId::new(source, target);
        let put = super::super::map::UnitMapPutDescriptor::new(
            target,
            UnitExpressionId::new(source, receiver),
            UnitExpressionId::new(source, index),
            UnitExpressionId::new(source, value),
            key_type,
            value_type,
        );
        if constructor != IntrinsicTypeConstructor::MutableMap {
            self.emit(
                codes::IMMUTABLE_CONTAINER_PLACE,
                "cannot mutate read-only Map; use MutableMap",
                operator_span,
            )?;
            self.check_expression(source, index, Some(key_type), None, return_type)?;
            self.check_expression(source, value, Some(value_type), None, return_type)?;
            self.parts
                .storage_recovery
                .unpublished_map_writes
                .insert(target, put);
            return Ok(Some(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            }));
        }

        if operator != AssignmentOperator::Assign {
            self.emit(
                codes::INVALID_CONTAINER_MEMBER,
                "Map subscript assignment supports only '=' operator",
                operator_span,
            )?;
        }

        let index_result =
            self.check_expression(source, index, Some(key_type), None, return_type)?;
        if !self.assignable(index_result.ty, key_type)
            && !self.is_error(index_result.ty)
            && !self.is_deferred(index_result.ty)
        {
            self.emit(
                codes::INVALID_CONTAINER_INDEX,
                "map index key type does not match map key type",
                self.file(source)
                    .ast()
                    .expressions()
                    .get(index)
                    .map_err(TypeCheckingError::from)?
                    .span(),
            )?;
        }

        let val_result =
            self.check_expression(source, value, Some(value_type), None, return_type)?;
        if !self.assignable(val_result.ty, value_type)
            && !self.is_error(val_result.ty)
            && !self.is_deferred(val_result.ty)
        {
            self.mismatch(
                self.file(source)
                    .ast()
                    .expressions()
                    .get(value)
                    .map_err(TypeCheckingError::from)?
                    .span(),
                None,
                val_result.ty,
                value_type,
            )?;
        }

        if operator != AssignmentOperator::Assign
            || self.is_error(index_result.ty)
            || self.is_error(val_result.ty)
            || self.map_write_has_pending_operands(&put, deferred)
        {
            self.parts
                .storage_recovery
                .unpublished_map_writes
                .insert(target, put);
        } else {
            self.parts.map_descriptors.puts.push(put);
        }
        Ok(Some(ExpressionCheck {
            ty: self.builtin(BuiltinType::Unit),
            falls_through: receiver_result.falls_through
                && index_result.falls_through
                && val_result.falls_through,
        }))
    }

    fn map_write_has_pending_operands(
        &self,
        put: &super::super::map::UnitMapPutDescriptor,
        deferred: usize,
    ) -> bool {
        self.has_pending_storage_value_since(deferred)
            || [put.receiver(), put.key(), put.value()]
                .into_iter()
                .any(|operand| {
                    self.storage_value_is_pending(operand)
                        || self
                            .parts
                            .expression_types
                            .get(&operand)
                            .is_some_and(|&ty| self.contains_unresolved_call_type(ty))
                })
    }

    fn mismatch(
        &mut self,
        primary: Span,
        expected_span: Option<Span>,
        actual: UnitTypeId,
        expected: UnitTypeId,
    ) -> Result<(), CompilationUnitTypeError> {
        if self.is_error(actual)
            || self.is_deferred(actual)
            || self.is_error(expected)
            || self.is_deferred(expected)
        {
            return Ok(());
        }
        let message = "expression type does not match the expected type";
        self.emit_maybe_label(
            codes::TYPE_MISMATCH,
            message,
            primary,
            expected_span,
            format!(
                "expected {}, found {}",
                self.type_name(expected),
                self.type_name(actual)
            ),
        )
    }
}
