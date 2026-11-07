//! Finite nominal-parameter obligations for sequential elements and Rc payloads.
use std::collections::{BTreeMap, BTreeSet};

use super::{
    UnitNominalSignature, UnitTypeId, UnitTypeKind, UnitTypeParameterBound,
    UnitTypeParameterDescriptor, UnitTypeTable,
};
use crate::{
    name_resolution::{DeclarationId, UnitSymbolId},
    type_checking::IntrinsicTypeConstructor,
};

pub(super) type StorageRequirements = BTreeMap<DeclarationId, BTreeSet<UnitSymbolId>>;

/// Propagate obligations over declarations, without expanding recursive heap instances.
pub(super) fn collect(
    types: &UnitTypeTable,
    nominals: &[&UnitNominalSignature],
    parameters: &BTreeMap<UnitSymbolId, UnitTypeParameterDescriptor>,
) -> StorageRequirements {
    let by_id = nominals
        .iter()
        .map(|nominal| (nominal.declaration(), *nominal))
        .collect::<BTreeMap<_, _>>();
    let mut required = by_id
        .keys()
        .map(|&id| (id, BTreeSet::new()))
        .collect::<StorageRequirements>();
    loop {
        let mut changed = false;
        for nominal in nominals {
            let mut pending = nominal
                .fields()
                .iter()
                .map(|field| field.ty())
                .chain(
                    nominal
                        .enum_cases()
                        .iter()
                        .flat_map(|case| case.payloads().iter().map(|field| field.ty())),
                )
                .chain(nominal.direct_interfaces().iter().copied())
                .chain(nominal.type_parameters().iter().filter_map(|parameter| {
                    match parameters.get(parameter)?.bound() {
                        UnitTypeParameterBound::Interface(bound) => Some(bound),
                        _ => None,
                    }
                }))
                .collect::<Vec<_>>();
            let mut seen = BTreeSet::new();
            let mut found = BTreeSet::new();
            while let Some(ty) = pending.pop() {
                if !seen.insert(ty) {
                    continue;
                }
                match types.get(ty) {
                    Some(UnitTypeKind::Intrinsic {
                        constructor,
                        arguments,
                    }) => {
                        if matches!(
                            constructor,
                            IntrinsicTypeConstructor::Array
                                | IntrinsicTypeConstructor::List
                                | IntrinsicTypeConstructor::MutableList
                                | IntrinsicTypeConstructor::Rc
                        ) && let [argument] = arguments.as_slice()
                        {
                            found.extend(value_parameters(types, *argument));
                        }
                        pending.extend(arguments);
                    }
                    Some(UnitTypeKind::Nominal {
                        declaration,
                        arguments,
                    }) => {
                        if let Some(target) = by_id.get(declaration) {
                            for (parameter, argument) in
                                target.type_parameters().iter().zip(arguments)
                            {
                                if required[declaration].contains(parameter) {
                                    found.extend(value_parameters(types, *argument));
                                }
                            }
                        }
                        pending.extend(arguments);
                    }
                    Some(
                        UnitTypeKind::Nullable(inner)
                        | UnitTypeKind::StaticSelf(inner)
                        | UnitTypeKind::EnumCase { root: inner, .. },
                    ) => pending.push(*inner),
                    Some(UnitTypeKind::Function {
                        parameters,
                        return_type,
                        ..
                    }) => {
                        pending.extend(parameters.iter().map(|parameter| parameter.ty()));
                        pending.push(*return_type);
                    }
                    _ => {}
                }
            }
            let slots = required
                .get_mut(&nominal.declaration())
                .expect("registered nominal");
            for parameter in nominal.type_parameters() {
                if found.contains(parameter) {
                    changed |= slots.insert(*parameter);
                }
            }
        }
        if !changed {
            return required;
        }
    }
}

// Function values and intrinsic handles have their own value representation. Their
// nested storage obligations are visited separately by the declaration walk.
fn value_parameters(types: &UnitTypeTable, root: UnitTypeId) -> BTreeSet<UnitSymbolId> {
    let mut pending = vec![root];
    let mut seen = BTreeSet::new();
    let mut parameters = BTreeSet::new();
    while let Some(ty) = pending.pop() {
        if !seen.insert(ty) {
            continue;
        }
        match types.get(ty) {
            Some(UnitTypeKind::TypeParameter(parameter)) => {
                parameters.insert(*parameter);
            }
            Some(
                UnitTypeKind::Nullable(inner)
                | UnitTypeKind::StaticSelf(inner)
                | UnitTypeKind::EnumCase { root: inner, .. },
            ) => pending.push(*inner),
            Some(UnitTypeKind::Nominal { arguments, .. }) => pending.extend(arguments),
            _ => {}
        }
    }
    parameters
}
