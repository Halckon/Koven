//! Closed value-branch joins; no implicit Any fallback.
use super::*;

impl Checker<'_> {
    pub(super) fn join_control_types(&mut self, left: TypeId, right: TypeId) -> Option<TypeId> {
        if self.is_deferred(left) {
            return Some(left);
        }
        if self.is_deferred(right) {
            return Some(right);
        }
        if left == right {
            return Some(left);
        }
        if self.is_error(left) {
            return Some(right);
        }
        if self.is_error(right) {
            return Some(left);
        }
        if self.is_builtin(left, BuiltinType::Nothing) {
            return Some(right);
        }
        if self.is_builtin(right, BuiltinType::Nothing) {
            return Some(left);
        }
        let left_kind = self.kind(left).clone();
        let right_kind = self.kind(right).clone();
        let left_inner = match left_kind {
            TypeKind::Nullable(inner) => inner,
            _ => left,
        };
        let right_inner = match right_kind {
            TypeKind::Nullable(inner) => inner,
            _ => right,
        };
        if left_inner != left || right_inner != right {
            let inner = self.join_control_types(left_inner, right_inner)?;
            return Some(self.types.intern(TypeKind::Nullable(inner)));
        }
        match (left_kind, right_kind) {
            (TypeKind::EnumCase { root, .. }, _) if root == right => Some(right),
            (_, TypeKind::EnumCase { root, .. }) if left == root => Some(left),
            (
                TypeKind::EnumCase {
                    root: left_root, ..
                },
                TypeKind::EnumCase {
                    root: right_root, ..
                },
            ) if left_root == right_root => Some(left_root),
            _ => None,
        }
    }
}
