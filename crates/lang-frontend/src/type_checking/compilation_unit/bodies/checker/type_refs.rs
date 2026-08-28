//! SPEC-0197 compilation-unit body 局部类型标注解析。

use std::collections::BTreeMap;

use crate::{
    ast::TypeRefId,
    diagnostic::codes,
    name_resolution::{Namespace, SourceUnitId, UnitReferenceTarget},
    parser::{ParameterModeMarker, TypePathSegment, TypeRef},
    source::Span,
    type_checking::{
        Capability, CompilationUnitTypeError, Copyability, DeferredReason, EnvironmentType,
        ExternalTypeBinding, IntrinsicTypeConstructor, NominalKind, ParameterMode,
        TypeCheckingError, UnitFunctionParameterType, UnitNominalSignature, UnitTypeId,
        UnitTypeKind, UnitTypeParameterBound, UnitTypeRefId,
    },
};

use super::{BodyChecker, copyability::UnitTransferability};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BodyTypeUse {
    Runtime,
    TypeTest,
}

impl BodyChecker<'_> {
    /// 解析 `is` / `!is` 的目标类型；case refinement 只由该调用路径消费。
    pub(super) fn resolve_type_test_ref(
        &mut self,
        source: SourceUnitId,
        id: TypeRefId,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        self.resolve_body_type_ref_for(source, id, BodyTypeUse::TypeTest)
    }

    /// 解析 body-local 标注及其泛型实参，并把结果写回 source-qualified facts。
    pub(super) fn resolve_body_type_ref(
        &mut self,
        source: SourceUnitId,
        id: TypeRefId,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        self.resolve_body_type_ref_for(source, id, BodyTypeUse::Runtime)
    }

    fn resolve_body_type_ref_for(
        &mut self,
        source: SourceUnitId,
        id: TypeRefId,
        usage: BodyTypeUse,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let key = UnitTypeRefId::new(source, id);
        if let Some(ty) = self
            .parts
            .type_ref_types
            .get(&key)
            .copied()
            .or_else(|| self.signatures.type_ref_type(key))
        {
            return Ok(ty);
        }
        let node = self
            .file(source)
            .ast()
            .type_refs()
            .get(id)
            .map_err(TypeCheckingError::from)?;
        let span = node.span();
        let mut ty = match node.payload().clone() {
            TypeRef::Error => self.error_type(),
            TypeRef::Function {
                move_span,
                parameters,
                return_type,
                ..
            } => {
                let mut resolved = Vec::with_capacity(parameters.len());
                let mut contains_deferred = false;
                let mut contains_error = false;
                for parameter in parameters {
                    let ty = self.resolve_body_type_ref(source, parameter.type_ref)?;
                    contains_deferred |= self.is_deferred(ty);
                    contains_error |= self.is_error(ty);
                    resolved.push(UnitFunctionParameterType::new(
                        parameter_mode(parameter.mode_marker),
                        ty,
                    ));
                }
                let return_type = self.resolve_body_type_ref(source, return_type)?;
                contains_deferred |= self.is_deferred(return_type);
                contains_error |= self.is_error(return_type);
                if contains_error {
                    self.error_type()
                } else if contains_deferred {
                    self.deferred_type(DeferredReason::FunctionContainsDeferred)
                } else {
                    self.signatures.types_mut().intern(UnitTypeKind::Function {
                        move_only: move_span.is_some(),
                        parameters: resolved,
                        return_type,
                    })
                }
            }
            TypeRef::Qualified {
                segments,
                nullable_span,
            } => self.resolve_qualified_body_type(source, id, &segments, nullable_span, usage)?,
        };
        let case_root = match self.signatures.types().get(ty) {
            Some(UnitTypeKind::EnumCase { root, .. }) => Some(*root),
            Some(UnitTypeKind::Nullable(inner)) => match self.signatures.types().get(*inner) {
                Some(UnitTypeKind::EnumCase { root, .. }) => Some(*root),
                _ => None,
            },
            _ => None,
        };
        if usage == BodyTypeUse::Runtime
            && let Some(root) = case_root
        {
            let UnitTypeKind::Nominal { declaration, .. } = self
                .signatures
                .types()
                .get(root)
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?
            else {
                return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
            };
            let root_span = self
                .names
                .names()
                .index()
                .declarations()
                .get(declaration.index())
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?
                .name_span();
            self.emit_maybe_label(
                crate::diagnostic::codes::ENUM_CASE_TYPE_POSITION,
                "enum case type is only allowed as an is or !is target",
                span,
                Some(root_span),
                "root enum declared here",
            )?;
            ty = self.error_type();
        }
        self.parts.type_ref_types.insert(key, ty);
        Ok(ty)
    }

    fn resolve_qualified_body_type(
        &mut self,
        source: SourceUnitId,
        id: TypeRefId,
        segments: &[TypePathSegment],
        nullable_span: Option<Span>,
        usage: BodyTypeUse,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        for segment in segments {
            for &argument in &segment.arguments {
                self.resolve_body_type_ref(source, argument)?;
            }
        }
        let Some(segment) = segments.last() else {
            return Ok(self.error_type());
        };
        let arguments = segment
            .arguments
            .iter()
            .map(|argument| self.resolve_body_type_ref(source, *argument))
            .collect::<Result<Vec<_>, _>>()?;
        let target = self
            .reference(source, segment.name_span, Namespace::Type)
            .cloned();
        let mut base = match target {
            Some(UnitReferenceTarget::Declaration(declaration)) => {
                self.instantiate_nominal_type(source, id, segment, declaration, arguments, usage)?
            }
            Some(UnitReferenceTarget::Symbol(symbol)) => self.instantiate_symbol_type(
                source,
                id,
                segment,
                symbol,
                arguments,
                usage,
                segments.len() == 1,
            )?,
            Some(UnitReferenceTarget::Symbols(symbols)) if symbols.len() == 1 => self
                .instantiate_symbol_type(
                    source,
                    id,
                    segment,
                    symbols[0],
                    arguments,
                    usage,
                    segments.len() == 1,
                )?,
            Some(UnitReferenceTarget::External(external)) => {
                self.instantiate_external_type(source, segment, external, arguments)?
            }
            _ => self.error_type(),
        };
        if usage == BodyTypeUse::Runtime
            && self.is_builtin(base, crate::type_checking::BuiltinType::Any)
        {
            base = self.deferred_type(DeferredReason::AnyValueRepresentation);
        }
        if nullable_span.is_some() && !self.is_error(base) && !self.is_deferred(base) {
            base = self
                .signatures
                .types_mut()
                .intern(UnitTypeKind::Nullable(base));
        }
        Ok(base)
    }

    #[allow(clippy::too_many_arguments)]
    fn instantiate_nominal_type(
        &mut self,
        source: SourceUnitId,
        id: TypeRefId,
        segment: &TypePathSegment,
        declaration: crate::name_resolution::DeclarationId,
        arguments: Vec<UnitTypeId>,
        usage: BodyTypeUse,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let Some(nominal) = self
            .signatures
            .declaration(declaration)
            .and_then(|signature| signature.nominal())
            .cloned()
        else {
            return Ok(self.error_type());
        };
        let declaration_span =
            self.names.names().index().declarations()[declaration.index()].name_span();
        if arguments.len() != nominal.type_parameters().len() {
            self.emit_maybe_label(
                codes::TYPE_ARGUMENT_ARITY,
                "nominal type has the wrong number of type arguments",
                segment.name_span,
                Some(declaration_span),
                format!(
                    "expected {} type arguments",
                    nominal.type_parameters().len()
                ),
            )?;
            return Ok(self.error_type());
        }
        if arguments.iter().any(|argument| self.is_error(*argument)) {
            return Ok(self.error_type());
        }
        let ty = self.signatures.types_mut().intern(UnitTypeKind::Nominal {
            declaration,
            arguments: arguments.clone(),
        });
        if usage == BodyTypeUse::Runtime && nominal.kind() == NominalKind::Interface {
            let primary = self
                .file(source)
                .ast()
                .type_refs()
                .get(id)
                .map_err(TypeCheckingError::from)?
                .span();
            self.emit_maybe_label(
                codes::INTERFACE_RUNTIME_VALUE,
                "interface cannot be used as a runtime value type without dyn",
                primary,
                Some(declaration_span),
                "interface declared here",
            )?;
            return Ok(self.error_type());
        }
        self.validate_nominal_type_arguments(source, segment, &nominal, &arguments)?;
        Ok(ty)
    }

    #[allow(clippy::too_many_arguments)]
    fn instantiate_symbol_type(
        &mut self,
        source: SourceUnitId,
        id: TypeRefId,
        segment: &TypePathSegment,
        symbol: crate::name_resolution::UnitSymbolId,
        arguments: Vec<UnitTypeId>,
        usage: BodyTypeUse,
        allow_implicit_case_arguments: bool,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let Some(ty) = self.signatures.symbol_type(symbol) else {
            return Ok(self.error_type());
        };
        match self.signatures.types().get(ty).cloned() {
            Some(UnitTypeKind::EnumCase { case, root }) => self.instantiate_case_type(
                source,
                id,
                segment,
                case,
                root,
                arguments,
                usage,
                allow_implicit_case_arguments,
            ),
            _ if arguments.is_empty() => Ok(ty),
            _ => {
                self.emit_maybe_label(
                    codes::TYPE_ARGUMENT_ARITY,
                    "type parameter does not accept type arguments",
                    self.argument_primary(source, segment)?,
                    Some(self.unit_symbol_span(symbol)?),
                    "type parameter declared here",
                )?;
                Ok(self.error_type())
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn instantiate_case_type(
        &mut self,
        source: SourceUnitId,
        id: TypeRefId,
        segment: &TypePathSegment,
        case: crate::name_resolution::UnitSymbolId,
        root: UnitTypeId,
        arguments: Vec<UnitTypeId>,
        usage: BodyTypeUse,
        allow_implicit_arguments: bool,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let UnitTypeKind::Nominal {
            declaration,
            arguments: implicit_arguments,
        } = self
            .signatures
            .types()
            .get(root)
            .cloned()
            .unwrap_or(UnitTypeKind::Error)
        else {
            return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
        };
        let nominal = self
            .signatures
            .declaration(declaration)
            .and_then(|signature| signature.nominal())
            .cloned()
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
        let declaration_span =
            self.names.names().index().declarations()[declaration.index()].name_span();
        if usage == BodyTypeUse::Runtime {
            let primary = self
                .file(source)
                .ast()
                .type_refs()
                .get(id)
                .map_err(TypeCheckingError::from)?
                .span();
            self.emit_maybe_label(
                codes::ENUM_CASE_TYPE_POSITION,
                "enum case type is only allowed as an is or !is target",
                primary,
                Some(declaration_span),
                "root enum declared here",
            )?;
            return Ok(self.error_type());
        }
        let root = if arguments.is_empty() && allow_implicit_arguments {
            self.signatures.types_mut().intern(UnitTypeKind::Nominal {
                declaration,
                arguments: implicit_arguments,
            })
        } else if arguments.len() == nominal.type_parameters().len() {
            self.validate_nominal_type_arguments(source, segment, &nominal, &arguments)?;
            self.signatures.types_mut().intern(UnitTypeKind::Nominal {
                declaration,
                arguments,
            })
        } else {
            self.emit_maybe_label(
                codes::TYPE_ARGUMENT_ARITY,
                "enum case type has the wrong number of root type arguments",
                segment.name_span,
                Some(declaration_span),
                format!(
                    "expected {} root type arguments",
                    nominal.type_parameters().len()
                ),
            )?;
            return Ok(self.error_type());
        };
        let ty = self
            .signatures
            .types_mut()
            .intern(UnitTypeKind::EnumCase { case, root });
        Ok(ty)
    }

    fn instantiate_external_type(
        &mut self,
        source: SourceUnitId,
        segment: &TypePathSegment,
        external: crate::name_resolution::ExternalSymbolId,
        arguments: Vec<UnitTypeId>,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        match self.environment.binding(external).cloned() {
            Some(ExternalTypeBinding::Builtin(builtin)) if arguments.is_empty() => {
                Ok(self.builtin(builtin))
            }
            Some(ExternalTypeBinding::Builtin(_)) => {
                self.emit_maybe_label(
                    codes::BUILTIN_TYPE_ARGUMENTS,
                    "builtin type does not accept type arguments",
                    self.argument_primary(source, segment)?,
                    Some(segment.name_span),
                    "builtin type declared here",
                )?;
                Ok(self.error_type())
            }
            Some(ExternalTypeBinding::Capability(capability)) if arguments.is_empty() => Ok(self
                .signatures
                .types_mut()
                .intern(UnitTypeKind::Capability(capability))),
            Some(ExternalTypeBinding::Capability(_)) => {
                self.emit_maybe_label(
                    codes::BUILTIN_TYPE_ARGUMENTS,
                    "builtin type does not accept type arguments",
                    self.argument_primary(source, segment)?,
                    Some(segment.name_span),
                    "builtin type declared here",
                )?;
                Ok(self.error_type())
            }
            Some(ExternalTypeBinding::Intrinsic(constructor)) => {
                self.instantiate_intrinsic_type(source, segment, constructor, arguments)
            }
            Some(ExternalTypeBinding::Value(ty)) if arguments.is_empty() => {
                Ok(self.normalize_environment_type(&ty))
            }
            Some(ExternalTypeBinding::Value(_)) => {
                self.emit_maybe_label(
                    codes::BUILTIN_TYPE_ARGUMENTS,
                    "external type does not accept type arguments",
                    self.argument_primary(source, segment)?,
                    Some(segment.name_span),
                    "builtin type declared here",
                )?;
                Ok(self.error_type())
            }
            None => Ok(self.deferred_type(DeferredReason::UnboundExternalType)),
            Some(ExternalTypeBinding::Function(_) | ExternalTypeBinding::IntrinsicCallable(_)) => {
                Ok(self.error_type())
            }
        }
    }

    fn instantiate_intrinsic_type(
        &mut self,
        source: SourceUnitId,
        segment: &TypePathSegment,
        constructor: IntrinsicTypeConstructor,
        arguments: Vec<UnitTypeId>,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        if arguments.len() != 1 {
            self.emit(
                codes::TYPE_ARGUMENT_ARITY,
                "intrinsic type has the wrong number of type arguments",
                segment.name_span,
            )?;
            return Ok(self.error_type());
        }
        let argument = arguments[0];
        if self.is_error(argument) {
            return Ok(self.error_type());
        }
        let valid = match (constructor, self.signatures.types().get(argument)) {
            (IntrinsicTypeConstructor::Box, Some(UnitTypeKind::Nominal { declaration, .. })) => {
                self.signatures
                    .declaration(*declaration)
                    .and_then(|signature| signature.nominal())
                    .is_some_and(|nominal| nominal.kind() == NominalKind::ValueClass)
            }
            (IntrinsicTypeConstructor::Box, _) => false,
            (
                IntrinsicTypeConstructor::Array
                | IntrinsicTypeConstructor::List
                | IntrinsicTypeConstructor::MutableList
                | IntrinsicTypeConstructor::Rc,
                _,
            ) => self.is_structurally_storable_type(argument),
        };
        if !valid {
            let (code, message) = if constructor == IntrinsicTypeConstructor::Box {
                (
                    codes::INVALID_BOX_ARGUMENT,
                    "Box type argument must be a concrete value class instance",
                )
            } else {
                (
                    codes::INVALID_CONTAINER_ELEMENT,
                    "sequential container element type is not structurally storable",
                )
            };
            let primary = self
                .file(source)
                .ast()
                .type_refs()
                .get(segment.arguments[0])
                .map_err(TypeCheckingError::from)?
                .span();
            self.emit(code, message, primary)?;
            return Ok(self.error_type());
        }
        Ok(self.signatures.types_mut().intern(UnitTypeKind::Intrinsic {
            constructor,
            arguments,
        }))
    }

    fn validate_nominal_type_arguments(
        &mut self,
        source: SourceUnitId,
        segment: &TypePathSegment,
        nominal: &UnitNominalSignature,
        arguments: &[UnitTypeId],
    ) -> Result<(), CompilationUnitTypeError> {
        let substitutions = nominal
            .type_parameters()
            .iter()
            .copied()
            .zip(arguments.iter().copied())
            .collect::<BTreeMap<_, _>>();
        for (index, (&parameter, &argument)) in
            nominal.type_parameters().iter().zip(arguments).enumerate()
        {
            if self.is_error(argument) {
                continue;
            }
            let Some(descriptor) = self.signatures.type_parameter(parameter) else {
                return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
            };
            let (failed, code, message, label) = match descriptor.bound() {
                UnitTypeParameterBound::Interface(bound) => {
                    let bound = self.substitute_type(bound, &substitutions)?;
                    (
                        !self.satisfies_interface(argument, bound)?,
                        codes::TYPE_ARGUMENT_BOUND,
                        "type argument does not satisfy its interface bound",
                        "interface bound declared here",
                    )
                }
                UnitTypeParameterBound::Capability(Capability::Copyable) => (
                    self.copyability_of(argument) == Copyability::MoveOnly,
                    codes::COPYABLE_TYPE_ARGUMENT_BOUND,
                    "type argument does not satisfy its Copyable bound",
                    "Copyable bound declared here",
                ),
                UnitTypeParameterBound::Capability(Capability::Transferable) => (
                    self.transferability_of(argument) == UnitTransferability::NotTransferable,
                    codes::TRANSFERABLE_TYPE_ARGUMENT_BOUND,
                    "type argument does not satisfy its Transferable bound",
                    "Transferable bound declared here",
                ),
                UnitTypeParameterBound::Any | UnitTypeParameterBound::Error => continue,
            };
            if failed {
                let primary = self
                    .file(source)
                    .ast()
                    .type_refs()
                    .get(segment.arguments[index])
                    .map_err(TypeCheckingError::from)?
                    .span();
                self.emit_maybe_label(
                    code,
                    message,
                    primary,
                    Some(self.unit_symbol_span(parameter)?),
                    label,
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn satisfies_interface(
        &mut self,
        actual: UnitTypeId,
        expected: UnitTypeId,
    ) -> Result<bool, CompilationUnitTypeError> {
        if actual == expected {
            return Ok(true);
        }
        match self.signatures.types().get(actual).cloned() {
            Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) => {
                let nominal = self
                    .signatures
                    .declaration(declaration)
                    .and_then(|signature| signature.nominal())
                    .cloned()
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                let substitutions = nominal
                    .type_parameters()
                    .iter()
                    .copied()
                    .zip(arguments)
                    .collect::<BTreeMap<_, _>>();
                for &interface in nominal.interfaces() {
                    if self.substitute_type(interface, &substitutions)? == expected {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Some(UnitTypeKind::TypeParameter(symbol)) => {
                let Some(UnitTypeParameterBound::Interface(bound)) = self
                    .signatures
                    .type_parameter(symbol)
                    .map(|descriptor| descriptor.bound())
                else {
                    return Ok(false);
                };
                self.satisfies_interface(bound, expected)
            }
            _ => Ok(false),
        }
    }

    fn is_structurally_storable_type(&self, ty: UnitTypeId) -> bool {
        match self.signatures.types().get(ty) {
            Some(UnitTypeKind::Builtin(
                crate::type_checking::BuiltinType::Any | crate::type_checking::BuiltinType::Nothing,
            )) => false,
            Some(UnitTypeKind::Builtin(_)) => true,
            Some(UnitTypeKind::Nullable(inner)) => self.is_structurally_storable_type(*inner),
            Some(UnitTypeKind::Function { .. }) => true,
            Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) => {
                self.signatures
                    .declaration(*declaration)
                    .and_then(|signature| signature.nominal())
                    .is_some_and(|nominal| nominal.kind() != NominalKind::Interface)
                    && !self
                        .signatures
                        .invalid_inline_nominals()
                        .contains(declaration)
                    && arguments
                        .iter()
                        .all(|argument| self.is_structurally_storable_type(*argument))
            }
            Some(UnitTypeKind::Intrinsic { .. } | UnitTypeKind::TypeParameter(_)) => true,
            Some(UnitTypeKind::EnumCase { root, .. } | UnitTypeKind::StaticSelf(root)) => {
                self.is_structurally_storable_type(*root)
            }
            Some(
                UnitTypeKind::Capability(_)
                | UnitTypeKind::IntegerLiteral(_)
                | UnitTypeKind::Error
                | UnitTypeKind::Deferred(_),
            )
            | None => false,
        }
    }

    fn normalize_environment_type(&mut self, ty: &EnvironmentType) -> UnitTypeId {
        match ty {
            EnvironmentType::Builtin(builtin) => self.builtin(*builtin),
            EnvironmentType::Nullable(inner) => {
                let inner = self.normalize_environment_type(inner);
                self.signatures
                    .types_mut()
                    .intern(UnitTypeKind::Nullable(inner))
            }
            EnvironmentType::Function {
                move_only,
                parameters,
                return_type,
            } => {
                let parameters = parameters
                    .iter()
                    .map(|parameter| {
                        let ty = self.normalize_environment_type(&parameter.ty);
                        UnitFunctionParameterType::new(parameter.mode, ty)
                    })
                    .collect();
                let return_type = self.normalize_environment_type(return_type);
                self.signatures.types_mut().intern(UnitTypeKind::Function {
                    move_only: *move_only,
                    parameters,
                    return_type,
                })
            }
        }
    }

    fn argument_primary(
        &self,
        source: SourceUnitId,
        segment: &TypePathSegment,
    ) -> Result<Span, CompilationUnitTypeError> {
        segment
            .arguments
            .first()
            .map_or(Ok(segment.name_span), |argument| {
                self.file(source)
                    .ast()
                    .type_refs()
                    .get(*argument)
                    .map(|node| node.span())
                    .map_err(TypeCheckingError::from)
                    .map_err(CompilationUnitTypeError::from)
            })
    }

    pub(super) fn unit_symbol_span(
        &self,
        symbol: crate::name_resolution::UnitSymbolId,
    ) -> Result<Span, CompilationUnitTypeError> {
        self.names.names().source_units()[symbol.source_unit().index()]
            .resolution()
            .symbols()
            .get(symbol.symbol().index())
            .map(|symbol| symbol.span())
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)
    }
}

const fn parameter_mode(marker: Option<ParameterModeMarker>) -> ParameterMode {
    match marker {
        None | Some(ParameterModeMarker::Borrow(_)) => ParameterMode::Borrow,
        Some(ParameterModeMarker::Own(_)) => ParameterMode::Value,
        Some(ParameterModeMarker::Inout(_)) => ParameterMode::Inout,
    }
}
