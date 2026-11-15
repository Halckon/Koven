mod construction;
mod element;
mod mutation;
use crate::{
    ast::{ExpressionId, TypeRefId},
    name_resolution::{Namespace, ReferenceTarget},
    parser::{AssignmentOperator, CallArgument, Expression, LiteralKind, ParameterModeMarker},
    source::Span,
    type_checking::{
        ContainerConstructionKind, ContainerInsertAtDescriptor, ContainerRemoveAtDescriptor,
        ContainerRemoveFirstDescriptor, ContainerRemoveLastDescriptor, IntrinsicCallable,
    },
};

use super::*;

#[derive(Clone, Copy)]
pub(super) struct ContainerCall<'a> {
    pub(super) expression: ExpressionId,
    pub(super) callee: ExpressionId,
    pub(super) span: Span,
    pub(super) type_arguments: &'a [TypeRefId],
    pub(super) arguments: &'a [CallArgument],
}

struct CheckedConstruction {
    container: SequentialContainerKind,
    shape: ContainerConstructionKind,
    element: TypeId,
    modes: Vec<ParameterMode>,
    valid: bool,
}

impl Checker<'_> {
    pub(super) fn check_intrinsic_container_call(
        &mut self,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
        expected: Option<TypeId>,
    ) -> Result<Option<ExprCheck>, TypeCheckingError> {
        let call = ContainerCall {
            expression,
            callee,
            span: call_span,
            type_arguments,
            arguments,
        };
        let callee_node = self.ast().expressions().get(callee)?;
        if !matches!(callee_node.payload(), Expression::Name) {
            if let Some(res) = self.check_map_method_call(
                expression,
                call_span,
                callee,
                type_arguments,
                arguments,
            )? {
                return Ok(Some(res));
            }
            if let Some(res) = self.check_container_append_call(
                expression,
                call_span,
                callee,
                type_arguments,
                arguments,
            )? {
                return Ok(Some(res));
            }
            if let Some(res) = self.check_container_clear_call(
                expression,
                call_span,
                callee,
                type_arguments,
                arguments,
            )? {
                return Ok(Some(res));
            }
            if let Some(res) = self.check_container_remove_last_call(
                expression,
                call_span,
                callee,
                type_arguments,
                arguments,
            )? {
                return Ok(Some(res));
            }
            if let Some(res) = self.check_container_remove_first_call(
                expression,
                call_span,
                callee,
                type_arguments,
                arguments,
            )? {
                return Ok(Some(res));
            }
            if let Some(res) = self.check_container_insert_at_call(
                expression,
                call_span,
                callee,
                type_arguments,
                arguments,
            )? {
                return Ok(Some(res));
            }
            return self.check_container_remove_at_call(
                expression,
                call_span,
                callee,
                type_arguments,
                arguments,
            );
        }
        let callee_span = callee_node.span();
        let intrinsic_callable = match self.reference(callee_span, Namespace::Value).cloned() {
            Some(ReferenceTarget::External(external)) => self
                .environment
                .binding(external)
                .and_then(|binding| match binding {
                    ExternalTypeBinding::IntrinsicCallable(callable) => Some(*callable),
                    _ => None,
                }),
            Some(ReferenceTarget::ExternalOverloadSet(externals)) => externals
                .iter()
                .filter_map(|external| match self.environment.binding(*external) {
                    Some(ExternalTypeBinding::IntrinsicCallable(callable)) => Some(*callable),
                    _ => None,
                })
                .next(),
            _ => None,
        };
        if let Some(callable) = intrinsic_callable {
            let kind = match callable {
                IntrinsicCallable::ArrayOf => SequentialContainerKind::Array,
                IntrinsicCallable::ListOf => SequentialContainerKind::List,
                IntrinsicCallable::MutableListOf => SequentialContainerKind::MutableList,
                IntrinsicCallable::MapOf | IntrinsicCallable::MutableMapOf => {
                    return self
                        .check_map_construction_call(call, callable, expected)
                        .map(Some);
                }
                IntrinsicCallable::Replace
                | IntrinsicCallable::Swap
                | IntrinsicCallable::RangeView
                | IntrinsicCallable::Text(_) => return Ok(None),
            };
            return self
                .check_list_form_construction(call, kind, expected)
                .map(Some);
        }
        if let Some(ReferenceTarget::External(external)) =
            self.reference(callee_span, Namespace::Type).cloned()
            && let Some(ExternalTypeBinding::Intrinsic(constructor)) =
                self.environment.binding(external).cloned()
            && let Some(kind) = Self::sequential_kind(constructor)
        {
            return self
                .check_named_container_construction(call, kind)
                .map(Some);
        }
        Ok(None)
    }

    fn check_call_operands(
        &mut self,
        callee: ExpressionId,
        arguments: &[CallArgument],
    ) -> Result<(), TypeCheckingError> {
        let deferred = self.deferred(DeferredReason::Call);
        self.set_expression(callee, deferred);
        self.check_construction_operands(arguments)
    }

    pub(super) fn check_container_index(
        &mut self,
        expression: ExpressionId,
        receiver: ExpressionId,
        index: ExpressionId,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let receiver_result = self.check_expression(receiver, None, None)?;
        if let Some(res) = self.check_map_index(expression, receiver, receiver_result, index)? {
            return Ok(res);
        }
        let mut reachable = self.pending_expression_exit(receiver, receiver_result.falls_through);
        let index_result = self.check_pending_operand(index, None, None, &mut reachable)?;
        self.note_pending_exit(expression, reachable);
        let Some((container, element)) = self.container_parts(receiver_result.ty) else {
            return Ok(ExprCheck {
                ty: self.deferred(DeferredReason::Index),
                falls_through: receiver_result.falls_through,
            });
        };
        let int = self.builtin(BuiltinType::Int);
        if !self.assignable(index_result.ty, int)
            && !self.is_error(index_result.ty)
            && !self.is_deferred(index_result.ty)
        {
            self.emit(
                self.invalid_container_index_code,
                "sequential container index must have type Int",
                self.ast().expressions().get(index)?.span(),
            )?;
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: receiver_result.falls_through && index_result.falls_through,
            });
        }
        if self.is_error(index_result.ty) {
            return Ok(ExprCheck {
                ty: self.error_type(),
                falls_through: receiver_result.falls_through && index_result.falls_through,
            });
        }
        if self.is_deferred(index_result.ty) {
            return Ok(ExprCheck {
                ty: self.deferred(DeferredReason::Index),
                falls_through: receiver_result.falls_through && index_result.falls_through,
            });
        }
        self.element_places.push(ElementPlaceDescriptor::new(
            expression, receiver, index, container, element,
        ));
        Ok(ExprCheck {
            ty: element,
            falls_through: receiver_result.falls_through && index_result.falls_through,
        })
    }

    pub(super) fn check_container_assignment(
        &mut self,
        target: ExpressionId,
        operator: AssignmentOperator,
        operator_span: Span,
        value: ExpressionId,
    ) -> Result<Option<ExprCheck>, TypeCheckingError> {
        if let Some(res) = self.check_map_assignment(target, operator, operator_span, value)? {
            return Ok(Some(res));
        }
        self.check_expression(target, None, None)?;
        if let Some(place) = self.element_place_for_expression(target) {
            let mut valid = place.is_mutable();
            if !valid {
                self.emit(
                    self.immutable_container_place_code,
                    "List element place is read-only",
                    operator_span,
                )?;
            }
            let value_result =
                self.check_pending_receiver_operand(target, value, place.element_type())?;
            valid &= !self.is_error(value_result.ty);
            if operator != AssignmentOperator::Assign && !self.is_numeric(place.element_type()) {
                self.emit(
                    self.operands_code,
                    "compound element assignment requires a numeric element type",
                    operator_span,
                )?;
                valid = false;
            }
            return Ok(Some(ExprCheck {
                ty: if valid {
                    self.builtin(BuiltinType::Unit)
                } else {
                    self.error_type()
                },
                falls_through: value_result.falls_through,
            }));
        }
        if self.is_read_only_container_size(target)? {
            let mut reachable = self.pending_checked_exit(target);
            self.check_pending_operand(value, None, None, &mut reachable)?;
            self.emit(
                self.immutable_container_place_code,
                "sequential container size is read-only",
                operator_span,
            )?;
            return Ok(Some(ExprCheck {
                ty: self.error_type(),
                falls_through: true,
            }));
        }
        Ok(None)
    }

    pub(super) fn container_member_type(
        &mut self,
        expression: ExpressionId,
        receiver_expression: ExpressionId,
        receiver: TypeId,
        name: &str,
        name_span: Span,
    ) -> Result<Option<TypeId>, TypeCheckingError> {
        if let Some(res) =
            self.check_map_member_type(expression, receiver_expression, receiver, name, name_span)?
        {
            return Ok(Some(res));
        }
        if name == "size"
            && matches!(
                self.kind(receiver),
                TypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::View,
                    ..
                }
            )
        {
            let result = self.builtin(BuiltinType::Int);
            self.range_sizes
                .push(crate::type_checking::RangeSizeDescriptor {
                    expression,
                    receiver: receiver_expression,
                    receiver_type: receiver,
                    result_type: result,
                    span: self.ast().expressions().get(expression)?.span(),
                });
            return Ok(Some(result));
        }
        let Some((container, element)) = self.container_parts(receiver) else {
            return Ok(None);
        };
        if name == "size" {
            let result = self.builtin(BuiltinType::Int);
            self.container_sizes.push(ContainerSizeDescriptor::new(
                expression,
                receiver_expression,
                container,
                receiver,
                element,
                result,
                self.ast().expressions().get(expression)?.span(),
            ));
            return Ok(Some(result));
        }
        if matches!(name, "get" | "set") {
            self.emit(
                self.invalid_container_member_code,
                "sequential container indexing is available only through []",
                name_span,
            )?;
            return Ok(Some(self.error_type()));
        }
        if matches!(
            name,
            "add" | "clear" | "removeAt" | "removeLast" | "removeFirst" | "insertAt"
        ) {
            if container == SequentialContainerKind::MutableList {
                self.emit(
                    self.invalid_container_member_code,
                    &format!("MutableList.{name} must be called as a method"),
                    name_span,
                )?;
            } else {
                self.emit(
                    self.invalid_container_member_code,
                    &format!("sequential container does not support '{name}'"),
                    name_span,
                )?;
            }
            return Ok(Some(self.error_type()));
        }
        Ok(None)
    }

    pub(super) fn is_mutable_element_place(&self, expression: ExpressionId) -> Option<bool> {
        self.element_place_for_expression(expression)
            .map(|place| place.is_mutable())
    }

    pub(super) fn is_read_only_container_size(
        &self,
        expression: ExpressionId,
    ) -> Result<bool, TypeCheckingError> {
        if let Expression::Group { expression } =
            self.ast().expressions().get(expression)?.payload()
        {
            return self.is_read_only_container_size(*expression);
        }
        let Expression::Member {
            receiver,
            name_span,
            ..
        } = self.ast().expressions().get(expression)?.payload()
        else {
            return Ok(false);
        };
        let Some(receiver_type) = self.expression_types[receiver.index()] else {
            return Ok(false);
        };
        Ok((self.container_parts(receiver_type).is_some()
            || matches!(
                self.kind(receiver_type),
                TypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::View,
                    ..
                }
            ))
            && self.sources.slice(*name_span)? == "size")
    }

    fn element_place_for_expression(
        &self,
        expression: ExpressionId,
    ) -> Option<ElementPlaceDescriptor> {
        if let Some(place) = self
            .element_places
            .iter()
            .find(|place| place.expression() == expression)
            .copied()
        {
            return Some(place);
        }
        match self.ast().expressions().get(expression).ok()?.payload() {
            Expression::Group { expression } => self.element_place_for_expression(*expression),
            _ => None,
        }
    }

    pub(super) fn container_parts(&self, ty: TypeId) -> Option<(SequentialContainerKind, TypeId)> {
        let TypeKind::Intrinsic {
            constructor,
            arguments,
        } = self.kind(ty)
        else {
            return None;
        };
        let kind = Self::sequential_kind(*constructor)?;
        Some((kind, *arguments.first()?))
    }

    fn sequential_kind(constructor: IntrinsicTypeConstructor) -> Option<SequentialContainerKind> {
        match constructor {
            IntrinsicTypeConstructor::Array => Some(SequentialContainerKind::Array),
            IntrinsicTypeConstructor::List => Some(SequentialContainerKind::List),
            IntrinsicTypeConstructor::MutableList => Some(SequentialContainerKind::MutableList),
            IntrinsicTypeConstructor::Box
            | IntrinsicTypeConstructor::Rc
            | IntrinsicTypeConstructor::Map
            | IntrinsicTypeConstructor::MutableMap
            | IntrinsicTypeConstructor::View => None,
        }
    }

    fn container_constructor(kind: SequentialContainerKind) -> IntrinsicTypeConstructor {
        match kind {
            SequentialContainerKind::Array => IntrinsicTypeConstructor::Array,
            SequentialContainerKind::List => IntrinsicTypeConstructor::List,
            SequentialContainerKind::MutableList => IntrinsicTypeConstructor::MutableList,
        }
    }
}
