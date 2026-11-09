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

impl UnitOwnerInvariantLoopObservationV1 {
    /// Actual prefix and post-loop consumption on the normal exit path, together with
    /// this observation's unchanged loop boundaries. Not a phi or runtime trace.
    /// Only straight-line normal bodies provide this optional capability.
    pub fn post_loop_transport_v1(&self) -> Option<&UnitOwnerValueObservationV1> {
        let suffix = self.post_loop.as_ref()?;
        let graph = &suffix.transport;
        let source = self.statement().source_unit();
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
        (self.is_consistent()
            && self.evidence.closed.completion == super::super::LoopBodyCompletion::Fallthrough
            && graph.is_consistent()
            && graph.choice().is_none()
            && graph.suffix().is_empty()
            && graph.producers().definitions().len() == 1
            && graph.producers().callable() == self.input_producers().callable()
            && std::sync::Arc::ptr_eq(&graph.catalog.identity, &self.evidence.inputs.identity)
            && graph
                .producers()
                .definition(self.entry().owner())
                .is_some_and(|definition| {
                    self.input_producers().definition(self.entry().owner()) == Some(definition)
                })
            && suffix.entry == *self.exit()
            && suffix.statement.source_unit() == source
            && suffix.statement != self.statement()
            && suffix.statement != self.body()
            && suffix.expression.source_unit() == source
            && suffix.retired.source_unit() == source
            && suffix.retired != self.entry().symbol()
            && *initializer == *bound
            && *held == self.entry().symbol()
            && *moved_from == *held
            && *expression == suffix.expression
            && *used == *expression
            && *retired == suffix.retired
            && *dropped == *retired
            && *point == UnitDropPoint::AfterStatement(suffix.statement)
            && [created, input, moved, installed, consumed]
                .into_iter()
                .all(|owner| *owner == self.entry().owner()))
        .then_some(graph)
    }

    pub(in crate::ownership_checking::compilation_unit) fn observe_post_loop(
        &mut self,
        statement: UnitStatementId,
        expression: UnitExpressionId,
        retired: UnitSymbolId,
        entry: UnitOwnerLoopBindingV1,
        transport: UnitOwnerValueObservationV1,
    ) {
        if self.post_loop.is_some() {
            return;
        }
        self.post_loop = Some(Box::new(PostLoopTransport {
            statement,
            expression,
            retired,
            entry,
            transport,
        }));
        if self.post_loop_transport_v1().is_none() {
            self.post_loop = None;
        }
    }
}
