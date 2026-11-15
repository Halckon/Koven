//! Read-only root initializer and final-use accessors.
use super::*;
impl EntryOwnerDescriptor {
    /// Original resolved named root.
    pub fn root(&self) -> UnitSymbolId {
        self.root
    }
    /// Original initializer expression.
    pub fn initializer(&self) -> UnitExpressionId {
        self.initializer
    }
    /// Original initializer span.
    pub fn span(&self) -> Span {
        self.span
    }
    /// Original decoded String bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Original final-use Drop, not a synthetic event.
    pub fn drop_fact(&self) -> &UnitDropFact {
        &self.drop
    }
}
