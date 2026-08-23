use crate::{
    name_resolution::{Namespace, ReferenceTarget},
    parser::{TypePathSegment, TypeRef},
};

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum TypeUse {
    Runtime,
    Static,
    TypeTest,
}

impl Checker<'_> {
    pub(super) fn resolve_type_ref(&mut self, id: TypeRefId) -> Result<TypeId, TypeCheckingError> {
        self.resolve_type_ref_for(id, TypeUse::Runtime)
    }

    pub(super) fn resolve_static_type_ref(
        &mut self,
        id: TypeRefId,
    ) -> Result<TypeId, TypeCheckingError> {
        self.resolve_type_ref_for(id, TypeUse::Static)
    }

    pub(super) fn resolve_type_test_ref(
        &mut self,
        id: TypeRefId,
    ) -> Result<TypeId, TypeCheckingError> {
        self.resolve_type_ref_for(id, TypeUse::TypeTest)
    }

    fn resolve_type_ref_for(
        &mut self,
        id: TypeRefId,
        usage: TypeUse,
    ) -> Result<TypeId, TypeCheckingError> {
        if let Some(ty) = self.type_ref_types[id.index()] {
            return Ok(ty);
        }
        let payload = self.ast().type_refs().get(id)?.payload().clone();
        let ty = match payload {
            TypeRef::Error => self.error_type(),
            TypeRef::Qualified {
                segments,
                nullable_span,
            } => self.resolve_qualified_type(id, &segments, nullable_span, usage)?,
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
                    let ty = self.resolve_type_ref(parameter.type_ref)?;
                    contains_deferred |= self.is_deferred(ty);
                    contains_error |= self.is_error(ty);
                    resolved.push(FunctionParameterType {
                        mode: source_parameter_mode(parameter.mode_marker),
                        ty,
                    });
                }
                let return_type = self.resolve_type_ref(return_type)?;
                contains_deferred |= self.is_deferred(return_type);
                contains_error |= self.is_error(return_type);
                if contains_error {
                    self.error_type()
                } else if contains_deferred {
                    self.deferred(DeferredReason::FunctionContainsDeferred)
                } else {
                    self.types.intern(TypeKind::Function {
                        move_only: move_span.is_some(),
                        parameters: resolved,
                        return_type,
                    })
                }
            }
        };
        self.set_type_ref(id, ty);
        Ok(ty)
    }

    fn resolve_qualified_type(
        &mut self,
        id: TypeRefId,
        segments: &[TypePathSegment],
        nullable_span: Option<Span>,
        usage: TypeUse,
    ) -> Result<TypeId, TypeCheckingError> {
        for segment in segments {
            for &argument in &segment.arguments {
                self.resolve_type_ref(argument)?;
            }
        }
        let mut base = match segments {
            [segment] => self.resolve_named_segment(id, segment, usage, true)?,
            [root, case] => {
                let target = self.reference(case.name_span, Namespace::Type).cloned();
                let Some(ReferenceTarget::Symbol(case_symbol)) = target else {
                    return Ok(self.deferred(DeferredReason::QualifiedType));
                };
                let Some(case_id) = self.enum_case_by_type_symbol.get(&case_symbol).copied() else {
                    return Ok(self.deferred(DeferredReason::QualifiedType));
                };
                let root_target = self.reference(root.name_span, Namespace::Type).cloned();
                let descriptor = self
                    .enum_case(case_id)
                    .cloned()
                    .ok_or(TypeCheckingError::InvalidExternalBinding)?;
                if root_target != Some(ReferenceTarget::Symbol(descriptor.root().symbol())) {
                    return Ok(self.deferred(DeferredReason::QualifiedType));
                }
                self.resolve_case_type(id, case, usage, case_id, false)?
            }
            _ => self.deferred(DeferredReason::QualifiedType),
        };
        if usage == TypeUse::Runtime
            && matches!(self.kind(base), TypeKind::Builtin(BuiltinType::Any))
        {
            base = self.deferred(DeferredReason::AnyValueRepresentation);
        }
        if nullable_span.is_some() && !self.is_error(base) && !self.is_deferred(base) {
            Ok(self.types.intern(TypeKind::Nullable(base)))
        } else {
            Ok(base)
        }
    }

    fn resolve_named_segment(
        &mut self,
        id: TypeRefId,
        segment: &TypePathSegment,
        usage: TypeUse,
        allow_implicit_case_arguments: bool,
    ) -> Result<TypeId, TypeCheckingError> {
        match self.reference(segment.name_span, Namespace::Type).cloned() {
            Some(ReferenceTarget::External(external)) => {
                if let Some(ExternalTypeBinding::Intrinsic(constructor)) =
                    self.environment.binding(external).cloned()
                {
                    return self.resolve_intrinsic_segment(segment, constructor);
                }
                if segment.arguments.is_empty() {
                    return self.external_type(external);
                }
                self.emit_with_label(
                    self.builtin_arguments_code,
                    "builtin type does not accept type arguments",
                    self.ast().type_refs().get(segment.arguments[0])?.span(),
                    segment.name_span,
                    "builtin type declared here",
                )?;
                Ok(self.error_type())
            }
            Some(ReferenceTarget::Symbol(symbol)) => match self
                .symbol_kinds
                .get(symbol.index())
                .copied()
            {
                Some(crate::name_resolution::SymbolKind::TypeParameter) => {
                    if segment.arguments.is_empty() {
                        Ok(self.types.intern(TypeKind::TypeParameter(symbol)))
                    } else {
                        self.emit_with_label(
                            self.type_argument_arity_code,
                            "type parameter does not accept type arguments",
                            self.ast().type_refs().get(segment.arguments[0])?.span(),
                            self.symbol_spans[symbol.index()],
                            "type parameter declared here",
                        )?;
                        Ok(self.error_type())
                    }
                }
                Some(crate::name_resolution::SymbolKind::Classifier) => {
                    self.resolve_nominal_segment(id, segment, usage, symbol)
                }
                Some(crate::name_resolution::SymbolKind::EnumCaseType) => {
                    let case = self
                        .enum_case_by_type_symbol
                        .get(&symbol)
                        .copied()
                        .ok_or(TypeCheckingError::InvalidExternalBinding)?;
                    self.resolve_case_type(id, segment, usage, case, allow_implicit_case_arguments)
                }
                _ => Ok(self.error_type()),
            },
            Some(ReferenceTarget::Unresolved | ReferenceTarget::LaterLocal(_)) | None => {
                Ok(self.error_type())
            }
            Some(
                ReferenceTarget::OverloadSet(_)
                | ReferenceTarget::ExternalOverloadSet(_)
                | ReferenceTarget::EnumCasePayloadCandidates(_),
            ) => Err(TypeCheckingError::InvalidExternalBinding),
        }
    }

    fn resolve_intrinsic_segment(
        &mut self,
        segment: &TypePathSegment,
        constructor: IntrinsicTypeConstructor,
    ) -> Result<TypeId, TypeCheckingError> {
        if segment.arguments.len() != 1 {
            self.emit(
                self.type_argument_arity_code,
                "intrinsic type has the wrong number of type arguments",
                segment.name_span,
            )?;
            return Ok(self.error_type());
        }
        let argument_ref = segment.arguments[0];
        let argument = self.resolve_type_ref(argument_ref)?;
        if self.is_error(argument) {
            return Ok(self.error_type());
        }
        let valid = match (constructor, self.kind(argument)) {
            (IntrinsicTypeConstructor::Box, TypeKind::Nominal { nominal, .. }) => {
                self.nominals.iter().any(|descriptor| {
                    descriptor.id() == *nominal && descriptor.kind() == NominalKind::ValueClass
                })
            }
            (IntrinsicTypeConstructor::Box, _) => false,
            (
                IntrinsicTypeConstructor::Array
                | IntrinsicTypeConstructor::List
                | IntrinsicTypeConstructor::MutableList,
                _,
            ) => self.is_structurally_storable_type(argument),
        };
        if !valid {
            let (code, message) = if constructor == IntrinsicTypeConstructor::Box {
                (
                    self.invalid_box_argument_code,
                    "Box type argument must be a concrete value class instance",
                )
            } else {
                (
                    self.invalid_container_element_code,
                    "sequential container element type is not structurally storable",
                )
            };
            self.emit(
                code,
                message,
                self.ast().type_refs().get(argument_ref)?.span(),
            )?;
            return Ok(self.error_type());
        }
        Ok(self.types.intern(TypeKind::Intrinsic {
            constructor,
            arguments: vec![argument],
        }))
    }

    fn resolve_nominal_segment(
        &mut self,
        id: TypeRefId,
        segment: &TypePathSegment,
        usage: TypeUse,
        symbol: SymbolId,
    ) -> Result<TypeId, TypeCheckingError> {
        let nominal = self
            .nominal_by_symbol
            .get(&symbol)
            .copied()
            .ok_or(TypeCheckingError::InvalidExternalBinding)?;
        let descriptor = self
            .nominals
            .iter()
            .find(|descriptor| descriptor.id() == nominal)
            .cloned()
            .ok_or(TypeCheckingError::InvalidExternalBinding)?;
        if segment.arguments.len() != descriptor.type_parameters().len() {
            self.emit_with_label(
                self.type_argument_arity_code,
                "nominal type has the wrong number of type arguments",
                segment.name_span,
                self.symbol_spans[symbol.index()],
                format!(
                    "expected {} type arguments",
                    descriptor.type_parameters().len()
                ),
            )?;
            return Ok(self.error_type());
        }
        let arguments = segment
            .arguments
            .iter()
            .map(|&argument| self.resolve_type_ref(argument))
            .collect::<Result<Vec<_>, _>>()?;
        if arguments.iter().any(|&argument| self.is_error(argument)) {
            return Ok(self.error_type());
        }
        if usage == TypeUse::Runtime && descriptor.kind() == NominalKind::Interface {
            self.emit_with_label(
                self.interface_runtime_value_code,
                "interface cannot be used as a runtime value type without dyn",
                self.ast().type_refs().get(id)?.span(),
                self.symbol_spans[symbol.index()],
                "interface declared here",
            )?;
            return Ok(self.error_type());
        }
        Ok(self.types.intern(TypeKind::Nominal { nominal, arguments }))
    }

    fn resolve_case_type(
        &mut self,
        id: TypeRefId,
        segment: &TypePathSegment,
        usage: TypeUse,
        case: crate::name_resolution::EnumCaseId,
        allow_implicit_arguments: bool,
    ) -> Result<TypeId, TypeCheckingError> {
        let descriptor = self
            .enum_case(case)
            .cloned()
            .ok_or(TypeCheckingError::InvalidExternalBinding)?;
        if usage != TypeUse::TypeTest {
            self.emit_with_label(
                self.enum_case_type_position_code,
                "enum case type is only allowed as an is or !is target",
                self.ast().type_refs().get(id)?.span(),
                self.symbol_spans[descriptor.root().symbol().index()],
                "root enum declared here",
            )?;
            return Ok(self.error_type());
        }
        let root_descriptor = self
            .nominals
            .iter()
            .find(|nominal| nominal.id() == descriptor.root())
            .cloned()
            .ok_or(TypeCheckingError::InvalidExternalBinding)?;
        let root = if segment.arguments.is_empty() && allow_implicit_arguments {
            descriptor.root_type()
        } else if segment.arguments.len() == root_descriptor.type_parameters().len() {
            let arguments = segment
                .arguments
                .iter()
                .map(|&argument| self.resolve_type_ref(argument))
                .collect::<Result<Vec<_>, _>>()?;
            self.types.intern(TypeKind::Nominal {
                nominal: descriptor.root(),
                arguments,
            })
        } else {
            self.emit_with_label(
                self.type_argument_arity_code,
                "enum case type has the wrong number of root type arguments",
                segment.name_span,
                self.symbol_spans[descriptor.root().symbol().index()],
                format!(
                    "expected {} root type arguments",
                    root_descriptor.type_parameters().len()
                ),
            )?;
            return Ok(self.error_type());
        };
        Ok(self.types.intern(TypeKind::EnumCase { case, root }))
    }
}
