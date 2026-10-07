use crate::{
    ast::{ExpressionId, TypeRefId},
    diagnostic::codes,
    name_resolution::SourceUnitId,
    parser::{AssignmentOperator, CallArgument, Expression},
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypeError, Copyability, IntrinsicCallable,
        IntrinsicTypeConstructor, TypeCheckingError, UnitTypeId, UnitTypeKind,
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

    /// 检查多文件编译单元中 `mapOf()` 与 `mutableMapOf()` 构造调用。
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_map_construction_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        call_span: Span,
        _callee: ExpressionId,
        callable: IntrinsicCallable,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
        expected: Option<UnitTypeId>,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let target_constructor = match callable {
            IntrinsicCallable::MapOf => IntrinsicTypeConstructor::Map,
            IntrinsicCallable::MutableMapOf => IntrinsicTypeConstructor::MutableMap,
            _ => unreachable!(),
        };

        let (key_type, value_type) = if type_arguments.len() == 2 {
            let key = self.resolve_body_type_ref(source, type_arguments[0])?;
            let val = self.resolve_body_type_ref(source, type_arguments[1])?;
            (key, val)
        } else if type_arguments.is_empty() {
            if let Some(expected_ty) = expected
                && let Some((expected_ctor, k, v)) = self.map_parts(expected_ty)
                && expected_ctor == target_constructor
            {
                (k, v)
            } else {
                self.emit(
                    codes::CANNOT_INFER_CONTAINER_ELEMENT,
                    "cannot infer Map key and value types without type arguments or contextual type",
                    call_span,
                )?;
                self.check_construction_operands(source, arguments, return_type)?;
                return Ok(ExpressionCheck {
                    ty: self.error_type(),
                    falls_through: true,
                });
            }
        } else {
            self.emit(
                codes::TYPE_ARGUMENT_ARITY,
                "map construction accepts exactly two type arguments",
                call_span,
            )?;
            self.check_construction_operands(source, arguments, return_type)?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        };

        if self.is_error(key_type) || self.is_error(value_type) {
            self.check_construction_operands(source, arguments, return_type)?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }

        if !self.is_hashable_type(key_type) {
            let key_span = if !type_arguments.is_empty() {
                self.file(source)
                    .ast()
                    .type_refs()
                    .get(type_arguments[0])
                    .map_err(TypeCheckingError::from)?
                    .span()
            } else {
                call_span
            };
            self.emit(
                codes::HASHABLE_TYPE_ARGUMENT_BOUND,
                "Map key type must be Hashable",
                key_span,
            )?;
            self.check_construction_operands(source, arguments, return_type)?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }

        if !self.is_structurally_storable_type(value_type) {
            let val_span = if !type_arguments.is_empty() {
                self.file(source)
                    .ast()
                    .type_refs()
                    .get(type_arguments[1])
                    .map_err(TypeCheckingError::from)?
                    .span()
            } else {
                call_span
            };
            self.emit(
                codes::INVALID_CONTAINER_ELEMENT,
                "container value type is not structurally storable",
                val_span,
            )?;
            self.check_construction_operands(source, arguments, return_type)?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }

        if !arguments.is_empty() {
            self.emit(
                codes::CALL_ARGUMENT_ARITY,
                "mapOf and mutableMapOf do not accept arguments in v1",
                call_span,
            )?;
            self.check_construction_operands(source, arguments, return_type)?;
        }

        let map_ty = self.signatures.types_mut().intern(UnitTypeKind::Intrinsic {
            constructor: target_constructor,
            arguments: vec![key_type, value_type],
        });

        self.parts
            .map_descriptors
            .constructions
            .push(super::super::map::UnitMapConstructionDescriptor::new(
                super::super::super::UnitExpressionId::new(source, expression),
                callable,
                map_ty,
                key_type,
                value_type,
            ));

        Ok(ExpressionCheck {
            ty: map_ty,
            falls_through: true,
        })
    }

    /// 检查 Map 相关的成员属性访问（如 `map.size`）。
    pub(super) fn check_map_member_type(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        receiver_expression: ExpressionId,
        receiver: UnitTypeId,
        name_span: Span,
    ) -> Result<Option<UnitTypeId>, CompilationUnitTypeError> {
        let Some((_constructor, _key_type, _value_type)) = self.map_parts(receiver) else {
            return Ok(None);
        };

        let name = self
            .sources
            .slice(name_span)
            .map_err(TypeCheckingError::from)?;
        if name == "size" {
            self.parts
                .map_descriptors
                .sizes
                .push(super::super::map::UnitMapSizeDescriptor::new(
                    super::super::super::UnitExpressionId::new(source, expression),
                    super::super::super::UnitExpressionId::new(source, receiver_expression),
                    receiver,
                ));
            return Ok(Some(self.builtin(BuiltinType::Int)));
        }

        if matches!(name, "get" | "put" | "remove" | "contains") {
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
        if !matches!(name, "get" | "put" | "remove" | "contains") {
            return Ok(None);
        }

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
                "cannot move MoveOnly map value by get; use borrow access",
                call_span,
            )?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through: receiver_falls_through && arg_result.falls_through,
            });
        }

        let nullable_val = self
            .signatures
            .types_mut()
            .intern(UnitTypeKind::Nullable(value_type));
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
            falls_through: receiver_falls_through && arg_result.falls_through,
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

        self.parts
            .map_descriptors
            .contains_calls
            .push(super::super::map::UnitMapContainsDescriptor::new(
                super::super::super::UnitExpressionId::new(source, expression),
                super::super::super::UnitExpressionId::new(source, receiver),
                super::super::super::UnitExpressionId::new(source, arg.value),
                key_type,
            ));
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

        self.parts
            .map_descriptors
            .puts
            .push(super::super::map::UnitMapPutDescriptor::new(
                super::super::super::UnitExpressionId::new(source, expression),
                super::super::super::UnitExpressionId::new(source, receiver),
                super::super::super::UnitExpressionId::new(source, key_arg.value),
                super::super::super::UnitExpressionId::new(source, val_arg.value),
                key_type,
                value_type,
            ));
        Ok(ExpressionCheck {
            ty: self.builtin(BuiltinType::Unit),
            falls_through: receiver_falls_through
                && key_result.falls_through
                && val_result.falls_through,
        })
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

        let nullable_val = self
            .signatures
            .types_mut()
            .intern(UnitTypeKind::Nullable(value_type));
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
        source: SourceUnitId,
        expression: ExpressionId,
        receiver: ExpressionId,
        receiver_result: ExpressionCheck,
        index: ExpressionId,
        return_type: UnitTypeId,
    ) -> Result<Option<ExpressionCheck>, CompilationUnitTypeError> {
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
                "cannot move MoveOnly map value by index; use borrow access",
                self.file(source)
                    .ast()
                    .expressions()
                    .get(expression)
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
        self.parts
            .map_descriptors
            .gets
            .push(super::super::map::UnitMapGetDescriptor::new(
                super::super::super::UnitExpressionId::new(source, expression),
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

        let receiver_result = self.check_expression(source, receiver, None, None, return_type)?;
        let Some((constructor, key_type, value_type)) = self.map_parts(receiver_result.ty) else {
            return Ok(None);
        };

        if constructor != IntrinsicTypeConstructor::MutableMap {
            self.emit(
                codes::IMMUTABLE_CONTAINER_PLACE,
                "cannot mutate read-only Map; use MutableMap",
                operator_span,
            )?;
            self.check_expression(source, index, Some(key_type), None, return_type)?;
            self.check_expression(source, value, Some(value_type), None, return_type)?;
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

        self.parts
            .map_descriptors
            .puts
            .push(super::super::map::UnitMapPutDescriptor::new(
                super::super::super::UnitExpressionId::new(source, target),
                super::super::super::UnitExpressionId::new(source, receiver),
                super::super::super::UnitExpressionId::new(source, index),
                super::super::super::UnitExpressionId::new(source, value),
                key_type,
                value_type,
            ));
        Ok(Some(ExpressionCheck {
            ty: self.builtin(BuiltinType::Unit),
            falls_through: receiver_result.falls_through
                && index_result.falls_through
                && val_result.falls_through,
        }))
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
