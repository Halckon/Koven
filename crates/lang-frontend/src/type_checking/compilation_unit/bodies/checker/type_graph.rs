//! Iterative canonical type queries and substitution shared by body/layout consumers.

use super::*;
use crate::type_checking::UnitTypeTable;

#[derive(Default)]
pub(super) struct ConcreteTypes {
    completed: BTreeMap<UnitTypeId, bool>,
    include_refinements: bool,
    #[cfg(test)]
    pub(super) visits: usize,
}

impl ConcreteTypes {
    /// Closed source arguments may retain an enum-case refinement without a runtime field recipe.
    pub(super) fn for_closed_arguments() -> Self {
        Self {
            include_refinements: true,
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
            ) || (self.include_refinements
                && matches!(kind, UnitTypeKind::EnumCase { .. }));
            let children = if self.include_refinements
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
        let kind = match kind {
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
        };
        completed.insert(ty, types.intern(kind));
        active.remove(&ty);
    }
    completed
        .get(&root)
        .copied()
        .ok_or(CompilationUnitTypeError::MissingDeclarationSymbol)
}

fn children(kind: &UnitTypeKind) -> Vec<UnitTypeId> {
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
