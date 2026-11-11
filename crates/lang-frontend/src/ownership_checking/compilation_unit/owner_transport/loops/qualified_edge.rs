//! Root-qualified control endpoints; not a multi-root join or drop authority.
use super::super::*;
use super::UnitOwnerLoopEdgeV1;
use crate::type_checking::UnitStatementId;

/// One structural edge bound to its original analysis, callable, control and root.
/// Equal-looking AST IDs from a new analysis do not identify the same edge.
#[derive(Clone, Debug)]
pub struct UnitOwnerRootEdgeV1 {
    identity: Arc<()>,
    callable: UnitOwnerCallable,
    control: UnitStatementId,
    root: UnitSymbolId,
    edge: UnitOwnerLoopEdgeV1,
}
impl PartialEq for UnitOwnerRootEdgeV1 {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.identity, &other.identity)
            && self.callable == other.callable
            && self.control == other.control
            && self.root == other.root
            && self.edge == other.edge
    }
}
impl Eq for UnitOwnerRootEdgeV1 {}
impl UnitOwnerRootEdgeV1 {
    /// Source-qualified owning callable.
    pub fn callable(&self) -> UnitOwnerCallable {
        self.callable
    }
    /// Actual loop statement, including its source identity.
    pub fn control(&self) -> UnitStatementId {
        self.control
    }
    /// Actual owning binding, never a symbol name or producer index.
    pub fn root(&self) -> UnitSymbolId {
        self.root
    }
    /// The original structural role and endpoints.
    pub fn edge(&self) -> UnitOwnerLoopEdgeV1 {
        self.edge
    }
}
impl super::UnitOwnerNormalBackedgeV1 {
    /// Qualified edges of the sealed single-root normal-header product.
    /// Does not authorize arbitrary nested loops, extra roots or consumer cleanup.
    pub fn qualified_edges(&self) -> Option<[UnitOwnerRootEdgeV1; 3]> {
        None
    }
}
