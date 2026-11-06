use std::collections::BTreeSet;

use crate::{
    ast::TypeRefId,
    diagnostic::codes,
    name_resolution::SourceUnitId,
    parser::{TypePathSegment, TypeRef},
    type_checking::{IntrinsicTypeConstructor, NominalKind, TypeCheckingError, UnitTypeRefId},
};

use super::{CompilationUnitTypeError, SignatureCollector, UnitTypeId, UnitTypeKind};

impl SignatureCollector<'_> {
    pub(super) fn instantiate_box_type(
        &mut self,
        source: SourceUnitId,
        segment: &TypePathSegment,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let argument_ref = segment.arguments[0];
        if self.normalize_box_argument(source, argument_ref)? {
            return Ok(self.error_type());
        }
        let argument = self.type_ref_types[&UnitTypeRefId::new(source, argument_ref)];
        let valid = self
            .nominal_declaration(argument)
            .is_some_and(|declaration| {
                matches!(
                    self.nominals[&declaration].kind(),
                    NominalKind::ValueClass | NominalKind::EnumClass
                )
            });
        if !valid {
            let primary = self.inputs[source.index()]
                .ast()
                .type_refs()
                .get(argument_ref)
                .map_err(TypeCheckingError::from)?
                .span();
            self.emit(
                codes::INVALID_BOX_ARGUMENT,
                "Box type argument must be a concrete value class or enum class instance",
                primary,
            )?;
            return Ok(self.error_type());
        }
        Ok(self.types.intern(UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Box,
            arguments: vec![argument],
        }))
    }

    // Box payloads are runtime types even when the enclosing Box occurs in a static bound.
    // Walk only the argument's syntax, never nominal fields or layout edges. A nested intrinsic
    // Box already normalized its payload during resolution, so its subtree need not be revisited.
    fn normalize_box_argument(
        &mut self,
        source: SourceUnitId,
        root: TypeRefId,
    ) -> Result<bool, CompilationUnitTypeError> {
        let mut invalid = BTreeSet::new();
        let mut pending = vec![(root, false)];
        while let Some((id, visited)) = pending.pop() {
            let key = UnitTypeRefId::new(source, id);
            let ty = self.type_ref_types[&key];
            if self.is_error(ty) {
                invalid.insert(id.index());
                continue;
            }
            let (non_nullable, nullable) = match self.types.get(ty) {
                Some(UnitTypeKind::Nullable(inner)) => (*inner, true),
                _ => (ty, false),
            };
            if matches!(
                self.types.get(non_nullable),
                Some(UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::Box,
                    ..
                })
            ) {
                continue;
            }
            let node = self.inputs[source.index()]
                .ast()
                .type_refs()
                .get(id)
                .map_err(TypeCheckingError::from)?;
            let children = match node.payload() {
                TypeRef::Qualified { segments, .. } => segments
                    .last()
                    .map(|segment| segment.arguments.clone())
                    .unwrap_or_default(),
                TypeRef::Function {
                    parameters,
                    return_type,
                    ..
                } => parameters
                    .iter()
                    .map(|parameter| parameter.type_ref)
                    .chain(std::iter::once(*return_type))
                    .collect(),
                TypeRef::Error => Vec::new(),
            };
            if !visited {
                pending.push((id, true));
                pending.extend(children.into_iter().rev().map(|child| (child, false)));
                continue;
            }
            if children
                .iter()
                .any(|child| invalid.contains(&child.index()))
            {
                invalid.insert(id.index());
                let error = self.error_type();
                let recovery = match self.types.get(non_nullable) {
                    Some(UnitTypeKind::Nominal { declaration, .. }) => {
                        let declaration = *declaration;
                        let arguments = children
                            .iter()
                            .map(|child| {
                                if invalid.contains(&child.index()) {
                                    error
                                } else {
                                    self.type_ref_types[&UnitTypeRefId::new(source, *child)]
                                }
                            })
                            .collect();
                        // Keep this nominal's independent bound checks without changing the
                        // shared interned type or treating a poisoned child as a valid argument.
                        let nominal = self.types.intern(UnitTypeKind::Nominal {
                            declaration,
                            arguments,
                        });
                        if nullable {
                            self.types.intern(UnitTypeKind::Nullable(nominal))
                        } else {
                            nominal
                        }
                    }
                    _ => error,
                };
                self.type_ref_types.insert(key, recovery);
            }
            if let Some(declaration) = self.nominal_declaration(non_nullable)
                && self.nominals[&declaration].kind() == NominalKind::Interface
            {
                // The existing post-pass owns L0094 and independent nominal bounds.
                invalid.insert(id.index());
            }
        }
        Ok(invalid.contains(&root.index()))
    }
}
