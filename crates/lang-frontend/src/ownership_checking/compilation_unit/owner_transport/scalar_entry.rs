//! Preserved owner vector through one observed resource-free scalar prefix.
use super::*;
use crate::type_checking::UnitStatementId;
/// Actual completed entry split/join and subsequent parent-loop entry.
#[derive(Clone, Debug)]
pub struct UnitOwnerScalarEntryV1 {
    pub(in crate::ownership_checking::compilation_unit) prefix: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) condition: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) calls: [UnitExpressionId; 2],
    pub(in crate::ownership_checking::compilation_unit) parent: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) owners: Vec<(UnitSymbolId, UnitOwnerId)>,
}
impl UnitOwnerScalarEntryV1 {
    /// The actual entry If expression.
    pub fn prefix(&self) -> UnitExpressionId {
        self.prefix
    }
    /// Its unchanged Boolean parameter condition.
    pub fn condition(&self) -> UnitExpressionId {
        self.condition
    }
    /// Completed empty Int-to-Unit calls, in then/else order.
    pub fn calls(&self) -> [UnitExpressionId; 2] {
        self.calls
    }
    /// The actual immediately following parent While.
    pub fn parent(&self) -> UnitStatementId {
        self.parent
    }
    /// Complete ordered owner vector unchanged at every observed boundary.
    pub fn owners(&self) -> &[(UnitSymbolId, UnitOwnerId)] {
        &self.owners
    }
    /// Source and opaque owner identity consistency; not an SSA edge proof.
    pub fn is_consistent(&self) -> bool {
        let source = self.parent.source_unit();
        self.prefix.source_unit() == source
            && self.condition.source_unit() == source
            && self.calls.iter().all(|c| c.source_unit() == source)
            && self.calls[0] != self.calls[1]
            && self.owners.len() >= 2
            && self.owners.iter().enumerate().all(|(i, (root, owner))| {
                root.source_unit() == source
                    && self.owners[..i]
                        .iter()
                        .all(|(r, o)| r != root && o != owner)
                    && self.owners.iter().all(|(_, o)| {
                        o.callable == owner.callable && Arc::ptr_eq(&o.identity, &owner.identity)
                    })
            })
    }
}
