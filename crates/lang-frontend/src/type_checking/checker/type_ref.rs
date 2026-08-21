use crate::{
    name_resolution::{Namespace, ReferenceTarget},
    parser::{ParameterModeMarker, TypeRef},
};

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum TypeUse {
    Runtime,
    Static,
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
            } => {
                for segment in &segments {
                    for &argument in &segment.arguments {
                        self.resolve_type_ref(argument)?;
                    }
                }
                if segments.len() != 1 {
                    self.deferred(DeferredReason::QualifiedType)
                } else {
                    let segment = &segments[0];
                    let target = self.reference(segment.name_span, Namespace::Type).cloned();
                    let mut base = match target {
                        Some(ReferenceTarget::External(external)) => {
                            if !segment.arguments.is_empty() {
                                let primary =
                                    self.ast().type_refs().get(segment.arguments[0])?.span();
                                self.emit_with_label(
                                    self.builtin_arguments_code,
                                    "builtin type does not accept type arguments",
                                    primary,
                                    segment.name_span,
                                    "builtin type declared here",
                                )?;
                                self.error_type()
                            } else {
                                self.external_type(external)?
                            }
                        }
                        Some(ReferenceTarget::Symbol(symbol)) => {
                            match self.symbol_kinds.get(symbol.index()).copied() {
                                Some(crate::name_resolution::SymbolKind::TypeParameter) => {
                                    if segment.arguments.is_empty() {
                                        self.types.intern(TypeKind::TypeParameter(symbol))
                                    } else {
                                        self.emit_with_label(
                                            self.type_argument_arity_code,
                                            "type parameter does not accept type arguments",
                                            self.ast()
                                                .type_refs()
                                                .get(segment.arguments[0])?
                                                .span(),
                                            self.symbol_spans[symbol.index()],
                                            "type parameter declared here",
                                        )?;
                                        self.error_type()
                                    }
                                }
                                Some(crate::name_resolution::SymbolKind::Classifier) => {
                                    let Some(nominal) =
                                        self.nominal_by_symbol.get(&symbol).copied()
                                    else {
                                        return Err(TypeCheckingError::InvalidExternalBinding);
                                    };
                                    let expected = self
                                        .nominals
                                        .iter()
                                        .find(|descriptor| descriptor.id() == nominal)
                                        .map(|descriptor| descriptor.type_parameters().len())
                                        .ok_or(TypeCheckingError::InvalidExternalBinding)?;
                                    if segment.arguments.len() != expected {
                                        self.emit_with_label(
                                            self.type_argument_arity_code,
                                            "nominal type has the wrong number of type arguments",
                                            segment.name_span,
                                            self.symbol_spans[symbol.index()],
                                            format!("expected {expected} type arguments"),
                                        )?;
                                        self.error_type()
                                    } else {
                                        let arguments = segment
                                            .arguments
                                            .iter()
                                            .map(|&argument| self.resolve_type_ref(argument))
                                            .collect::<Result<Vec<_>, _>>()?;
                                        if arguments.iter().any(|&argument| self.is_error(argument))
                                        {
                                            self.error_type()
                                        } else {
                                            let is_interface =
                                                self.nominals.iter().any(|descriptor| {
                                                    descriptor.id() == nominal
                                                        && descriptor.kind()
                                                            == NominalKind::Interface
                                                });
                                            if usage == TypeUse::Runtime && is_interface {
                                                self.emit_with_label(
                                                    self.interface_runtime_value_code,
                                                    "interface cannot be used as a runtime value type without dyn",
                                                    self.ast().type_refs().get(id)?.span(),
                                                    self.symbol_spans[symbol.index()],
                                                    "interface declared here",
                                                )?;
                                                self.error_type()
                                            } else {
                                                self.types.intern(TypeKind::Nominal {
                                                    nominal,
                                                    arguments,
                                                })
                                            }
                                        }
                                    }
                                }
                                _ => self.error_type(),
                            }
                        }
                        Some(ReferenceTarget::Unresolved | ReferenceTarget::LaterLocal(_))
                        | None => self.error_type(),
                        Some(
                            ReferenceTarget::OverloadSet(_)
                            | ReferenceTarget::ExternalOverloadSet(_),
                        ) => return Err(TypeCheckingError::InvalidExternalBinding),
                    };
                    if usage == TypeUse::Runtime
                        && matches!(self.kind(base), TypeKind::Builtin(BuiltinType::Any))
                    {
                        base = self.deferred(DeferredReason::AnyValueRepresentation);
                    }
                    if nullable_span.is_some() && !self.is_error(base) && !self.is_deferred(base) {
                        self.types.intern(TypeKind::Nullable(base))
                    } else {
                        base
                    }
                }
            }
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
                        mode: parameter_mode(parameter.mode_marker),
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
}

fn parameter_mode(marker: Option<ParameterModeMarker>) -> ParameterMode {
    match marker {
        None => ParameterMode::Value,
        Some(ParameterModeMarker::Borrow(_)) => ParameterMode::Borrow,
        Some(ParameterModeMarker::Inout(_)) => ParameterMode::Inout,
    }
}
