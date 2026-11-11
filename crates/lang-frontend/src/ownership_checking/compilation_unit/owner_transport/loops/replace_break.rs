mod call_consumer;
mod consumer;
mod return_consumer;
pub(in crate::ownership_checking::compilation_unit) use call_consumer::CallConsumerTarget;
pub use call_consumer::{UnitOwnerReplaceBreakCallActionV1, UnitOwnerReplaceBreakCallConsumerV1};
pub use consumer::{UnitOwnerReplaceBreakConsumerActionV1, UnitOwnerReplaceBreakConsumerV1};
pub(in crate::ownership_checking::compilation_unit) use return_consumer::ReturnConsumerTarget;
pub use return_consumer::{
    UnitOwnerReplaceBreakReturnActionV1, UnitOwnerReplaceBreakReturnConsumerV1,
};
// One actual replacement followed by Break and an optional bounded consumer.
use super::super::{
    UnitOwnerCallable, UnitOwnerId, UnitOwnerObservation, UnitOwnerOrigin,
    UnitOwnerProducerCatalogV1, UnitOwnerTransportEvent,
};
use super::{LoopBodyCompletion, UnitOwnerLoopEdgeV1};
use crate::{
    name_resolution::UnitSymbolId,
    ownership_checking::UnitDropPoint,
    source::Span,
    type_checking::{UnitExpressionId, UnitStatementId},
};
use std::sync::Arc;

/// One independently read planner boundary. Its token belongs to the documented catalog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitOwnerReplaceBreakStateV1 {
    pub(in crate::ownership_checking::compilation_unit) symbol: UnitSymbolId,
    pub(in crate::ownership_checking::compilation_unit) owner: UnitOwnerId,
    pub(in crate::ownership_checking::compilation_unit) origin: Span,
    pub(in crate::ownership_checking::compilation_unit) declaration: Span,
    pub(in crate::ownership_checking::compilation_unit) scope_depth: usize,
}
impl UnitOwnerReplaceBreakStateV1 {
    /// Actual source-qualified root binding.
    pub fn symbol(&self) -> UnitSymbolId {
        self.symbol
    }
    /// P resolves in input_producers; Q resolves in body_producers, never by index alone.
    pub fn owner(&self) -> UnitOwnerId {
        self.owner.clone()
    }
    /// Actual planner origin: binding marker for P, checked commit span for Q.
    pub fn origin(&self) -> Span {
        self.origin
    }
    /// Root declaration marker, preserved by replacement.
    pub fn declaration(&self) -> Span {
        self.declaration
    }
    /// Lexical planner depth, not a dynamic allocation generation.
    pub fn scope_depth(&self) -> usize {
        self.scope_depth
    }
}

/// Bounded open-body witness: P is consumed in the body, Q remains at its Break exit.
/// The independent condition-false exit still carries P. Only the exact suffix may consume both incoming alternatives.
#[derive(Clone, Debug)]
pub struct UnitOwnerReplaceBreakObservationV1 {
    pub(in crate::ownership_checking::compilation_unit) suffix: ReplaceBreakSuffix,
    pub(in crate::ownership_checking::compilation_unit) return_consumer:
        Option<UnitOwnerReplaceBreakReturnConsumerV1>,
    pub(in crate::ownership_checking::compilation_unit) call_consumer:
        Option<UnitOwnerReplaceBreakCallConsumerV1>,
    pub(in crate::ownership_checking::compilation_unit) consumer:
        Option<UnitOwnerReplaceBreakConsumerV1>,
    pub(in crate::ownership_checking::compilation_unit) prefix: super::UnitOwnerOpenLoopPrefixV1,
    pub(in crate::ownership_checking::compilation_unit) completed_identity: Arc<()>,
    pub(in crate::ownership_checking::compilation_unit) completed_jump: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) body_completion:
        UnitOwnerReplaceBreakStateV1,
    pub(in crate::ownership_checking::compilation_unit) edges: Vec<UnitOwnerLoopEdgeV1>,
}
#[derive(Clone, Debug)]
pub(in crate::ownership_checking::compilation_unit) enum ReplaceBreakSuffix {
    Bind {
        initializer: UnitExpressionId,
        statement: UnitStatementId,
        retired: UnitSymbolId,
    },
    Call(CallConsumerTarget),
    Return(ReturnConsumerTarget),
}
impl UnitOwnerReplaceBreakObservationV1 {
    /// Actual Take followed by delivery through the unique owned String Return.
    pub fn suffix_return_consumer_v1(&self) -> Option<&UnitOwnerReplaceBreakReturnConsumerV1> {
        self.return_consumer
            .as_ref()
            .filter(|consumer| self.is_consistent() && consumer.matches_boundary(self))
    }

    /// Actual Take followed by transfer into the checked ordinary own call.
    pub fn suffix_call_consumer_v1(&self) -> Option<&UnitOwnerReplaceBreakCallConsumerV1> {
        self.call_consumer
            .as_ref()
            .filter(|consumer| self.is_consistent() && consumer.matches_boundary(self))
    }

    /// Complete actual consumption by the unique admitted tail, when available.
    pub fn suffix_consumer_v1(&self) -> Option<&UnitOwnerReplaceBreakConsumerV1> {
        self.consumer
            .as_ref()
            .filter(|consumer| self.is_consistent() && consumer.matches_boundary(self))
    }

    /// Actual callable, shared by both catalogs.
    pub fn callable(&self) -> UnitOwnerCallable {
        self.callable
    }
    /// Actual While statement.
    pub fn statement(&self) -> UnitStatementId {
        self.statement
    }
    /// Actual Boolean condition, retaining admitted logical, comparison and Group/Not identities.
    pub fn condition(&self) -> UnitExpressionId {
        self.condition
    }
    /// Actual body statement.
    pub fn body(&self) -> UnitStatementId {
        self.body
    }
    /// Bare Break whose cleanup actually completed.
    pub fn break_expression(&self) -> UnitExpressionId {
        self.jump
    }
    /// Actual prefix producer P; not proof of final consumption on the false edge.
    pub fn input_producers(&self) -> &UnitOwnerProducerCatalogV1 {
        &self.inputs
    }
    /// Actual fresh body producer Q; not a closed body transport.
    pub fn body_producers(&self) -> &UnitOwnerProducerCatalogV1 {
        &self.producers
    }
    /// Actual state after condition evaluation, carrying P.
    pub fn entry(&self) -> &UnitOwnerReplaceBreakStateV1 {
        &self.entry
    }
    /// Independently read state after Break cleanup, carrying Q.
    pub fn break_boundary(&self) -> &UnitOwnerReplaceBreakStateV1 {
        &self.installed
    }
    /// Independently read state after the body's actual early return, carrying Q.
    pub fn body_completion(&self) -> &UnitOwnerReplaceBreakStateV1 {
        &self.body_completion
    }
    /// Original condition state after LoopExit cleanup, carrying P, not a common exit.
    pub fn condition_false_exit(&self) -> &UnitOwnerReplaceBreakStateV1 {
        &self.condition_false
    }
    /// Exactly six actual body events; tokens may belong to either catalog.
    pub fn body_events(&self) -> &[UnitOwnerTransportEvent] {
        &self.events
    }
    /// EnterBody, BreakExit and ConditionFalseExit only, not a runtime trace.
    pub fn edges(&self) -> &[UnitOwnerLoopEdgeV1] {
        &self.edges
    }
    /// Verify the fixed cross-catalog transfer, independent boundaries and completion seal.
    pub fn is_consistent(&self) -> bool {
        self.prefix.prefix_is_consistent()
            && self
                .inputs
                .definition(self.entry.owner())
                .is_some_and(|definition| {
                    definition.origin() == UnitOwnerOrigin::String(self.initial)
                })
            && self.events.len() == 6
            && Arc::ptr_eq(&self.completed_identity, &self.producers.identity)
            && self.completed_jump == self.jump
            && self.installed == self.body_completion
            && self
                .consumer
                .as_ref()
                .is_none_or(|consumer| consumer.matches_boundary(self))
            && self
                .call_consumer
                .as_ref()
                .is_none_or(|consumer| consumer.matches_boundary(self))
            && self
                .return_consumer
                .as_ref()
                .is_none_or(|consumer| consumer.matches_boundary(self))
            && [
                self.consumer.is_some(),
                self.call_consumer.is_some(),
                self.return_consumer.is_some(),
            ]
            .into_iter()
            .filter(|present| *present)
            .count()
                <= 1
            && self.edges
                == LoopBodyCompletion::Break(self.jump).edges(
                    self.statement,
                    self.condition,
                    self.body,
                )
    }
}
impl UnitOwnerObservation {
    /// Separate one-replacement Break capability with an optional exact suffix consumer.
    pub fn replace_break_observation_v1(&self) -> Option<&UnitOwnerReplaceBreakObservationV1> {
        match self.open_loop_observation_v1()? {
            super::UnitOwnerOpenLoopObservationV1::BreakLive(value) => Some(value),
            super::UnitOwnerOpenLoopObservationV1::ReturnDelivered(_) => None,
        }
    }
}

impl std::ops::Deref for UnitOwnerReplaceBreakObservationV1 {
    type Target = super::UnitOwnerOpenLoopPrefixV1;
    fn deref(&self) -> &Self::Target {
        &self.prefix
    }
}
