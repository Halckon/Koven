use crate::{
    name_resolution::{Namespace, ReferenceTarget},
    parser::{ParameterModeMarker, TypeRef},
};

use super::*;

impl Checker<'_> {
    pub(super) fn resolve_type_ref(&mut self, id: TypeRefId) -> Result<TypeId, TypeCheckingError> {
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
                        Some(ReferenceTarget::Symbol(_)) => {
                            self.deferred(DeferredReason::NominalOrTypeParameter)
                        }
                        Some(ReferenceTarget::Unresolved | ReferenceTarget::LaterLocal(_))
                        | None => self.error_type(),
                        Some(
                            ReferenceTarget::OverloadSet(_)
                            | ReferenceTarget::ExternalOverloadSet(_),
                        ) => return Err(TypeCheckingError::InvalidExternalBinding),
                    };
                    if matches!(self.kind(base), TypeKind::Builtin(BuiltinType::Any)) {
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
