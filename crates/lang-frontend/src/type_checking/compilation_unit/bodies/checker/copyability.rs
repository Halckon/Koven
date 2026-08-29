//! SPEC-0197 compilation-unit body 的封闭 `Copyable` 判定。

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    name_resolution::{DeclarationId, UnitSymbolId},
    type_checking::{
        BuiltinType, Capability, CompilationUnitSignatures, Copyability, DeferredReason,
        NominalKind, UnitTypeId, UnitTypeKind, UnitTypeParameterBound,
    },
};

use super::BodyChecker;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UnitTransferability {
    Transferable,
    NotTransferable,
    Unknown,
    Error,
}

struct CapabilityQuery<'a> {
    signatures: &'a CompilationUnitSignatures,
}

impl CapabilityQuery<'_> {
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
        if self
            .signatures
            .invalid_inline_nominals()
            .contains(&declaration)
            || !active.insert(declaration)
        {
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

    pub(super) fn transferability_of(&self, ty: UnitTypeId) -> UnitTransferability {
        self.transferability_with(ty, &BTreeMap::new(), &mut BTreeSet::new())
    }

    fn transferability_with(
        &self,
        ty: UnitTypeId,
        substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
        active: &mut BTreeSet<DeclarationId>,
    ) -> UnitTransferability {
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
                    | BuiltinType::String
                    | BuiltinType::Unit
                    | BuiltinType::Nothing,
                )
                | UnitTypeKind::IntegerLiteral(_),
            ) => UnitTransferability::Transferable,
            Some(UnitTypeKind::Builtin(BuiltinType::Any) | UnitTypeKind::Function { .. }) => {
                UnitTransferability::NotTransferable
            }
            Some(UnitTypeKind::Nullable(inner)) => {
                self.transferability_with(*inner, substitutions, active)
            }
            Some(UnitTypeKind::Intrinsic {
                constructor: crate::type_checking::IntrinsicTypeConstructor::Rc,
                ..
            }) => UnitTransferability::NotTransferable,
            Some(UnitTypeKind::Intrinsic { arguments, .. }) => {
                arguments
                    .iter()
                    .fold(UnitTransferability::Transferable, |state, argument| {
                        combine_transferability(
                            state,
                            self.transferability_with(*argument, substitutions, active),
                        )
                    })
            }
            Some(UnitTypeKind::Nominal {
                declaration,
                arguments,
            }) => self.nominal_transferability(*declaration, arguments, substitutions, active),
            Some(UnitTypeKind::EnumCase { root, .. }) => {
                self.transferability_with(*root, substitutions, active)
            }
            Some(UnitTypeKind::TypeParameter(symbol)) => {
                if let Some(actual) = substitutions.get(symbol).copied()
                    && actual != ty
                {
                    return self.transferability_with(actual, substitutions, active);
                }
                match self
                    .signatures
                    .type_parameter(*symbol)
                    .map(|item| item.bound())
                {
                    Some(UnitTypeParameterBound::Capability(Capability::Transferable)) => {
                        UnitTransferability::Transferable
                    }
                    Some(UnitTypeParameterBound::Error) => UnitTransferability::Error,
                    _ => UnitTransferability::NotTransferable,
                }
            }
            Some(UnitTypeKind::Deferred(DeferredReason::AnyValueRepresentation)) => {
                UnitTransferability::NotTransferable
            }
            Some(UnitTypeKind::Deferred(_)) => UnitTransferability::Unknown,
            Some(
                UnitTypeKind::Error | UnitTypeKind::StaticSelf(_) | UnitTypeKind::Capability(_),
            )
            | None => UnitTransferability::Error,
        }
    }

    fn nominal_transferability(
        &self,
        declaration: DeclarationId,
        arguments: &[UnitTypeId],
        outer: &BTreeMap<UnitSymbolId, UnitTypeId>,
        active: &mut BTreeSet<DeclarationId>,
    ) -> UnitTransferability {
        if self
            .signatures
            .invalid_inline_nominals()
            .contains(&declaration)
        {
            return UnitTransferability::Error;
        }
        if !active.insert(declaration) {
            return UnitTransferability::Transferable;
        }
        let Some(nominal) = self
            .signatures
            .declaration(declaration)
            .and_then(|signature| signature.nominal())
        else {
            active.remove(&declaration);
            return UnitTransferability::Error;
        };
        let result = match nominal.kind() {
            NominalKind::Object => UnitTransferability::NotTransferable,
            NominalKind::Interface => UnitTransferability::Error,
            NominalKind::Class | NominalKind::ValueClass | NominalKind::EnumClass => {
                let mut substitutions = outer.clone();
                substitutions.extend(
                    nominal
                        .type_parameters()
                        .iter()
                        .copied()
                        .zip(arguments.iter().copied()),
                );
                let components = if nominal.kind() == NominalKind::EnumClass {
                    nominal
                        .enum_cases()
                        .iter()
                        .flat_map(|case| case.payloads().iter().map(|payload| payload.ty()))
                        .collect::<Vec<_>>()
                } else {
                    nominal
                        .fields()
                        .iter()
                        .map(|field| field.ty())
                        .collect::<Vec<_>>()
                };
                components.into_iter().fold(
                    UnitTransferability::Transferable,
                    |state, component| {
                        combine_transferability(
                            state,
                            self.transferability_with(component, &substitutions, active),
                        )
                    },
                )
            }
        };
        active.remove(&declaration);
        result
    }
}

pub(in crate::type_checking::compilation_unit) fn unit_copyability(
    signatures: &CompilationUnitSignatures,
    ty: UnitTypeId,
) -> Copyability {
    CapabilityQuery { signatures }.copyability_of(ty)
}

impl BodyChecker<'_> {
    pub(super) fn copyability_of(&self, ty: UnitTypeId) -> Copyability {
        unit_copyability(&self.signatures, ty)
    }

    pub(super) fn transferability_of(&self, ty: UnitTypeId) -> UnitTransferability {
        CapabilityQuery {
            signatures: &self.signatures,
        }
        .transferability_of(ty)
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

fn combine_transferability(
    left: UnitTransferability,
    right: UnitTransferability,
) -> UnitTransferability {
    match (left, right) {
        (UnitTransferability::Error, _) | (_, UnitTransferability::Error) => {
            UnitTransferability::Error
        }
        (UnitTransferability::Unknown, _) | (_, UnitTransferability::Unknown) => {
            UnitTransferability::Unknown
        }
        (UnitTransferability::NotTransferable, _) | (_, UnitTransferability::NotTransferable) => {
            UnitTransferability::NotTransferable
        }
        (UnitTransferability::Transferable, UnitTransferability::Transferable) => {
            UnitTransferability::Transferable
        }
    }
}
