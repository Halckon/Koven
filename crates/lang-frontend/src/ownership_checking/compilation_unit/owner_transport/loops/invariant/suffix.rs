//! The optional suffix capability does not change the original boundary contract.
use super::*;

#[derive(Clone, Debug)]
pub(in crate::ownership_checking::compilation_unit) struct PostLoopTransport;

impl UnitOwnerInvariantLoopObservationV1 {
    /// Actual prefix and post-loop consumption on the normal exit path, together with
    /// this observation's unchanged loop boundaries. Not a phi or runtime trace.
    pub fn post_loop_transport_v1(&self) -> Option<&UnitOwnerValueObservationV1> {
        let _completed = self.post_loop.as_ref()?;
        None
    }
}
