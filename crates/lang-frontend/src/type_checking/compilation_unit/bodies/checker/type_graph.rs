//! Iterative canonical type queries and substitution shared by body/layout consumers.

use super::*;
use crate::type_checking::UnitTypeTable;

#[derive(Default)]
pub(super) struct ConcreteTypes {
    completed: BTreeMap<UnitTypeId, bool>,
    closed_arguments: bool,
    #[cfg(test)]
    pub(super) visits: usize,
}

impl ConcreteTypes {
    /// Closed arguments may retain enum refinements or known Any without a field recipe.
    pub(super) fn for_closed_arguments() -> Self {
        Self {
            closed_arguments: true,
            ..Self::default()
        }
    }

    /// Cache completed DAG nodes across owners; instance limits do not bound repeated paths.
    pub(super) fn is_concrete(
        &mut self,
        types: &UnitTypeTable,
        root: UnitTypeId,
    ) -> Result<bool, CompilationUnitTypeError> {
        let mut pending = vec![(root, false)];
        let mut active = BTreeSet::new();
        while let Some((ty, finish)) = pending.pop() {
            if self.completed.contains_key(&ty) {
                continue;
            }
            let kind = types
                .get(ty)
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
            let supported = matches!(
                kind,
                UnitTypeKind::Builtin(_)
                    | UnitTypeKind::Nullable(_)
                    | UnitTypeKind::Function { .. }
                    | UnitTypeKind::Nominal { .. }
                    | UnitTypeKind::Intrinsic { .. }
            ) || (self.closed_arguments
                && matches!(
                    kind,
                    UnitTypeKind::EnumCase { .. }
                        | UnitTypeKind::Deferred(DeferredReason::AnyValueRepresentation)
                ));
            let children = if self.closed_arguments
                && let UnitTypeKind::EnumCase { root, .. } = kind
            {
                vec![*root]
            } else if supported {
                children(kind)
            } else {
                Vec::new()
            };
            if finish {
                let mut concrete = supported;
                for child in children {
                    concrete &= self
                        .completed
                        .get(&child)
                        .copied()
                        .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
                }
                self.completed.insert(ty, concrete);
                active.remove(&ty);
            } else {
                if !active.insert(ty) {
                    return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
                }
                #[cfg(test)]
                {
                    self.visits += 1;
                }
                pending.push((ty, true));
                pending.extend(children.into_iter().rev().map(|child| (child, false)));
            }
        }
        self.completed
            .get(&root)
            .copied()
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)
    }
}

/// One memo belongs to one substitution environment; actual arguments remain leaf identities.
pub(super) fn substitute(
    types: &mut UnitTypeTable,
    root: UnitTypeId,
    substitutions: &BTreeMap<UnitSymbolId, UnitTypeId>,
    completed: &mut BTreeMap<UnitTypeId, UnitTypeId>,
) -> Result<UnitTypeId, CompilationUnitTypeError> {
    let mut pending = vec![(root, false)];
    let mut active = BTreeSet::new();
    while let Some((ty, finish)) = pending.pop() {
        if completed.contains_key(&ty) {
            continue;
        }
        let kind = types.get(ty).cloned().unwrap_or(UnitTypeKind::Error);
        if let UnitTypeKind::TypeParameter(symbol) = kind {
            completed.insert(ty, substitutions.get(&symbol).copied().unwrap_or(ty));
            continue;
        }
        if !finish {
            if !active.insert(ty) {
                return Err(CompilationUnitTypeError::MissingDeclarationSymbol);
            }
            pending.push((ty, true));
            pending.extend(
                children(&kind)
                    .into_iter()
                    .rev()
                    .map(|child| (child, false)),
            );
            continue;
        }
        let mapped = |id: UnitTypeId| {
            completed
                .get(&id)
                .copied()
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)
        };
        let kind = map_type_children(kind, mapped)?;
        completed.insert(ty, types.intern(kind));
        active.remove(&ty);
    }
    completed
        .get(&root)
        .copied()
        .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)
}

pub(super) fn map_type_children(
    kind: UnitTypeKind,
    mut mapped: impl FnMut(UnitTypeId) -> Result<UnitTypeId, CompilationUnitTypeError>,
) -> Result<UnitTypeKind, CompilationUnitTypeError> {
    Ok(match kind {
        UnitTypeKind::Nullable(inner) => UnitTypeKind::Nullable(mapped(inner)?),
        UnitTypeKind::Function {
            move_only,
            parameters,
            return_type,
        } => UnitTypeKind::Function {
            move_only,
            parameters: parameters
                .into_iter()
                .map(|p| Ok(UnitFunctionParameterType::new(p.mode(), mapped(p.ty())?)))
                .collect::<Result<_, CompilationUnitTypeError>>()?,
            return_type: mapped(return_type)?,
        },
        UnitTypeKind::Nominal {
            declaration,
            arguments,
        } => UnitTypeKind::Nominal {
            declaration,
            arguments: arguments
                .into_iter()
                .map(mapped)
                .collect::<Result<_, _>>()?,
        },
        UnitTypeKind::Intrinsic {
            constructor,
            arguments,
        } => UnitTypeKind::Intrinsic {
            constructor,
            arguments: arguments
                .into_iter()
                .map(mapped)
                .collect::<Result<_, _>>()?,
        },
        UnitTypeKind::StaticSelf(inner) => UnitTypeKind::StaticSelf(mapped(inner)?),
        other => other,
    })
}

pub(super) fn children(kind: &UnitTypeKind) -> Vec<UnitTypeId> {
    match kind {
        UnitTypeKind::Nullable(inner) | UnitTypeKind::StaticSelf(inner) => vec![*inner],
        UnitTypeKind::Function {
            parameters,
            return_type,
            ..
        } => parameters
            .iter()
            .map(|p| p.ty())
            .chain(std::iter::once(*return_type))
            .collect(),
        UnitTypeKind::Nominal { arguments, .. } | UnitTypeKind::Intrinsic { arguments, .. } => {
            arguments.clone()
        }
        _ => Vec::new(),
    }
}

/// Types created by instantiation before argument checking. These IDs are private:
/// argument checks may reuse their numeric indices for unrelated canonical types.
pub(super) struct CandidateTypes {
    checkpoint: usize,
    kinds: Vec<UnitTypeKind>,
}

impl CandidateTypes {
    pub(super) fn detach(types: &mut UnitTypeTable, checkpoint: usize) -> Option<Self> {
        (types.len() > checkpoint).then(|| Self {
            checkpoint,
            kinds: types.take_after(checkpoint),
        })
    }

    pub(super) fn kind<'a>(
        &'a self,
        types: &'a UnitTypeTable,
        ty: UnitTypeId,
    ) -> Option<&'a UnitTypeKind> {
        match ty.index().checked_sub(self.checkpoint) {
            Some(index) => self.kinds.get(index),
            None => types.get(ty),
        }
    }

    pub(super) fn materialize_expected(
        &self,
        types: &mut UnitTypeTable,
        root: UnitTypeId,
    ) -> Result<UnitTypeId, CompilationUnitTypeError> {
        let Some(root_index) = root.index().checked_sub(self.checkpoint) else {
            return Ok(root);
        };
        // Dependencies were interned before their parents. Collect only this
        // expected type's subgraph and replay it in the original order.
        let mut pending = vec![root_index];
        let mut needed = BTreeSet::new();
        while let Some(index) = pending.pop() {
            if !needed.insert(index) {
                continue;
            }
            let kind = self
                .kinds
                .get(index)
                .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
            let operands = match kind {
                UnitTypeKind::EnumCase { root, .. } => vec![*root],
                _ => children(kind),
            };
            pending.extend(
                operands
                    .into_iter()
                    .filter_map(|child| child.index().checked_sub(self.checkpoint)),
            );
        }
        let mut replayed = BTreeMap::new();
        for index in needed {
            let kind = self.remap(self.kinds[index].clone(), |index| {
                replayed.get(&index).copied()
            })?;
            replayed.insert(index, types.intern(kind));
        }
        replayed
            .get(&root_index)
            .copied()
            .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)
    }

    pub(super) fn publish<'a>(
        &self,
        types: &mut UnitTypeTable,
        roots: impl Iterator<Item = &'a mut UnitTypeId>,
    ) -> Result<(), CompilationUnitTypeError> {
        let mut replayed = Vec::with_capacity(self.kinds.len());
        for kind in &self.kinds {
            let kind = self.remap(kind.clone(), |index| replayed.get(index).copied())?;
            replayed.push(types.intern(kind));
        }
        for root in roots {
            if let Some(index) = root.index().checked_sub(self.checkpoint) {
                *root = replayed
                    .get(index)
                    .copied()
                    .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)?;
            }
        }
        Ok(())
    }

    fn remap(
        &self,
        kind: UnitTypeKind,
        lookup: impl Fn(usize) -> Option<UnitTypeId>,
    ) -> Result<UnitTypeKind, CompilationUnitTypeError> {
        let mapped = |id: UnitTypeId| match id.index().checked_sub(self.checkpoint) {
            Some(index) => lookup(index).ok_or(CompilationUnitTypeError::MissingDeclarationSymbol),
            None => Ok(id),
        };
        if let UnitTypeKind::EnumCase { case, root } = kind {
            return Ok(UnitTypeKind::EnumCase {
                case,
                root: mapped(root)?,
            });
        }
        map_type_children(kind, mapped)
    }
}
