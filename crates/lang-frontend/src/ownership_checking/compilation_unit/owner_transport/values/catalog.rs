//! Independent producer identity, never populated from cleanup or a selection node.
use super::super::{
    UnitOwnerCallable, UnitOwnerDefinition, UnitOwnerId, UnitOwnerObservation,
    UnitOwnerObservationStatus, UnitOwnerOrigin,
};
use std::sync::Arc;

/// Read-only real String producer catalog, independent of the legacy event vocabulary.
#[derive(Clone, Debug)]
pub struct UnitOwnerProducerCatalogV1 {
    pub(in crate::ownership_checking::compilation_unit) identity: Arc<()>,
    pub(in crate::ownership_checking::compilation_unit) callable: UnitOwnerCallable,
    pub(in crate::ownership_checking::compilation_unit) definitions: Vec<UnitOwnerDefinition>,
}
impl UnitOwnerProducerCatalogV1 {
    /// Callable in which these actual producers were evaluated.
    pub fn callable(&self) -> UnitOwnerCallable {
        self.callable
    }
    /// Actual String definitions in deterministic completion order.
    pub fn definitions(&self) -> &[UnitOwnerDefinition] {
        &self.definitions
    }
    /// Look up a token only within this exact analysis product and callable.
    pub fn definition(&self, id: UnitOwnerId) -> Option<&UnitOwnerDefinition> {
        self.definitions
            .get(id.index)
            .filter(|definition| definition.id == id)
    }
    pub(in crate::ownership_checking::compilation_unit) fn from_legacy(
        legacy: &UnitOwnerObservation,
    ) -> Option<Self> {
        if legacy.status != UnitOwnerObservationStatus::Complete || !legacy.is_consistent() {
            return None;
        }
        let catalog = Self {
            identity: legacy.identity.clone(),
            callable: legacy.callable,
            definitions: legacy.definitions.clone(),
        };
        catalog.is_consistent().then_some(catalog)
    }
    /// Verify product, callable, dense index, unique origin and source identities.
    /// This checks definitions only, not typed admission or transport conservation.
    pub fn is_consistent(&self) -> bool {
        let UnitOwnerCallable::Function(function) = self.callable else {
            return false;
        };
        self.definitions
            .iter()
            .enumerate()
            .all(|(index, definition)| {
                let UnitOwnerOrigin::String(expression) = definition.origin else {
                    return false;
                };
                Arc::ptr_eq(&definition.id.identity, &self.identity)
                    && definition.id.callable == self.callable
                    && definition.id.index == index
                    && expression.source_unit() == function.source_unit()
                    && !self.definitions[..index]
                        .iter()
                        .any(|prior| prior.origin == definition.origin)
            })
    }
}
