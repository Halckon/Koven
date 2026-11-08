//! A retry can qualify sequential iteration with while, or a while header
//! that invalidates a proof, only with a validated local destructuring.
use super::{RefineInputs, accepts_with_loop_route};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LoopRoute {
    Existing,
    Candidate,
    Verified,
}

pub(in crate::ssa::refine) fn accepts(inputs: &RefineInputs<'_>) -> bool {
    let (accepted, retry) = accepts_with_loop_route(inputs, LoopRoute::Existing);
    accepted
        || retry
            && has_destructuring_candidate(inputs)
            && accepts_with_loop_route(inputs, LoopRoute::Candidate).0
}

// Span membership only avoids a retry on unrelated files. The retry itself must
// see the descriptor through this callable's normal reachable-body traversal.
fn has_destructuring_candidate(inputs: &RefineInputs<'_>) -> bool {
    let Some(scope) = inputs.scope_span() else {
        return false;
    };
    inputs.typed.destructurings().iter().any(|descriptor| {
        let statement = descriptor.statement();
        descriptor.is_reachable()
            && statement.source_unit() == inputs.source_unit
            && inputs
                .parsed
                .ast()
                .statements()
                .get(statement.statement())
                .is_ok_and(|node| {
                    let span = node.span();
                    span.source_id() == scope.source_id()
                        && scope.start() <= span.start()
                        && span.end() <= scope.end()
                })
    })
}
