//! SPEC-0197 compilation-unit body 的封闭 `Copyable` 判定。

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    name_resolution::{DeclarationId, UnitSymbolId},
    type_checking::{
        BuiltinType, Capability, Copyability, DeferredReason, NominalKind, UnitTypeId,
        UnitTypeKind, UnitTypeParameterBound,
    },
};

use super::BodyChecker;

impl BodyChecker<'_> {
    pub(super) fn copyability_of(&self, ty: UnitTypeId) -> Copyability {
        self.copyability_with(ty, &BTreeMap::new(), &mut BTreeSet::new())
    }

    fn copyability_with(
        &self,
        ty: UnitTypeId,
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
        active: &mut BTreeSet<DeclarationId>,
    ) -> Copyability {
        match self.signatures.types().get(ty) {
            Some(
                UnitTypeKind::Builtin(
                    BuiltinType::Byte
                    | BuiltinType::Short
                    | BuiltinType::Int
                    | BuiltinType::Long
                    | BuiltinType::UByte
                    | BuiltinType::UShort
                    | BuiltinType::UInt
                    | BuiltinType::ULong
                    | BuiltinType::Float
                    | BuiltinType::Double
                    | BuiltinType::Boolean
                    | BuiltinType::Char
                    | BuiltinType::Unit
                    | BuiltinType::Nothing,
                )
                | UnitTypeKind::IntegerLiteral(_),
            ) => Copyability::Copyable,
            Some(
                UnitTypeKind::Builtin(BuiltinType::String | BuiltinType::Any)
                | UnitTypeKind::Function { .. }
                | UnitTypeKind::Intrinsic { .. },
            ) => Copyability::MoveOnly,
            Some(UnitTypeKind::Nullable(inner)) => {
                self.copyability_with(*inner, substitutions, active)
            }
            Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) => self.nominal_copyability(*declaration, arguments, substitutions, active),
            Some(UnitTypeKind::EnumCase { root, .. }) => {
                self.copyability_with(*root, substitutions, active)
            }
            Some(UnitTypeKind::TypeParameter(symbol)) => {
                if let Some(actual) = substitutions.get(symbol).copied()
                    && actual != ty
                {
                    return self.copyability_with(actual, substitutions, active);
                }
                match self
                    .signatures
                    .type_parameter(*symbol)
                    .map(|item| item.bound())
                {
                    Some(UnitTypeParameterBound::Capability(Capability::Copyable)) => {
                        Copyability::Copyable
                    }
                    Some(UnitTypeParameterBound::Error) => Copyability::Error,
                    _ => Copyability::MoveOnly,
                }
            }
            Some(UnitTypeKind::Deferred(DeferredReason::AnyValueRepresentation)) => {
                Copyability::MoveOnly
            }
            Some(UnitTypeKind::Deferred(_)) => Copyability::Unknown,
            Some(
                UnitTypeKind::Error | UnitTypeKind::StaticSelf(_) | UnitTypeKind::Capability(_),
            )
            | None => Copyability::Error,
        }
    }

    fn nominal_copyability(
        &self,
        declaration: DeclarationId,
        arguments: &[UnitTypeId],
        outer: &BTreeMap<UnitSymbolId, UnitTypeId>,
        active: &mut BTreeSet<DeclarationId>,
    ) -> Copyability {
        if !active.insert(declaration) {
            return Copyability::Error;
        }
        let Some(nominal) = self
            .signatures
            .declaration(declaration)
            .and_then(|signature| signature.nominal())
        else {
            active.remove(&declaration);
            return Copyability::Error;
        };
        let result = match nominal.kind() {
            NominalKind::Class | NominalKind::Object => Copyability::MoveOnly,
            NominalKind::Interface => Copyability::Error,
            NominalKind::ValueClass | NominalKind::EnumClass => {
                let mut substitutions = outer.clone();
                substitutions.extend(
                    nominal
                        .type_parameters()
                        .iter()
                        .copied()
                        .zip(arguments.iter().copied()),
                );
                let components = if nominal.kind() == NominalKind::ValueClass {
                    nominal
                        .fields()
                        .iter()
                        .map(|field| field.ty())
                        .collect::<Vec<_>>()
                } else {
                    nominal
                        .enum_cases()
                        .iter()
                        .flat_map(|case| case.payloads().iter().map(|payload| payload.ty()))
                        .collect::<Vec<_>>()
                };
                components
                    .into_iter()
                    .fold(Copyability::Copyable, |state, component| {
                        combine(
                            state,
                            self.copyability_with(component, &substitutions, active),
                        )
                    })
            }
        };
        active.remove(&declaration);
        result
    }
}

fn combine(left: Copyability, right: Copyability) -> Copyability {
    match (left, right) {
        (Copyability::Error, _) | (_, Copyability::Error) => Copyability::Error,
        (Copyability::Unknown, _) | (_, Copyability::Unknown) => Copyability::Unknown,
        (Copyability::MoveOnly, _) | (_, Copyability::MoveOnly) => Copyability::MoveOnly,
        (Copyability::Copyable, Copyability::Copyable) => Copyability::Copyable,
    }
}
