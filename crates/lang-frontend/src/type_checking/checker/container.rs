use crate::{
    ast::{ExpressionId, TypeRefId},
    name_resolution::{Namespace, ReferenceTarget},
    parser::{AssignmentOperator, CallArgument, Expression, LiteralKind, ParameterModeMarker},
    source::Span,
    type_checking::{ContainerConstructionKind, IntrinsicCallable},
};

use super::*;

#[derive(Clone, Copy)]
struct ContainerCall<'a> {
    expression: ExpressionId,
    callee: ExpressionId,
    span: Span,
    type_arguments: &'a [TypeRefId],
    arguments: &'a [CallArgument],
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
            return Ok(None);
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
                IntrinsicCallable::Replace | IntrinsicCallable::Swap => return Ok(None),
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

    fn check_list_form_construction(
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
        let element = if let Some(element) = explicit.or(contextual) {
            element
        } else if let Some(first) = arguments.first() {
            let first_result = if matches!(
                self.ast().expressions().get(first.value)?.payload(),
                Expression::Literal(LiteralKind::Null)
            ) {
                let nothing = self.builtin(BuiltinType::Nothing);
                let nullable_nothing = self.types.intern(TypeKind::Nullable(nothing));
                self.check_expression(first.value, Some(nullable_nothing), None)?
            } else {
                self.check_expression(first.value, None, None)?
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
                .check_expression(argument.value, Some(element), None)
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

    fn check_named_container_construction(
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
                    .check_expression(argument.value, Some(expected), None)
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

    fn check_call_operands(
        &mut self,
        callee: ExpressionId,
        arguments: &[CallArgument],
    ) -> Result<(), TypeCheckingError> {
        let deferred = self.deferred(DeferredReason::Call);
        self.set_expression(callee, deferred);
        for argument in arguments {
            self.check_expression(argument.value, None, None)?;
        }
        Ok(())
    }

    fn validate_construction_element(
        &mut self,
        element: TypeId,
        span: Span,
    ) -> Result<bool, TypeCheckingError> {
        if self.is_error(element) || self.is_deferred(element) {
            return Ok(false);
        }
        if self.is_structurally_storable_type(element) {
            return Ok(true);
        }
        self.emit(
            self.invalid_container_element_code,
            "sequential container element type is not structurally storable",
            span,
        )?;
        Ok(false)
    }

    pub(super) fn is_structurally_storable_type(&self, ty: TypeId) -> bool {
        match self.kind(ty) {
            TypeKind::Builtin(BuiltinType::Any | BuiltinType::Nothing) => false,
            TypeKind::Builtin(_) => true,
            TypeKind::Nullable(inner) => self.is_structurally_storable_type(*inner),
            TypeKind::Function { .. } => true,
            TypeKind::Nominal { nominal, arguments } => {
                self.nominals
                    .iter()
                    .find(|descriptor| descriptor.id() == *nominal)
                    .is_some_and(|descriptor| descriptor.kind() != NominalKind::Interface)
                    && !self.invalid_inline_nominals.contains(nominal)
                    && arguments
                        .iter()
                        .all(|argument| self.is_structurally_storable_type(*argument))
            }
            TypeKind::Intrinsic { .. } | TypeKind::TypeParameter(_) => true,
            TypeKind::EnumCase { root, .. } | TypeKind::StaticSelf(root) => {
                self.is_structurally_storable_type(*root)
            }
            TypeKind::Capability(_)
            | TypeKind::IntegerLiteral(_)
            | TypeKind::Error
            | TypeKind::Deferred(_) => false,
        }
    }

    pub(super) fn check_container_index(
        &mut self,
        expression: ExpressionId,
        receiver: ExpressionId,
        index: ExpressionId,
    ) -> Result<ExprCheck, TypeCheckingError> {
        let receiver_result = self.check_expression(receiver, None, None)?;
        let Some((container, element)) = self.container_parts(receiver_result.ty) else {
            self.check_expression(index, None, None)?;
            return Ok(ExprCheck {
                ty: self.deferred(DeferredReason::Index),
                falls_through: receiver_result.falls_through,
            });
        };
        let int = self.builtin(BuiltinType::Int);
        let index_result = self.check_expression(index, None, None)?;
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
            let value_result = self.check_expression(value, Some(place.element_type()), None)?;
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
            self.check_expression(value, None, None)?;
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
        receiver: TypeId,
        name: &str,
        name_span: Span,
    ) -> Result<Option<TypeId>, TypeCheckingError> {
        if self.container_parts(receiver).is_none() {
            return Ok(None);
        }
        if name == "size" {
            return Ok(Some(self.builtin(BuiltinType::Int)));
        }
        if matches!(name, "get" | "set") {
            self.emit(
                self.invalid_container_member_code,
                "sequential container indexing is available only through []",
                name_span,
            )?;
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
        Ok(self.container_parts(receiver_type).is_some()
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
            IntrinsicTypeConstructor::Box | IntrinsicTypeConstructor::Rc => None,
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
