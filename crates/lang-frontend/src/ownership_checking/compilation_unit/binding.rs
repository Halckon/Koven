//! Compilation-unit callable 参数的 Phase 3 binding 能力。

use crate::{name_resolution::UnitSymbolId, source::Span};

use super::OwnershipBindingKind;

/// compilation-unit callable 参数在 Phase 3 中提供的能力。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitOwnershipBindingDescriptor {
    symbol: UnitSymbolId,
    kind: OwnershipBindingKind,
    declaration_span: Span,
}

impl UnitOwnershipBindingDescriptor {
    pub(super) const fn new(
        symbol: UnitSymbolId,
        kind: OwnershipBindingKind,
        declaration_span: Span,
    ) -> Self {
        Self {
            symbol,
            kind,
            declaration_span,
        }
    }

    /// 返回 source-qualified 参数 symbol。
    #[must_use]
    pub const fn symbol(self) -> UnitSymbolId {
        self.symbol
    }

    /// 返回 owned/shared/exclusive 能力。
    #[must_use]
    pub const fn kind(self) -> OwnershipBindingKind {
        self.kind
    }

    /// 返回参数声明范围；后续跨文件诊断可直接引用该位置。
    #[must_use]
    pub const fn declaration_span(self) -> Span {
        self.declaration_span
    }
}
