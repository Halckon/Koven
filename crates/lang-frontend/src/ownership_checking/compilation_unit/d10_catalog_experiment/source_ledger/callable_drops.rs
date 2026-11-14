//! Successful real callable traversals, not owner-transport or CFG authority.
use super::*;
/// The actual planner body boundary. Skipped/deferred functions are not visits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropCallable {
    /// Original function item. An absent body is distinct from a visited empty body.
    Function {
        /// Source-qualified original item.
        item: UnitItemId,
        /// Whether this declaration actually has a body.
        has_body: bool,
    },
    /// Original deinitializer body, even when the owner-transport observer rejects it.
    Deinit(UnitItemId),
    /// Lambda body visited in the original second pass.
    Lambda(UnitExpressionId),
}
impl DropCallable {
    /// Source of this actual traversal; not the declaration of every drop target.
    #[must_use]
    pub const fn source(self) -> SourceUnitId {
        match self {
            Self::Function { item, .. } | Self::Deinit(item) => item.source_unit(),
            Self::Lambda(lambda) => lambda.source_unit(),
        }
    }
}
/// Newly appended globally normalized/deduplicated flat facts, not independent callable output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallableDropRecord {
    callable: DropCallable,
    drops: Vec<UnitDropFact>,
    conditional_receivers: Vec<UnitConditionalReceiverDropFact>,
}
impl CallableDropRecord {
    pub(crate) fn new(
        callable: DropCallable,
        drops: Vec<UnitDropFact>,
        conditional_receivers: Vec<UnitConditionalReceiverDropFact>,
    ) -> Self {
        Self {
            callable,
            drops,
            conditional_receivers,
        }
    }
    /// Actual successful traversal identity and body presence.
    #[must_use]
    pub const fn callable(&self) -> DropCallable {
        self.callable
    }
    /// Original full points, targets and origins, in append order.
    #[must_use]
    pub fn drops(&self) -> &[UnitDropFact] {
        &self.drops
    }
    /// Original conditional receiver obligations, with all fields and the producer's original
    /// count of already-appended flat drops at that point. Never rebased to this batch.
    #[must_use]
    pub fn conditional_receiver_drops(&self) -> &[UnitConditionalReceiverDropFact] {
        &self.conditional_receivers
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DropCollections {
    pub(crate) roots: Vec<DropTraversalRecord>,
    pub(crate) callables: Vec<CallableDropRecord>,
}
impl DropCollections {
    pub(super) fn callables_match(
        &self,
        source: SourceUnitId,
        drops: &[UnitDropFact],
        conditional: &[UnitConditionalReceiverDropFact],
    ) -> bool {
        self.callables.iter().all(|batch| {
            batch.callable.source() == source
                && batch.conditional_receivers.iter().all(|fact| {
                    super::super::super::owner_transport_validation::point_source(fact.point())
                        == source
                })
                && batch.drops.iter().all(|fact| {
                    super::super::super::owner_transport_validation::point_source(fact.point())
                        == source
                })
        }) && self
            .callables
            .iter()
            .flat_map(|batch| &batch.drops)
            .eq(drops)
            && self
                .callables
                .iter()
                .flat_map(|batch| &batch.conditional_receivers)
                .eq(conditional)
    }
}
