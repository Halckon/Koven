//! 顺序容器元素的可存储性：包括 N1a 非逃逸 carrier 的推断拒绝。
use super::*;

impl Checker<'_> {
    pub(in crate::type_checking::checker) fn validate_construction_element(
        &mut self,
        element: TypeId,
        span: Span,
    ) -> Result<bool, TypeCheckingError> {
        if self.is_error(element) || self.is_deferred(element) {
            return Ok(false);
        }
        if crate::type_checking::range_type_uses::file_contains_range(&self.types, element) {
            self.emit(
                codes::catalog()?.resolve(codes::BORROW_RESULT_ESCAPE)?,
                "range carrier cannot be a container element",
                span,
            )?;
            return Ok(false);
        }
        if self.is_structurally_storable_type(element) {
            return Ok(true);
        }
        self.emit(
            self.invalid_container_element_code,
            "sequential container element type is not structurally storable",
            span,
        )?;
        Ok(false)
    }

    pub(in crate::type_checking::checker) fn is_structurally_storable_type(
        &self,
        ty: TypeId,
    ) -> bool {
        if crate::type_checking::range_type_uses::file_contains_range(&self.types, ty) {
            return false;
        }
        match self.kind(ty) {
            TypeKind::Builtin(BuiltinType::Any | BuiltinType::Nothing) => false,
            TypeKind::Builtin(_) => true,
            TypeKind::Nullable(inner) => self.is_structurally_storable_type(*inner),
            TypeKind::Function { .. } => true,
            TypeKind::Nominal { nominal, arguments } => {
                self.nominals
                    .iter()
                    .find(|descriptor| descriptor.id() == *nominal)
                    .is_some_and(|descriptor| descriptor.kind() != NominalKind::Interface)
                    && !self.invalid_inline_nominals.contains(nominal)
                    && arguments
                        .iter()
                        .all(|argument| self.is_structurally_storable_type(*argument))
            }
            TypeKind::Intrinsic { .. } | TypeKind::TypeParameter(_) => true,
            TypeKind::EnumCase { root, .. } | TypeKind::StaticSelf(root) => {
                self.is_structurally_storable_type(*root)
            }
            TypeKind::Capability(_)
            | TypeKind::IntegerLiteral(_)
            | TypeKind::Error
            | TypeKind::Deferred(_) => false,
        }
    }
}
