//! Scratch shapes preserve source external identities erased by body recovery.

use std::collections::BTreeSet;

use crate::{
    ast::TypeRefId,
    name_resolution::{Namespace, SourceUnitId, UnitReferenceTarget},
    parser::{TypePathSegment, TypeRef},
    type_checking::{
        DeferredReason, TypeCheckingError, UnitFunctionParameterType, UnitTypeId, UnitTypeKind,
        UnitTypeRefId,
    },
};

use super::super::{type_graph::children, type_refs::parameter_mode};
use super::{BodyChecker, CompilationUnitTypeError};

impl BodyChecker<'_> {
    /// Called inside the comparison checkpoint. The result may only be used
    /// as a pattern and must never become a committed expression or TypeRef fact.
    pub(super) fn explicit_constraint_type(
        &mut self,
        source: SourceUnitId,
        id: TypeRefId,
    ) -> Result<Option<UnitTypeId>, CompilationUnitTypeError> {
        let Some(pattern) = self.constraint_ref(source, id)? else {
            return Ok(None);
        };
        let types = self.signatures.types();
        let mut visited = BTreeSet::new();
        let mut pending = vec![pattern];
        while let Some(ty) = pending.pop() {
            if visited.insert(ty)
                && let Some(kind) = types.get(ty)
            {
                if matches!(
                    kind,
                    UnitTypeKind::Deferred(DeferredReason::ExternalConstraint(_))
                ) {
                    return Ok(Some(pattern));
                }
                pending.extend(children(kind));
            }
        }
        Ok(None)
    }

    fn constraint_ref(
        &mut self,
        source: SourceUnitId,
        id: TypeRefId,
    ) -> Result<Option<UnitTypeId>, CompilationUnitTypeError> {
        let key = UnitTypeRefId::new(source, id);
        let Some(raw) = self
            .parts
            .type_ref_types
            .get(&key)
            .copied()
            .or_else(|| self.signatures.type_ref_type(key))
        else {
            return Ok(None);
        };
        if self.is_error(raw) {
            return Ok(Some(raw));
        }
        let node = self
            .file(source)
            .ast()
            .type_refs()
            .get(id)
            .map_err(TypeCheckingError::from)?
            .payload()
            .clone();
        let ty = match node {
            TypeRef::Qualified {
                segments,
                nullable_span,
            } => {
                let base = self.qualified_constraint_type(source, raw, &segments)?;
                match base {
                    Some(base) if nullable_span.is_some() => self
                        .signatures
                        .types_mut()
                        .intern(UnitTypeKind::Nullable(base)),
                    Some(base) => base,
                    None => raw,
                }
            }
            TypeRef::Function {
                move_span,
                parameters,
                return_type,
                ..
            } => {
                let mut resolved = Vec::with_capacity(parameters.len());
                for parameter in parameters {
                    let Some(ty) = self.constraint_ref(source, parameter.type_ref)? else {
                        return Ok(Some(raw));
                    };
                    resolved.push(UnitFunctionParameterType::new(
                        parameter_mode(parameter.mode_marker),
                        ty,
                    ));
                }
                let Some(return_type) = self.constraint_ref(source, return_type)? else {
                    return Ok(Some(raw));
                };
                self.signatures.types_mut().intern(UnitTypeKind::Function {
                    move_only: move_span.is_some(),
                    parameters: resolved,
                    return_type,
                })
            }
            TypeRef::Error => raw,
        };
        Ok(Some(ty))
    }

    fn qualified_constraint_type(
        &mut self,
        source: SourceUnitId,
        raw: UnitTypeId,
        segments: &[TypePathSegment],
    ) -> Result<Option<UnitTypeId>, CompilationUnitTypeError> {
        let Some(segment) = segments.last() else {
            return Ok(None);
        };
        let types = self.signatures.types();
        let base = match types.get(raw) {
            Some(UnitTypeKind::Nullable(inner)) => *inner,
            _ => raw,
        };
        if matches!(
            types.get(base),
            Some(UnitTypeKind::Deferred(DeferredReason::UnboundExternalType))
        ) && segments.iter().all(|segment| segment.arguments.is_empty())
            && let Some(UnitReferenceTarget::External(external)) =
                self.reference(source, segment.name_span, Namespace::Type)
            && self.environment.binding(*external).is_none()
        {
            return Ok(Some(
                self.deferred_type(DeferredReason::ExternalConstraint(*external)),
            ));
        }
        let Some(kind) = types.get(base).cloned() else {
            return Ok(None);
        };
        let mut arguments = Vec::with_capacity(segment.arguments.len());
        match &kind {
            UnitTypeKind::Nominal { arguments: raw, .. }
            | UnitTypeKind::Intrinsic { arguments: raw, .. }
                if raw.len() == segment.arguments.len() => {}
            _ => return Ok(None),
        }
        for &reference in &segment.arguments {
            let Some(ty) = self.constraint_ref(source, reference)? else {
                return Ok(None);
            };
            arguments.push(ty);
        }
        let kind = match kind {
            UnitTypeKind::Nominal { declaration, .. } => UnitTypeKind::Nominal {
                declaration,
                arguments,
            },
            UnitTypeKind::Intrinsic { constructor, .. } => UnitTypeKind::Intrinsic {
                constructor,
                arguments,
            },
            _ => return Ok(None),
        };
        Ok(Some(self.signatures.types_mut().intern(kind)))
    }
}
