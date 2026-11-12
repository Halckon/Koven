//! Core resource projection: only empty capture-result cache entries are normalized.
use super::{Checker, State};
impl Checker<'_> {
    pub(super) fn resource_snapshot(&self, state: &State) -> State {
        let mut snapshot = state.clone();
        // Empty closure-result caches contain no captured environment obligations.
        snapshot
            .closure_results
            .retain(|_, origins| !origins.is_empty());
        snapshot
    }
    pub(super) fn simple_resources(&self, snapshot: &State) -> bool {
        let mut empty_origins = Default::default();
        // This only constructs the comparison anchor; it does not alter actual state.
        crate::ownership_checking::callable_provenance::graph::OriginState::attach(
            &mut empty_origins,
            &self.callable_sources.arena,
        );
        snapshot.origins == empty_origins
            && snapshot.closure_results.is_empty()
            && snapshot.replacements.is_empty()
            && snapshot.closures.is_empty()
            && snapshot.closure_captures.is_empty()
            && snapshot.immutable_captures.is_empty()
            && snapshot.pending_closure_calls.is_empty()
    }
}
