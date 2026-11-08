//! Partial comparison for call candidates with unresolved type inputs.

use std::collections::BTreeSet;

use crate::type_checking::{BuiltinType, DeferredReason, UnitTypeId, UnitTypeKind, UnitTypeTable};

use super::{BodyChecker, type_graph::children};

fn is_unresolved(kind: Option<&UnitTypeKind>) -> bool {
    matches!(
        kind,
        Some(UnitTypeKind::Deferred(reason)) if *reason != DeferredReason::AnyValueRepresentation
    )
}

pub(super) fn contains_unresolved(
    types: &UnitTypeTable,
    root: UnitTypeId,
    visited: &mut BTreeSet<UnitTypeId>,
) -> bool {
    if visited.contains(&root) {
        return false;
    }
    let kind = types.get(root);
    if is_unresolved(kind) {
        return true;
    }
    let mut pending = match kind {
        Some(UnitTypeKind::EnumCase { root, .. }) => vec![*root],
        Some(kind) => children(kind),
        None => return false,
    };
    if pending.is_empty() {
        return false;
    }
    visited.insert(root);
    while let Some(ty) = pending.pop() {
        if !visited.insert(ty) {
            continue;
        }
        let kind = types.get(ty);
        if is_unresolved(kind) {
            return true;
        }
        match kind {
            Some(UnitTypeKind::EnumCase { root, .. }) => pending.push(*root),
            Some(kind) => pending.extend(children(kind)),
            None => {}
        }
    }
    false
}

impl BodyChecker<'_> {
    pub(super) fn is_unresolved_call_type(&self, ty: UnitTypeId) -> bool {
        is_unresolved(self.signatures.types().get(ty))
    }

    pub(super) fn contains_unresolved_call_type(&self, root: UnitTypeId) -> bool {
        contains_unresolved(self.signatures.types(), root, &mut BTreeSet::new())
    }

    pub(super) fn call_argument_could_match(
        &self,
        actual: UnitTypeId,
        expected: UnitTypeId,
        pending: bool,
    ) -> bool {
        if let Some(compatible) = self.contextual_integer_could_match(actual, expected, pending) {
            return compatible;
        }
        if self.assignable(actual, expected) {
            return true;
        }
        if !pending {
            return false;
        }
        let types = self.signatures.types();
        let root = match types.get(actual) {
            Some(UnitTypeKind::EnumCase { root, .. } | UnitTypeKind::StaticSelf(root)) => *root,
            _ => actual,
        };
        self.types_could_be_equal(actual, expected)
            || (root != actual && self.types_could_be_equal(root, expected))
            || matches!(
                types.get(expected),
                Some(UnitTypeKind::Nullable(inner))
                    if self.types_could_be_equal(actual, *inner)
                        || matches!(types.get(actual), Some(UnitTypeKind::Nullable(actual_inner))
                            if self.is_builtin(*actual_inner, BuiltinType::Nothing))
                        || matches!(types.get(actual), Some(UnitTypeKind::EnumCase { .. }))
                            && self.types_could_be_equal(root, *inner)
            )
    }

    /// Compare invariant structure, treating only genuinely unresolved nodes
    /// as holes. Known heads, arity and callable delivery modes still constrain it.
    pub(super) fn types_could_be_equal(&self, left: UnitTypeId, right: UnitTypeId) -> bool {
        let types = self.signatures.types();
        let mut pending = vec![(left, right)];
        let mut visited = BTreeSet::new();
        while let Some((left, right)) = pending.pop() {
            if left == right
                || !visited.insert((left, right))
                || self.is_unresolved_call_type(left)
                || self.is_unresolved_call_type(right)
            {
                continue;
            }
            let Some(children) = invariant_type_children(types.get(left), types.get(right)) else {
                return false;
            };
            pending.extend(children);
        }
        true
    }
}

/// Children of matching known heads, in the canonical kind's operand order.
pub(super) fn invariant_type_children(
    left: Option<&UnitTypeKind>,
    right: Option<&UnitTypeKind>,
) -> Option<Vec<(UnitTypeId, UnitTypeId)>> {
    let mut pending = Vec::new();
    match (left, right) {
        (Some(UnitTypeKind::Nullable(left)), Some(UnitTypeKind::Nullable(right)))
        | (Some(UnitTypeKind::StaticSelf(left)), Some(UnitTypeKind::StaticSelf(right))) => {
            pending.push((*left, *right));
        }
        (
            Some(UnitTypeKind::Nominal {
                declaration: left,
                arguments: largs,
            }),
            Some(UnitTypeKind::Nominal {
                declaration: right,
                arguments: rargs,
            }),
        ) if left == right && largs.len() == rargs.len() => {
            pending.extend(largs.iter().copied().zip(rargs.iter().copied()));
        }
        (
            Some(UnitTypeKind::Intrinsic {
                constructor: left,
                arguments: largs,
            }),
            Some(UnitTypeKind::Intrinsic {
                constructor: right,
                arguments: rargs,
            }),
        ) if left == right && largs.len() == rargs.len() => {
            pending.extend(largs.iter().copied().zip(rargs.iter().copied()));
        }
        (
            Some(UnitTypeKind::Function {
                move_only: left_move,
                parameters: left,
                return_type: left_ret,
            }),
            Some(UnitTypeKind::Function {
                move_only: right_move,
                parameters: right,
                return_type: right_ret,
            }),
        ) if left_move == right_move
            && left.len() == right.len()
            && left
                .iter()
                .zip(right)
                .all(|(left, right)| left.mode() == right.mode()) =>
        {
            pending.extend(
                left.iter()
                    .zip(right)
                    .map(|(left, right)| (left.ty(), right.ty())),
            );
            pending.push((*left_ret, *right_ret));
        }
        (
            Some(UnitTypeKind::EnumCase {
                case: left,
                root: left_root,
            }),
            Some(UnitTypeKind::EnumCase {
                case: right,
                root: right_root,
            }),
        ) if left == right => pending.push((*left_root, *right_root)),
        _ => return None,
    }
    Some(pending)
}
