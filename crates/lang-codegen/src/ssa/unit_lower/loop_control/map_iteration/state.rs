//! Copy path state and separately owned cumulative evidence; history is never a checkpoint.
use super::*;
pub(in super::super) struct MapRuntime {
    pub(in super::super) path: MapState,
    pub(super) evidence: conditional::Evidence,
}
impl std::ops::Deref for MapRuntime {
    type Target = MapState;
    fn deref(&self) -> &Self::Target {
        &self.path
    }
}
impl std::ops::DerefMut for MapRuntime {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.path
    }
}
