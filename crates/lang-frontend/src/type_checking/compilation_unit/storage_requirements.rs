//! Finite nominal-parameter obligations for sequential elements and Rc payloads.
use std::collections::{BTreeMap, BTreeSet};

use super::{
    UnitNominalSignature, UnitTypeId, UnitTypeKind, UnitTypeParameterBound,
    UnitTypeParameterDescriptor, UnitTypeTable,
};
use crate::{
    name_resolution::{DeclarationId, UnitSymbolId},
    type_checking::{IntrinsicTypeConstructor, NominalKind},
};

type LayoutParameters = BTreeMap<DeclarationId, BTreeSet<usize>>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct StorageRequirements {
    nested: BTreeMap<DeclarationId, BTreeSet<UnitSymbolId>>,
    class_layout: LayoutParameters,
}

impl StorageRequirements {
    pub(super) fn required(&self, declaration: DeclarationId) -> Option<&BTreeSet<UnitSymbolId>> {
        self.nested.get(&declaration)
    }

    pub(super) fn representation_arguments<'a>(
        &'a self,
        declaration: DeclarationId,
        arguments: &'a [UnitTypeId],
    ) -> impl Iterator<Item = UnitTypeId> + 'a {
        representation_arguments(&self.class_layout, declaration, arguments)
    }
}

fn representation_arguments<'a>(
    layouts: &'a LayoutParameters,
    declaration: DeclarationId,
    arguments: &'a [UnitTypeId],
) -> impl Iterator<Item = UnitTypeId> + 'a {
    let slots = layouts.get(&declaration);
    arguments
        .iter()
        .copied()
        .enumerate()
        .filter_map(move |(index, argument)| {
            slots
                .is_none_or(|slots| slots.contains(&index))
                .then_some(argument)
        })
}

fn layout_components(nominal: &UnitNominalSignature) -> impl Iterator<Item = UnitTypeId> + '_ {
    nominal.fields().iter().map(|field| field.ty()).chain(
        nominal
            .enum_cases()
            .iter()
            .flat_map(|case| case.payloads().iter().map(|field| field.ty())),
    )
}

// Compute parameter dependencies of the class instance, including owned payload
// layouts. Track users of each slot set so a change only revisits dependents.
fn class_layout_parameters(
    types: &UnitTypeTable,
    nominals: &BTreeMap<DeclarationId, &UnitNominalSignature>,
) -> LayoutParameters {
    let mut layouts = nominals
        .iter()
        .filter(|(_, nominal)| !nominal.type_parameters().is_empty())
        .map(|(&declaration, _)| (declaration, BTreeSet::new()))
        .collect::<LayoutParameters>();
    let mut pending = layouts.keys().copied().collect::<BTreeSet<_>>();
    let mut dependents = BTreeMap::<DeclarationId, BTreeSet<DeclarationId>>::new();
    while let Some(declaration) = pending.pop_first() {
        let nominal = nominals[&declaration];
        let found = value_parameters(
            types,
            layout_components(nominal),
            |kind, children| match kind {
                UnitTypeKind::Nominal {
                    declaration: target,
                    arguments,
                } => {
                    if layouts.contains_key(target) {
                        dependents.entry(*target).or_default().insert(declaration);
                    }
                    children.extend(representation_arguments(&layouts, *target, arguments));
                }
                UnitTypeKind::Intrinsic {
                    constructor,
                    arguments,
                } if *constructor != IntrinsicTypeConstructor::View => {
                    children.extend(arguments);
                }
                _ => {}
            },
        );
        let slots = layouts.entry(declaration).or_default();
        let previous_len = slots.len();
        slots.extend(
            nominal
                .type_parameters()
                .iter()
                .enumerate()
                .filter_map(|(index, parameter)| found.contains(parameter).then_some(index)),
        );
        if slots.len() != previous_len {
            pending.extend(dependents.get(&declaration).into_iter().flatten().copied());
        }
    }
    // Only ordinary classes use these slots at a storage use site. Value and
    // enum types contribute to the class calculation through their payloads.
    layouts.retain(|declaration, _| nominals[declaration].kind() == NominalKind::Class);
    layouts
}

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
    let class_layout = class_layout_parameters(types, &by_id);
    // F11 projects an already stored type. Its nested intrinsic arguments
    // are validated separately by the declaration walk below.
    let nominal_children = |kind: &UnitTypeKind, children: &mut Vec<UnitTypeId>| {
        if let UnitTypeKind::Nominal {
            declaration,
            arguments,
        } = kind
        {
            children.extend(representation_arguments(
                &class_layout,
                *declaration,
                arguments,
            ));
        }
    };
    let mut required = by_id
        .keys()
        .map(|&id| (id, BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();
    loop {
        let mut changed = false;
        for nominal in nominals {
            if nominal.type_parameters().is_empty() {
                continue;
            }
            let mut pending = layout_components(nominal)
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
                            found.extend(value_parameters(types, [*argument], nominal_children));
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
                                    found.extend(value_parameters(
                                        types,
                                        [*argument],
                                        nominal_children,
                                    ));
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
            return StorageRequirements {
                nested: required,
                class_layout,
            };
        }
    }
}

fn value_parameters(
    types: &UnitTypeTable,
    roots: impl IntoIterator<Item = UnitTypeId>,
    mut extend_children: impl FnMut(&UnitTypeKind, &mut Vec<UnitTypeId>),
) -> BTreeSet<UnitSymbolId> {
    let mut pending = roots.into_iter().collect::<Vec<_>>();
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
            Some(kind) => extend_children(kind, &mut pending),
            None => {}
        }
    }
    parameters
}
