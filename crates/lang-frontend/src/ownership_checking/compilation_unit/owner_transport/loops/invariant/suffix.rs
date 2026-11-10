//! Real prefix/suffix consumption is sealed separately from unchanged loop boundaries.
use super::*;
use crate::ownership_checking::{
    UnitDropPoint, UnitOwnerValueEventV1 as Event, UnitOwnerValueV1 as Value,
};

#[derive(Clone, Debug)]
pub(in crate::ownership_checking::compilation_unit) struct PostLoopTransport {
    pub(in crate::ownership_checking::compilation_unit) statement: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) expression: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) retired: UnitSymbolId,
    pub(in crate::ownership_checking::compilation_unit) entry: UnitOwnerLoopBindingV1,
    pub(in crate::ownership_checking::compilation_unit) transport: UnitOwnerValueObservationV1,
}

impl PostLoopTransport {
    // Shared consumption checks do not independently grant a control capability.
    fn validate(&self, evidence: &InvariantLoopBoundaries) -> Option<&UnitOwnerValueObservationV1> {
        let graph = &self.transport;
        let source = evidence.closed.statement().source_unit();
        let [
            Event::Create(initializer, created),
            Event::Bind(held, bound, Value::Definition(input)),
            Event::Move(expression, moved_from, Value::Definition(moved)),
            Event::Bind(retired, used, Value::Definition(installed)),
            Event::Drop(point, dropped, Value::Definition(consumed)),
        ] = graph.prefix()
        else {
            return None;
        };
        (evidence.is_consistent()
            && graph.is_consistent()
            && graph.choice().is_none()
            && graph.suffix().is_empty()
            && graph.producers().definitions().len() == 1
            && graph.producers().callable() == evidence.inputs.callable()
            && std::sync::Arc::ptr_eq(&graph.catalog.identity, &evidence.inputs.identity)
            && graph
                .producers()
                .definition(evidence.entry.owner())
                .is_some_and(|definition| {
                    evidence.inputs.definition(evidence.entry.owner()) == Some(definition)
                })
            && self.entry == evidence.exit
            && self.statement.source_unit() == source
            && self.statement != evidence.closed.statement()
            && self.statement != evidence.closed.body()
            && self.expression.source_unit() == source
            && self.retired.source_unit() == source
            && self.retired != evidence.entry.symbol()
            && *initializer == *bound
            && *held == evidence.entry.symbol()
            && *moved_from == *held
            && *expression == self.expression
            && *used == *expression
            && *retired == self.retired
            && *dropped == *retired
            && *point == UnitDropPoint::AfterStatement(self.statement)
            && [created, input, moved, installed, consumed]
                .into_iter()
                .all(|owner| *owner == evidence.entry.owner()))
        .then_some(graph)
    }
}

impl UnitOwnerInvariantLoopObservationV1 {
    /// Actual prefix and post-loop consumption on the normal exit path, together with
    /// this observation's unchanged loop boundaries. Not a phi or runtime trace.
    /// Only straight-line normal bodies provide this optional capability.
    pub fn post_loop_transport_v1(&self) -> Option<&UnitOwnerValueObservationV1> {
        if !self.is_consistent()
            || self.evidence.closed.completion != super::super::LoopBodyCompletion::Fallthrough
        {
            return None;
        }
        self.post_loop.as_deref()?.validate(&self.evidence)
    }
}
impl UnitOwnerInvariantBreakObservationV1 {
    /// Actual prefix and post-loop consumption, together with this unchanged Break
    /// and condition-false exit evidence. Not a general exit phi or runtime trace.
    pub fn post_loop_transport_v1(&self) -> Option<&UnitOwnerValueObservationV1> {
        if !self.is_consistent() {
            return None;
        }
        self.post_loop.as_deref()?.validate(&self.evidence)
    }
}
impl InvariantLoopValueV1 {
    pub(in crate::ownership_checking::compilation_unit) fn suffix_boundaries(
        &self,
    ) -> Option<&InvariantLoopBoundaries> {
        match self {
            Self::Backedge(value)
                if value.is_consistent()
                    && value.evidence.closed.completion
                        == super::super::LoopBodyCompletion::Fallthrough
                    && value.post_loop.is_none() =>
            {
                Some(&value.evidence)
            }
            Self::Break(value) if value.is_consistent() && value.post_loop.is_none() => {
                Some(&value.evidence)
            }
            _ => None,
        }
    }
    pub(in crate::ownership_checking::compilation_unit) fn observe_post_loop(
        &mut self,
        statement: UnitStatementId,
        expression: UnitExpressionId,
        retired: UnitSymbolId,
        entry: UnitOwnerLoopBindingV1,
        transport: UnitOwnerValueObservationV1,
    ) {
        if self.suffix_boundaries().is_none() {
            return;
        }
        let suffix = Box::new(PostLoopTransport {
            statement,
            expression,
            retired,
            entry,
            transport,
        });
        match self {
            Self::Backedge(value) => {
                value.post_loop = Some(suffix);
                if value.post_loop_transport_v1().is_none() {
                    value.post_loop = None;
                }
            }
            Self::Break(value) => {
                value.post_loop = Some(suffix);
                if value.post_loop_transport_v1().is_none() {
                    value.post_loop = None;
                }
            }
            _ => {}
        }
    }
}
