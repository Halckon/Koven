//! ADR-0033 inline nullable identities for non-pointer `T?`.
use super::model::{ModelError, Module, SsaTypeId, SsaTypeKind};

impl Module {
    /// Interns `T?` for an inline inner type. Pointer-like owners keep `NullableHandle`;
    /// String and sequential containers use the null data-pointer niche.
    pub(crate) fn add_inline_nullable_type(
        &mut self,
        inner: SsaTypeId,
    ) -> Result<SsaTypeId, ModelError> {
        self.check_type_id(inner)?;
        let niche = self
            .type_is_defined(inner)
            .then(|| inline_nullable_niche(self, inner))
            .flatten()
            .ok_or(ModelError::ExpectedInlineNullableInner { ty: inner })?;
        Ok(self.intern_type(SsaTypeKind::InlineNullable { inner, niche }))
    }

    /// Returns `(inner, niche)` for an ADR-0033 inline nullable type.
    pub(crate) fn inline_nullable(&self, ty: SsaTypeId) -> Option<(SsaTypeId, bool)> {
        match self.type_kind(ty)? {
            SsaTypeKind::InlineNullable { inner, niche } => Some((*inner, *niche)),
            _ => None,
        }
    }
}

/// `Some(niche)` when `inner` may carry an inline nullable wrapper, `None` otherwise.
pub(super) fn inline_nullable_niche(module: &Module, inner: SsaTypeId) -> Option<bool> {
    match module.type_kind(inner)? {
        SsaTypeKind::Boolean | SsaTypeKind::Char | SsaTypeKind::Integer { .. } => Some(false),
        SsaTypeKind::Aggregate { .. } if module.presence_payload_type(inner).is_none() => {
            Some(false)
        }
        SsaTypeKind::TaggedUnion { .. } => Some(false),
        SsaTypeKind::StringOwner | SsaTypeKind::SequentialContainer { .. } => Some(true),
        _ => None,
    }
}
