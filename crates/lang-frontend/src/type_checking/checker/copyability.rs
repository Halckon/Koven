use std::collections::{BTreeMap, BTreeSet};

use super::*;

impl Checker<'_> {
    pub(super) fn all_copyabilities(&self) -> Vec<Copyability> {
        (0..self.types.len())
            .map(|index| self.copyability_of(TypeId::new(index)))
            .collect()
    }

    pub(super) fn copyability_of(&self, ty: TypeId) -> Copyability {
        self.copyability_with(ty, &BTreeMap::new(), &mut BTreeSet::new())
    }

    fn copyability_with(
        &self,
        ty: TypeId,
        substitutions: &BTreeMap<SymbolId, TypeId>,
        active: &mut BTreeSet<NominalId>,
    ) -> Copyability {
        match self.kind(ty) {
            TypeKind::Builtin(
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
            | TypeKind::IntegerLiteral(_) => Copyability::Copyable,
            TypeKind::Builtin(BuiltinType::String | BuiltinType::Any)
            | TypeKind::Function { .. }
            | TypeKind::Intrinsic { .. } => Copyability::MoveOnly,
            TypeKind::Nullable(inner) => self.copyability_with(*inner, substitutions, active),
            TypeKind::Nominal { nominal, arguments } => {
                self.nominal_copyability(*nominal, arguments, substitutions, active)
            }
            TypeKind::EnumCase { root, .. } => self.copyability_with(*root, substitutions, active),
            TypeKind::TypeParameter(symbol) => {
                if let Some(actual) = substitutions.get(symbol).copied()
                    && actual != ty
                {
                    return self.copyability_with(actual, substitutions, active);
                }
                match self
                    .type_parameters
                    .iter()
                    .find(|descriptor| descriptor.symbol() == *symbol)
                    .map(|descriptor| descriptor.bound())
                {
                    Some(TypeParameterBound::Capability(Capability::Copyable)) => {
                        Copyability::Copyable
                    }
                    Some(TypeParameterBound::Error) => Copyability::Error,
                    Some(
                        TypeParameterBound::Any
                        | TypeParameterBound::Interface(_)
                        | TypeParameterBound::Capability(
                            Capability::Transferable | Capability::Hashable,
                        ),
                    )
                    | None => Copyability::MoveOnly,
                }
            }
            TypeKind::Deferred(DeferredReason::AnyValueRepresentation) => Copyability::MoveOnly,
            TypeKind::Deferred(_) => Copyability::Unknown,
            TypeKind::Error | TypeKind::StaticSelf(_) | TypeKind::Capability(_) => {
                Copyability::Error
            }
        }
    }

    fn nominal_copyability(
        &self,
        nominal: NominalId,
        arguments: &[TypeId],
        outer: &BTreeMap<SymbolId, TypeId>,
        active: &mut BTreeSet<NominalId>,
    ) -> Copyability {
        if self.invalid_inline_nominals.contains(&nominal) || !active.insert(nominal) {
            return Copyability::Error;
        }
        let Some(descriptor) = self
            .nominals
            .iter()
            .find(|descriptor| descriptor.id() == nominal)
            .cloned()
        else {
            active.remove(&nominal);
            return Copyability::Error;
        };
        let result = match descriptor.kind() {
            NominalKind::Class | NominalKind::Object => Copyability::MoveOnly,
            NominalKind::Interface => Copyability::Error,
            NominalKind::ValueClass | NominalKind::EnumClass => {
                let mut substitutions = outer.clone();
                substitutions.extend(
                    descriptor
                        .type_parameters()
                        .iter()
                        .copied()
                        .zip(arguments.iter().copied()),
                );
                let component_types = if descriptor.kind() == NominalKind::ValueClass {
                    let Some(fields) = descriptor
                        .fields()
                        .iter()
                        .map(|field| self.symbol_type(*field))
                        .collect::<Option<Vec<_>>>()
                    else {
                        active.remove(&nominal);
                        return Copyability::Error;
                    };
                    fields
                } else {
                    self.enum_cases
                        .iter()
                        .filter(|case| case.root() == nominal)
                        .flat_map(|case| case.payloads().iter().map(|(_, ty)| *ty))
                        .collect::<Vec<_>>()
                };
                component_types
                    .into_iter()
                    .fold(Copyability::Copyable, |state, component| {
                        combine(
                            state,
                            self.copyability_with(component, &substitutions, active),
                        )
                    })
            }
        };
        active.remove(&nominal);
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
