use crate::{
    ast::{ExpressionId, TypeRefId},
    diagnostic::codes,
    parser::{AssignmentOperator, CallArgument, Expression},
    source::Span,
    type_checking::{
        BuiltinType, Capability, Copyability, IntrinsicCallable, IntrinsicTypeConstructor,
        TypeCheckingError, TypeId, TypeKind, TypeParameterBound,
    },
};

use super::{Checker, ExprCheck, container::ContainerCall};

impl Checker<'_> {
    /// 判定给定类型是否满足 `Hashable` 能力约束。
    /// v1 规则：内建 Int, Boolean, Char, String 满足；具有 Hashable bound 的类型参数满足；其余类型不满足。
    pub(in crate::type_checking) fn is_hashable_type(&self, ty: TypeId) -> bool {
        match self.kind(ty) {
            TypeKind::Builtin(
                BuiltinType::Int | BuiltinType::Boolean | BuiltinType::Char | BuiltinType::String,
            ) => true,
            TypeKind::TypeParameter(symbol) => self
                .type_parameters
                .iter()
                .find(|descriptor| descriptor.symbol() == *symbol)
                .is_some_and(|descriptor| {
                    descriptor.bound() == TypeParameterBound::Capability(Capability::Hashable)
                }),
            _ => false,
        }
    }

    /// 若类型是 `Map<K, V>` 或 `MutableMap<K, V>`，返回 `(constructor, key_type, value_type)`。
    pub(super) fn map_parts(
        &self,
        ty: TypeId,
    ) -> Option<(IntrinsicTypeConstructor, TypeId, TypeId)> {
        let TypeKind::Intrinsic {
            constructor,
            arguments,
        } = self.kind(ty)
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

    /// 检查 `mapOf()` 与 `mutableMapOf()` 构造调用。
    pub(super) fn check_map_construction_call(
        &mut self,
        call: ContainerCall<'_>,
        callable: IntrinsicCallable,
        expected: Option<TypeId>,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let target_constructor = match callable {
            IntrinsicCallable::MapOf => IntrinsicTypeConstructor::Map,
            IntrinsicCallable::MutableMapOf => IntrinsicTypeConstructor::MutableMap,
            _ => unreachable!(),
        };

        let (key_type, value_type) = if call.type_arguments.len() == 2 {
            let key = self.resolve_type_ref(call.type_arguments[0])?;
            let val = self.resolve_type_ref(call.type_arguments[1])?;
            (key, val)
        } else if call.type_arguments.is_empty() {
            if let Some(expected_ty) = expected
                && let Some((expected_ctor, k, v)) = self.map_parts(expected_ty)
                && expected_ctor == target_constructor
            {
                (k, v)
            } else {
                self.emit(
                    self.cannot_infer_container_element_code,
                    "cannot infer Map key and value types without type arguments or contextual type",
                    call.span,
                )?;
                self.check_construction_operands(call.arguments)?;
                return Ok(ExprCheck {
                    ty: self.error_type(),
                    falls_through: true,
                });
            }
        } else {
            self.emit(
                self.type_argument_arity_code,
                "map construction accepts exactly two type arguments",
                call.span,
            )?;
            self.check_construction_operands(call.arguments)?;
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        };

        if self.is_error(key_type) || self.is_error(value_type) {
            self.check_construction_operands(call.arguments)?;
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }

        if !self.is_hashable_type(key_type) {
            let key_span = if !call.type_arguments.is_empty() {
                self.ast().type_refs().get(call.type_arguments[0])?.span()
            } else {
                call.span
            };
            self.emit(
                self.hashable_type_argument_bound_code,
                "Map key type must be Hashable",
                key_span,
            )?;
            self.check_construction_operands(call.arguments)?;
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }

        if !self.is_structurally_storable_type(value_type) {
            let val_span = if !call.type_arguments.is_empty() {
                self.ast().type_refs().get(call.type_arguments[1])?.span()
            } else {
                call.span
            };
            self.emit(
                self.invalid_container_element_code,
                "container value type is not structurally storable",
                val_span,
            )?;
            self.check_construction_operands(call.arguments)?;
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            });
        }

        if !call.arguments.is_empty() {
            self.emit(
                self.call_argument_arity_code,
                "mapOf and mutableMapOf do not accept arguments in v1",
                call.span,
            )?;
            self.check_construction_operands(call.arguments)?;
        }

        let map_ty = self.types.intern(TypeKind::Intrinsic {
            constructor: target_constructor,
            arguments: vec![key_type, value_type],
        });

        Ok(ExprCheck {
            ty: map_ty,
            falls_through: true,
        })
    }

    /// 检查 Map 相关的成员属性访问（如 `map.size`）。
    pub(super) fn check_map_member_type(
        &mut self,
        _expression: ExpressionId,
        _receiver_expression: ExpressionId,
        receiver: TypeId,
        name: &str,
        name_span: Span,
    ) -> Result<Option<TypeId>, TypeCheckingError> {
        let Some((_constructor, _key_type, _value_type)) = self.map_parts(receiver) else {
            return Ok(None);
        };

        if name == "size" {
            return Ok(Some(self.builtin(BuiltinType::Int)));
        }

        if matches!(name, "get" | "put" | "remove" | "contains") {
            self.emit(
                self.invalid_container_member_code,
                &format!("Map.{name} must be called as a method"),
                name_span,
            )?;
            return Ok(Some(self.error_type()));
        }

        Ok(None)
    }

    /// 检查 Map 相关的成员方法调用（`get`, `put`, `remove`, `contains`）。
    pub(super) fn check_map_method_call(
        &mut self,
        _expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
    ) -> Result<Option<ExprCheck>, TypeCheckingError> {
        let callee_node = self.ast().expressions().get(callee)?;
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

        let name = self.sources.slice(name_span)?;
        if !matches!(name, "get" | "put" | "remove" | "contains") {
            return Ok(None);
        }

        let receiver_result = self.check_expression(receiver, None, None)?;
        let Some((constructor, key_type, value_type)) = self.map_parts(receiver_result.ty) else {
            return Ok(None);
        };

        if !type_arguments.is_empty() {
            self.emit(
                self.type_argument_arity_code,
                "Map methods do not accept type arguments",
                call_span,
            )?;
            self.check_construction_operands(arguments)?;
            return Ok(Some(ExprCheck {
                ty: self.error_type(),
                falls_through: receiver_result.falls_through,
            }));
        }

        match name {
            "get" => {
                let check = self.check_map_get_call(
                    receiver_result.falls_through,
                    key_type,
                    value_type,
                    arguments,
                    call_span,
                )?;
                Ok(Some(check))
            }
            "contains" => {
                let check = self.check_map_contains_call(
                    receiver_result.falls_through,
                    key_type,
                    arguments,
                    call_span,
                )?;
                Ok(Some(check))
            }
            "put" => {
                if constructor != IntrinsicTypeConstructor::MutableMap {
                    self.emit(
                        self.invalid_container_member_code,
                        "read-only Map does not support 'put'; use MutableMap",
                        name_span,
                    )?;
                    self.check_construction_operands(arguments)?;
                    return Ok(Some(ExprCheck {
                        ty: self.error_type(),
                        falls_through: receiver_result.falls_through,
                    }));
                }
                let check = self.check_map_put_call(
                    receiver_result.falls_through,
                    key_type,
                    value_type,
                    arguments,
                    call_span,
                )?;
                Ok(Some(check))
            }
            "remove" => {
                if constructor != IntrinsicTypeConstructor::MutableMap {
                    self.emit(
                        self.invalid_container_member_code,
                        "read-only Map does not support 'remove'; use MutableMap",
                        name_span,
                    )?;
                    self.check_construction_operands(arguments)?;
                    return Ok(Some(ExprCheck {
                        ty: self.error_type(),
                        falls_through: receiver_result.falls_through,
                    }));
                }
                let check = self.check_map_remove_call(
                    receiver_result.falls_through,
                    key_type,
                    value_type,
                    arguments,
                    call_span,
                )?;
                Ok(Some(check))
            }
            _ => Ok(None),
        }
    }

    fn check_map_get_call(
        &mut self,
        receiver_falls_through: bool,
        key_type: TypeId,
        value_type: TypeId,
        arguments: &[CallArgument],
        call_span: Span,
    ) -> Result<ExprCheck, TypeCheckingError> {
        if arguments.len() != 1 {
            self.emit(
                self.call_argument_arity_code,
                "Map.get expects exactly 1 argument",
                call_span,
            )?;
            self.check_construction_operands(arguments)?;
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: receiver_falls_through,
            });
        }

        let arg = &arguments[0];
        let arg_result = self.check_expression(arg.value, Some(key_type), None)?;
        if !self.is_error(arg_result.ty)
            && !self.is_deferred(arg_result.ty)
            && !self.assignable(arg_result.ty, key_type)
        {
            self.mismatch(
                self.ast().expressions().get(arg.value)?.span(),
                None,
                arg_result.ty,
                key_type,
            )?;
        }

        if self.copyability_of(value_type) == Copyability::MoveOnly {
            let code = codes::catalog()?
                .resolve(codes::MOVE_FROM_CONTAINER_ELEMENT)
                .expect("MOVE_FROM_CONTAINER_ELEMENT catalog resolve");
            self.emit(
                code,
                "cannot move MoveOnly map value by get; use borrow access",
                call_span,
            )?;
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: receiver_falls_through && arg_result.falls_through,
            });
        }

        let nullable_val = self.types.intern(TypeKind::Nullable(value_type));
        Ok(ExprCheck {
            ty: nullable_val,
            falls_through: receiver_falls_through && arg_result.falls_through,
        })
    }

    fn check_map_contains_call(
        &mut self,
        receiver_falls_through: bool,
        key_type: TypeId,
        arguments: &[CallArgument],
        call_span: Span,
    ) -> Result<ExprCheck, TypeCheckingError> {
        if arguments.len() != 1 {
            self.emit(
                self.call_argument_arity_code,
                "Map.contains expects exactly 1 argument",
                call_span,
            )?;
            self.check_construction_operands(arguments)?;
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: receiver_falls_through,
            });
        }

        let arg = &arguments[0];
        let arg_result = self.check_expression(arg.value, Some(key_type), None)?;
        if !self.is_error(arg_result.ty)
            && !self.is_deferred(arg_result.ty)
            && !self.assignable(arg_result.ty, key_type)
        {
            self.mismatch(
                self.ast().expressions().get(arg.value)?.span(),
                None,
                arg_result.ty,
                key_type,
            )?;
        }

        Ok(ExprCheck {
            ty: self.builtin(BuiltinType::Boolean),
            falls_through: receiver_falls_through && arg_result.falls_through,
        })
    }

    fn check_map_put_call(
        &mut self,
        receiver_falls_through: bool,
        key_type: TypeId,
        value_type: TypeId,
        arguments: &[CallArgument],
        call_span: Span,
    ) -> Result<ExprCheck, TypeCheckingError> {
        if arguments.len() != 2 {
            self.emit(
                self.call_argument_arity_code,
                "MutableMap.put expects exactly 2 arguments (key, value)",
                call_span,
            )?;
            self.check_construction_operands(arguments)?;
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: receiver_falls_through,
            });
        }

        let key_arg = &arguments[0];
        let val_arg = &arguments[1];
        let key_result = self.check_expression(key_arg.value, Some(key_type), None)?;
        let val_result = self.check_expression(val_arg.value, Some(value_type), None)?;

        if !self.is_error(key_result.ty)
            && !self.is_deferred(key_result.ty)
            && !self.assignable(key_result.ty, key_type)
        {
            self.mismatch(
                self.ast().expressions().get(key_arg.value)?.span(),
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
                self.ast().expressions().get(val_arg.value)?.span(),
                None,
                val_result.ty,
                value_type,
            )?;
        }

        Ok(ExprCheck {
            ty: self.builtin(BuiltinType::Unit),
            falls_through: receiver_falls_through
                && key_result.falls_through
                && val_result.falls_through,
        })
    }

    fn check_map_remove_call(
        &mut self,
        receiver_falls_through: bool,
        key_type: TypeId,
        value_type: TypeId,
        arguments: &[CallArgument],
        call_span: Span,
    ) -> Result<ExprCheck, TypeCheckingError> {
        if arguments.len() != 1 {
            self.emit(
                self.call_argument_arity_code,
                "MutableMap.remove expects exactly 1 argument",
                call_span,
            )?;
            self.check_construction_operands(arguments)?;
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: receiver_falls_through,
            });
        }

        let key_arg = &arguments[0];
        let key_result = self.check_expression(key_arg.value, Some(key_type), None)?;
        if !self.is_error(key_result.ty)
            && !self.is_deferred(key_result.ty)
            && !self.assignable(key_result.ty, key_type)
        {
            self.mismatch(
                self.ast().expressions().get(key_arg.value)?.span(),
                None,
                key_result.ty,
                key_type,
            )?;
        }

        let nullable_val = self.types.intern(TypeKind::Nullable(value_type));
        Ok(ExprCheck {
            ty: nullable_val,
            falls_through: receiver_falls_through && key_result.falls_through,
        })
    }

    /// 检查 Map 下标查询 `map[key]`。
    pub(super) fn check_map_index(
        &mut self,
        expression: ExpressionId,
        _receiver: ExpressionId,
        receiver_result: ExprCheck,
        index: ExpressionId,
    ) -> Result<Option<ExprCheck>, TypeCheckingError> {
        let Some((_constructor, key_type, value_type)) = self.map_parts(receiver_result.ty) else {
            return Ok(None);
        };

        let index_result = self.check_expression(index, Some(key_type), None)?;
        if !self.assignable(index_result.ty, key_type)
            && !self.is_error(index_result.ty)
            && !self.is_deferred(index_result.ty)
        {
            self.emit(
                self.invalid_container_index_code,
                "map index key type does not match map key type",
                self.ast().expressions().get(index)?.span(),
            )?;
            return Ok(Some(ExprCheck {
                ty: self.error_type(),
                falls_through: receiver_result.falls_through && index_result.falls_through,
            }));
        }

        if self.is_error(index_result.ty) {
            return Ok(Some(ExprCheck {
                ty: self.error_type(),
                falls_through: receiver_result.falls_through && index_result.falls_through,
            }));
        }

        if self.copyability_of(value_type) == Copyability::MoveOnly {
            let code = codes::catalog()?
                .resolve(codes::MOVE_FROM_CONTAINER_ELEMENT)
                .expect("MOVE_FROM_CONTAINER_ELEMENT catalog resolve");
            self.emit(
                code,
                "cannot move MoveOnly map value by index; use borrow access",
                self.ast().expressions().get(expression)?.span(),
            )?;
            return Ok(Some(ExprCheck {
                ty: self.error_type(),
                falls_through: receiver_result.falls_through && index_result.falls_through,
            }));
        }

        let nullable_val = self.types.intern(TypeKind::Nullable(value_type));
        Ok(Some(ExprCheck {
            ty: nullable_val,
            falls_through: receiver_result.falls_through && index_result.falls_through,
        }))
    }

    /// 检查 Map 下标赋值 `mutableMap[key] = value`。
    pub(super) fn check_map_assignment(
        &mut self,
        target: ExpressionId,
        operator: AssignmentOperator,
        operator_span: Span,
        value: ExpressionId,
    ) -> Result<Option<ExprCheck>, TypeCheckingError> {
        let Expression::Index { receiver, index } =
            self.ast().expressions().get(target)?.payload().clone()
        else {
            return Ok(None);
        };

        let receiver_result = self.check_expression(receiver, None, None)?;
        let Some((constructor, key_type, value_type)) = self.map_parts(receiver_result.ty) else {
            return Ok(None);
        };

        if constructor != IntrinsicTypeConstructor::MutableMap {
            self.emit(
                self.immutable_container_place_code,
                "cannot mutate read-only Map; use MutableMap",
                operator_span,
            )?;
            self.check_expression(index, Some(key_type), None)?;
            self.check_expression(value, Some(value_type), None)?;
            return Ok(Some(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            }));
        }

        if operator != AssignmentOperator::Assign {
            self.emit(
                self.invalid_container_member_code,
                "Map subscript assignment supports only '=' operator",
                operator_span,
            )?;
        }

        let index_result = self.check_expression(index, Some(key_type), None)?;
        if !self.assignable(index_result.ty, key_type)
            && !self.is_error(index_result.ty)
            && !self.is_deferred(index_result.ty)
        {
            self.emit(
                self.invalid_container_index_code,
                "map index key type does not match map key type",
                self.ast().expressions().get(index)?.span(),
            )?;
        }

        let val_result = self.check_expression(value, Some(value_type), None)?;
        if !self.assignable(val_result.ty, value_type)
            && !self.is_error(val_result.ty)
            && !self.is_deferred(val_result.ty)
        {
            self.mismatch(
                self.ast().expressions().get(value)?.span(),
                None,
                val_result.ty,
                value_type,
            )?;
        }

        Ok(Some(ExprCheck {
            ty: self.builtin(BuiltinType::Unit),
            falls_through: receiver_result.falls_through
                && index_result.falls_through
                && val_result.falls_through,
        }))
    }
}
