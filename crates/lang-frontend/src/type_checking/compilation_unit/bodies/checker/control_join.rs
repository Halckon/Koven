//! Closed value-branch joins; no implicit Any fallback.
use super::{BodyChecker, BuiltinType, CompilationUnitTypeError, UnitTypeId, UnitTypeKind};

impl BodyChecker<'_> {
    pub(super) fn control_branch_expected(
        &mut self,
        source: crate::name_resolution::SourceUnitId,
        body: crate::ast::StatementId,
        expected: Option<UnitTypeId>,
    ) -> Result<Option<UnitTypeId>, CompilationUnitTypeError> {
        if expected.is_some()
            || !crate::type_checking::control_body::ends_in_null(self.file(source), body)?
        {
            return Ok(expected);
        }
        let nothing = self.builtin(BuiltinType::Nothing);
        Ok(Some(
            self.signatures
                .types_mut()
                .intern(UnitTypeKind::Nullable(nothing)),
        ))
    }

    pub(super) fn join_control_types(
        &mut self,
        left: UnitTypeId,
        right: UnitTypeId,
    ) -> Option<UnitTypeId> {
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
        let left_kind = self.signatures.types().get(left)?.clone();
        let right_kind = self.signatures.types().get(right)?.clone();
        let left_inner = match left_kind {
            UnitTypeKind::Nullable(inner) => inner,
            _ => left,
        };
        let right_inner = match right_kind {
            UnitTypeKind::Nullable(inner) => inner,
            _ => right,
        };
        if left_inner != left || right_inner != right {
            let inner = self.join_control_types(left_inner, right_inner)?;
            return Some(
                self.signatures
                    .types_mut()
                    .intern(UnitTypeKind::Nullable(inner)),
            );
        }
        match (left_kind, right_kind) {
            (UnitTypeKind::EnumCase { root, .. }, _) if root == right => Some(right),
            (_, UnitTypeKind::EnumCase { root, .. }) if left == root => Some(left),
            (
                UnitTypeKind::EnumCase {
                    root: left_root, ..
                },
                UnitTypeKind::EnumCase {
                    root: right_root, ..
                },
            ) if left_root == right_root => Some(left_root),
            _ => None,
        }
    }
}
