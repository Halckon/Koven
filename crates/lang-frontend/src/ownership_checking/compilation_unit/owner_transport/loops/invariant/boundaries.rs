//! Shared private evidence does not choose a public control-flow capability.
use super::{
    UnitExpressionId, UnitOwnerLoopBindingV1, UnitOwnerLoopObservationV1,
    UnitOwnerProducerCatalogV1,
};

#[derive(Clone, Debug)]
pub(in crate::ownership_checking::compilation_unit) struct InvariantLoopBoundaries {
    pub(in crate::ownership_checking::compilation_unit) closed: UnitOwnerLoopObservationV1,
    pub(in crate::ownership_checking::compilation_unit) inputs: UnitOwnerProducerCatalogV1,
    pub(in crate::ownership_checking::compilation_unit) entry: UnitOwnerLoopBindingV1,
    pub(in crate::ownership_checking::compilation_unit) body_completion: UnitOwnerLoopBindingV1,
    pub(in crate::ownership_checking::compilation_unit) exit: UnitOwnerLoopBindingV1,
    pub(in crate::ownership_checking::compilation_unit) transfer:
        Option<(std::sync::Arc<()>, UnitExpressionId, UnitOwnerLoopBindingV1)>,
}
impl InvariantLoopBoundaries {
    pub(super) fn is_consistent(&self) -> bool {
        self.closed.is_consistent()
            && self.inputs.is_consistent()
            && self.inputs.definitions().len() == 1
            && self.inputs.callable() == self.closed.callable
            && std::sync::Arc::ptr_eq(&self.inputs.identity, &self.closed.identity)
            && self.entry == self.body_completion
            && self.entry == self.exit
            && self.entry.symbol.source_unit() == self.closed.statement().source_unit()
            && self.inputs.definition(self.entry.owner()).is_some()
    }
    pub(super) fn transfer_matches(&self, expected: UnitExpressionId) -> bool {
        self.transfer
            .as_ref()
            .is_some_and(|(identity, actual, binding)| {
                expected == *actual
                    && actual.source_unit() == self.closed.statement().source_unit()
                    && *actual != self.closed.condition()
                    && std::sync::Arc::ptr_eq(identity, &self.closed.body_identity)
                    && binding == &self.entry
            })
    }
    pub(super) fn transfer_boundary(&self) -> Option<(UnitExpressionId, &UnitOwnerLoopBindingV1)> {
        self.transfer
            .as_ref()
            .map(|(_, expression, binding)| (*expression, binding))
    }
}
