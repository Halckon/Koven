//! Separately sealed sequential closed-loop catalog; not a runtime iteration trace.
use super::*;
/// Ordered independent closed-loop products from one callable analysis.
#[derive(Clone, Debug)]
pub struct UnitOwnerClosedLoopBatchV1 {
    products: Vec<UnitOwnerLoopObservationV1>,
}
impl UnitOwnerClosedLoopBatchV1 {
    /// Products in original AST statement order.
    pub fn products(&self) -> &[UnitOwnerLoopObservationV1] {
        &self.products
    }
}
impl UnitOwnerObservation {
    /// Separate closed sequence capability; never a single-loop or straight-line projection.
    pub fn closed_loop_batch_v1(&self) -> Option<&UnitOwnerClosedLoopBatchV1> {
        None
    }
}
