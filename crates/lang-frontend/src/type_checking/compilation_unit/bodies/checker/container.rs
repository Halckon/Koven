//! SPEC-0197 compilation-unit 核心顺序容器 construction。

use crate::{
    ast::{ExpressionId, TypeRefId},
    diagnostic::codes,
    name_resolution::{Namespace, SourceUnitId, UnitReferenceTarget},
    parser::{CallArgument, Expression, LiteralKind, ParameterModeMarker},
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypeError, ContainerConstructionKind, DeferredReason,
        ExternalTypeBinding, IntrinsicCallable, IntrinsicTypeConstructor, ParameterMode,
        SequentialContainerKind, TypeCheckingError, UnitContainerConstructionDescriptor,
        UnitExpressionId, UnitFunctionParameterType, UnitTypeId, UnitTypeKind,
    },
};

use super::{BodyChecker, ExpressionCheck};

#[derive(Clone, Copy)]
struct ContainerCall<'a> {
    source: SourceUnitId,
    expression: ExpressionId,
    callee: ExpressionId,
    span: Span,
    type_arguments: &'a [TypeRefId],
    arguments: &'a [CallArgument],
    expected: Option<UnitTypeId>,
    expected_span: Option<Span>,
    return_type: UnitTypeId,
}

#[derive(Clone, Copy)]
enum Target {
    ListForm(SequentialContainerKind),
    Named(SequentialContainerKind),
}

struct CheckedConstruction {
    container: SequentialContainerKind,
    shape: ContainerConstructionKind,
    element: UnitTypeId,
    modes: Vec<ParameterMode>,
    valid: bool,
}

impl BodyChecker<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_intrinsic_container_call(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        call_span: Span,
        callee: ExpressionId,
        type_arguments: &[TypeRefId],
        arguments: &[CallArgument],
        expected: Option<UnitTypeId>,
        expected_span: Option<Span>,
        return_type: UnitTypeId,
    ) -> Result<Option<ExpressionCheck>, CompilationUnitTypeError> {
        let Some(target) = self.container_target(source, callee)? else {
            return Ok(None);
        };
        let call = ContainerCall {
            source,
            expression,
            callee,
            span: call_span,
            type_arguments,
            arguments,
            expected,
            expected_span,
            return_type,
        };
        match target {
            Target::ListForm(kind) => self.check_list_form_construction(call, kind).map(Some),
            Target::Named(kind) => self
                .check_named_container_construction(call, kind)
                .map(Some),
        }
    }

    fn container_target(
        &self,
        source: SourceUnitId,
        callee: ExpressionId,
    ) -> Result<Option<Target>, CompilationUnitTypeError> {
        let node = self
            .file(source)
            .ast()
            .expressions()
            .get(callee)
            .map_err(TypeCheckingError::from)?;
        if !matches!(node.payload(), Expression::Name) {
            return Ok(None);
        }
        let span = node.span();
        let value_target = self.reference(source, span, Namespace::Value);
        if let Some(callable) = self.intrinsic_container_callable(value_target) {
            let kind = match callable {
                IntrinsicCallable::ArrayOf => SequentialContainerKind::Array,
                IntrinsicCallable::ListOf => SequentialContainerKind::List,
                IntrinsicCallable::MutableListOf => SequentialContainerKind::MutableList,
            };
            return Ok(Some(Target::ListForm(kind)));
        }
        if !matches!(value_target, None | Some(UnitReferenceTarget::Unresolved)) {
            return Ok(None);
        }
        let Some(UnitReferenceTarget::External(external)) =
            self.reference(source, span, Namespace::Type)
        else {
            return Ok(None);
        };
        let Some(ExternalTypeBinding::Intrinsic(constructor)) = self.environment.binding(*external)
        else {
            return Ok(None);
        };
        Ok(Self::sequential_kind(*constructor).map(Target::Named))
    }

    fn intrinsic_container_callable(
        &self,
        target: Option<&UnitReferenceTarget>,
    ) -> Option<IntrinsicCallable> {
        match target {
            Some(UnitReferenceTarget::External(external)) => {
                match self.environment.binding(*external) {
                    Some(ExternalTypeBinding::IntrinsicCallable(callable)) => Some(*callable),
                    _ => None,
                }
            }
            Some(UnitReferenceTarget::ExternalOverloadSet(externals)) => {
                externals
                    .iter()
                    .find_map(|external| match self.environment.binding(*external) {
                        Some(ExternalTypeBinding::IntrinsicCallable(callable)) => Some(*callable),
                        _ => None,
                    })
            }
            _ => None,
        }
    }

    fn check_list_form_construction(
        &mut self,
        call: ContainerCall<'_>,
        kind: SequentialContainerKind,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        if call.type_arguments.len() > 1 {
            self.emit(
                codes::TYPE_ARGUMENT_ARITY,
                "list-form container construction accepts at most one type argument",
                call.span,
            )?;
            self.check_container_operands(call)?;
            return Ok(self.failed_construction());
        }
        let invalid_shape = call
            .arguments
            .iter()
            .find(|argument| argument.named_prefix.is_some() || argument.mode_marker.is_some());
        if let Some(argument) = invalid_shape {
            let code = if argument.named_prefix.is_some() {
                codes::INVALID_NAMED_ARGUMENT
            } else {
                codes::CALL_ARGUMENT_MODE
            };
            self.emit(
                code,
                "list-form elements are repeated Value arguments",
                argument.span,
            )?;
        }
        let explicit = call
            .type_arguments
            .first()
            .map(|type_ref| self.resolve_body_type_ref(call.source, *type_ref))
            .transpose()?;
        let contextual = call.expected.and_then(|ty| {
            self.container_parts(ty)
                .filter(|(expected_kind, _)| *expected_kind == kind)
                .map(|(_, element)| element)
        });
        let element = if let Some(element) = explicit.or(contextual) {
            element
        } else if let Some(first) = call.arguments.first() {
            let first_result = if self.is_null_literal(call.source, first.value)? {
                let nothing = self.builtin(BuiltinType::Nothing);
                let nullable_nothing = self
                    .signatures
                    .types_mut()
                    .intern(UnitTypeKind::Nullable(nothing));
                self.check_expression(
                    call.source,
                    first.value,
                    Some(nullable_nothing),
                    None,
                    call.return_type,
                )?
            } else {
                self.check_expression(call.source, first.value, None, None, call.return_type)?
            };
            if self.is_builtin(first_result.ty, BuiltinType::Nothing)
                || self.is_nullable_nothing(first_result.ty)
            {
                let span = self
                    .file(call.source)
                    .ast()
                    .expressions()
                    .get(first.value)
                    .map_err(TypeCheckingError::from)?
                    .span();
                self.emit(
                    codes::CANNOT_INFER_CONTAINER_ELEMENT,
                    "cannot infer a storable element type from a bottom or null first element",
                    span,
                )?;
                self.error_type()
            } else {
                first_result.ty
            }
        } else {
            self.emit(
                codes::CANNOT_INFER_CONTAINER_ELEMENT,
                "empty list-form construction requires an expected or explicit element type",
                call.span,
            )?;
            self.error_type()
        };
        let mut valid = self.validate_container_element(element, call.span)?;
        for (index, argument) in call.arguments.iter().enumerate() {
            if index == 0 && explicit.is_none() && contextual.is_none() {
                valid &= !self.construction_type_contains_poison(element);
                continue;
            }
            let result = self.check_expression(
                call.source,
                argument.value,
                Some(element),
                None,
                call.return_type,
            )?;
            valid &= !self.construction_type_contains_poison(result.ty);
        }
        valid &= invalid_shape.is_none();
        self.finish_container_construction(
            call,
            CheckedConstruction {
                container: kind,
                shape: ContainerConstructionKind::ListForm,
                element,
                modes: vec![ParameterMode::Value; call.arguments.len()],
                valid,
            },
        )
    }

    fn check_named_container_construction(
        &mut self,
        call: ContainerCall<'_>,
        kind: SequentialContainerKind,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        if call.type_arguments.len() != 1 {
            self.emit(
                codes::TYPE_ARGUMENT_ARITY,
                "core container construction requires exactly one type argument",
                call.span,
            )?;
            self.check_container_operands(call)?;
            return Ok(self.failed_construction());
        }
        let element = self.resolve_body_type_ref(call.source, call.type_arguments[0])?;
        let mut valid = self.validate_container_element(element, call.span)?;
        let (shape, modes) = match kind {
            SequentialContainerKind::Array | SequentialContainerKind::List => (
                ContainerConstructionKind::RuntimeLength,
                vec![ParameterMode::Borrow, ParameterMode::Borrow],
            ),
            SequentialContainerKind::MutableList => {
                (ContainerConstructionKind::EmptyMutableList, Vec::new())
            }
        };
        if call.arguments.len() != modes.len() {
            self.emit(
                codes::INVALID_CONTAINER_CONSTRUCTION,
                "core container construction has the wrong number of arguments",
                call.span,
            )?;
            self.check_container_operands(call)?;
            valid = false;
        } else {
            let int = self.builtin(BuiltinType::Int);
            let initializer = self.signatures.types_mut().intern(UnitTypeKind::Function {
                move_only: false,
                parameters: vec![UnitFunctionParameterType::new(ParameterMode::Borrow, int)],
                return_type: element,
            });
            for (index, argument) in call.arguments.iter().enumerate() {
                if argument.named_prefix.is_some()
                    || !matches!(
                        argument.mode_marker,
                        None | Some(ParameterModeMarker::Borrow(_))
                    )
                {
                    self.emit(
                        codes::INVALID_CONTAINER_CONSTRUCTION,
                        "core container argument does not match its fixed parameter contract",
                        argument.span,
                    )?;
                    valid = false;
                }
                let expected = if index == 0 { int } else { initializer };
                let result = self.check_expression(
                    call.source,
                    argument.value,
                    Some(expected),
                    None,
                    call.return_type,
                )?;
                valid &= !self.construction_type_contains_poison(result.ty);
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
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        if !construction.valid || self.construction_type_contains_poison(construction.element) {
            return Ok(self.failed_construction());
        }
        let constructor = Self::container_constructor(construction.container);
        let container_type = self.signatures.types_mut().intern(UnitTypeKind::Intrinsic {
            constructor,
            arguments: vec![construction.element],
        });
        if !self.validate_construction_result(
            container_type,
            call.expected,
            call.span,
            call.expected_span,
        )? {
            return Ok(self.failed_construction());
        }
        let deferred = self.deferred_type(DeferredReason::Call);
        self.record_expression(call.source, call.callee, deferred);
        self.parts
            .container_constructions
            .push(UnitContainerConstructionDescriptor::new(
                UnitExpressionId::new(call.source, call.expression),
                construction.shape,
                construction.container,
                container_type,
                construction.element,
                construction.modes,
            ));
        Ok(ExpressionCheck {
            ty: container_type,
            falls_through: true,
        })
    }

    fn check_container_operands(
        &mut self,
        call: ContainerCall<'_>,
    ) -> Result<(), CompilationUnitTypeError> {
        self.check_construction_operands(call.source, call.arguments, call.return_type)
    }

    fn validate_container_element(
        &mut self,
        element: UnitTypeId,
        span: Span,
    ) -> Result<bool, CompilationUnitTypeError> {
        if self.construction_type_contains_poison(element) {
            return Ok(false);
        }
        if self.is_structurally_storable_type(element) {
            return Ok(true);
        }
        self.emit(
            codes::INVALID_CONTAINER_ELEMENT,
            "sequential container element type is not structurally storable",
            span,
        )?;
        Ok(false)
    }

    fn is_null_literal(
        &self,
        source: SourceUnitId,
        expression: ExpressionId,
    ) -> Result<bool, CompilationUnitTypeError> {
        Ok(matches!(
            self.file(source)
                .ast()
                .expressions()
                .get(expression)
                .map_err(TypeCheckingError::from)?
                .payload(),
            Expression::Literal(LiteralKind::Null)
        ))
    }

    fn is_nullable_nothing(&self, ty: UnitTypeId) -> bool {
        matches!(
            self.signatures.types().get(ty),
            Some(UnitTypeKind::Nullable(inner))
                if self.is_builtin(*inner, BuiltinType::Nothing)
        )
    }

    fn container_parts(&self, ty: UnitTypeId) -> Option<(SequentialContainerKind, UnitTypeId)> {
        let UnitTypeKind::Intrinsic {
            constructor,
            arguments,
        } = self.signatures.types().get(ty)?
        else {
            return None;
        };
        let kind = Self::sequential_kind(*constructor)?;
        match arguments.as_slice() {
            [element] => Some((kind, *element)),
            _ => None,
        }
    }

    pub(super) fn is_successful_container_callee(&self, expression: UnitExpressionId) -> bool {
        self.parts.container_constructions.iter().any(|descriptor| {
            let call = descriptor.expression();
            if call.source_unit() != expression.source_unit() {
                return false;
            }
            self.file(call.source_unit())
                .ast()
                .expressions()
                .get(call.expression())
                .is_ok_and(|node| {
                    matches!(
                        node.payload(),
                        Expression::Call { callee, .. } if *callee == expression.expression()
                    )
                })
        })
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
