//! Separately sealed sequential closed-loop catalog; not a runtime iteration trace.
use super::*;
/// Ordered independent closed-loop products from one callable analysis.
/// This catalog does not assert joint reachability, termination, or carried owners.
#[derive(Clone, Debug)]
pub struct UnitOwnerClosedLoopBatchV1 {
    controls: Vec<UnitStatementId>,
    products: Vec<UnitOwnerLoopObservationV1>,
}
impl UnitOwnerClosedLoopBatchV1 {
    pub(in crate::ownership_checking::compilation_unit) fn seal(
        controls: Vec<UnitStatementId>,
        products: Vec<UnitOwnerLoopObservationV1>,
    ) -> Option<Self> {
        let value = Self { controls, products };
        value.is_consistent().then_some(value)
    }
    /// Products in original AST statement order, not runtime execution order.
    pub fn products(&self) -> &[UnitOwnerLoopObservationV1] {
        &self.products
    }
    /// Check exact cardinality, order, source/catalog identity and independent body catalogs.
    pub fn is_consistent(&self) -> bool {
        let Some(first) = self.products.first() else {
            return false;
        };
        self.controls.len() >= 2
            && self.controls.len() == self.products.len()
            && self.controls.iter().zip(&self.products).enumerate().all(
                |(index, (control, product))| {
                    *control == product.statement
                        && product.is_consistent()
                        && product.callable == first.callable
                        && std::sync::Arc::ptr_eq(&product.identity, &first.identity)
                        && self.products[..index].iter().all(|previous| {
                            previous.statement != product.statement
                                && previous.body != product.body
                                && !std::sync::Arc::ptr_eq(
                                    &previous.body_identity,
                                    &product.body_identity,
                                )
                        })
                },
            )
    }
}
impl UnitOwnerObservation {
    /// Separate closed sequence capability; never a single-loop or straight-line projection.
    pub fn closed_loop_batch_v1(&self) -> Option<&UnitOwnerClosedLoopBatchV1> {
        let batch = self.closed_loop_batch.as_deref()?;
        let first = batch.products.first()?;
        (self.private_control_chain.is_none()
            && self.control_bridge.is_none()
            && self.counter_batch.is_none()
            && self.multi_root_boundary.is_none()
            && self.loop_value.is_none()
            && self.invariant_loop_value.is_none()
            && self.open_loop_value.is_none()
            && self.choice_value.is_none()
            && self.definitions.is_empty()
            && self.events.is_empty()
            && matches!(self.status, super::super::UnitOwnerObservationStatus::Unsupported {
                point: UnitDropPoint::AfterStatement(control) | UnitDropPoint::LoopExit(control),
                reason: super::super::UnitOwnerUnsupportedReason::Syntax,
            } if control == first.statement)
            && first.callable == self.callable
            && std::sync::Arc::ptr_eq(&first.identity, &self.identity)
            && batch.is_consistent())
        .then_some(batch)
    }
}

#[cfg(test)]
mod tests {
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    #[test]
    fn ci_only_closed_batch_rejects_partial_reordered_duplicate_and_foreign_products() {
        let fixture = Fixture::new(&[(
            "batch-guards.ko",
            r#"fun run(flag: Boolean) {
            while(flag) { val first = "first"; break }
            while(flag) { val second = "second"; break }
        }"#,
        )]);
        let owned = fixture.shared();
        let observation = &owned.owner_observations()[0];
        let batch = observation.closed_loop_batch_v1().unwrap();
        let mut partial = batch.clone();
        partial.products.pop();
        assert!(!partial.is_consistent());
        let mut reordered = batch.clone();
        reordered.products.reverse();
        assert!(!reordered.is_consistent());
        let mut duplicate = batch.clone();
        duplicate.products[1] = duplicate.products[0].clone();
        duplicate.controls[1] = duplicate.controls[0];
        assert!(!duplicate.is_consistent());
        let reanalyzed = fixture.shared();
        let mut mixed = batch.clone();
        mixed.products[1] = reanalyzed.owner_observations()[0]
            .closed_loop_batch_v1()
            .unwrap()
            .products[1]
            .clone();
        assert!(!mixed.is_consistent());
        let mut foreign = observation.clone();
        foreign.closed_loop_batch = reanalyzed.owner_observations()[0].closed_loop_batch.clone();
        assert!(foreign.closed_loop_batch_v1().is_none());
        let deferred = super::super::publish_observations(vec![observation.clone()], true, false);
        assert!(deferred[0].closed_loop_batch_v1().is_none());
        let mut collision = observation.clone();
        collision.loop_value = Some(Box::new(batch.products[0].clone()));
        assert!(collision.closed_loop_batch_v1().is_none());
        assert!(collision.loop_observation_v1().is_none());
        assert!(collision.value_observation_v1().is_none());
    }
}
