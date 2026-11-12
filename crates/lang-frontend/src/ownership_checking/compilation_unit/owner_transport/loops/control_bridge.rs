//! Static, whole-vector alias bridge; not a dynamic exit token or next-loop permission.
use super::*;
use crate::name_resolution::UnitSymbolId;

/// Actual aliases from one complete live exit, retaining both static predecessor sources.
#[derive(Clone, Debug)]
pub struct UnitOwnerControlBridgeEvidenceV1 {
    prior: UnitOwnerCounterTailEvidenceV1,
    next_control: UnitStatementId,
    targets: Vec<UnitSymbolId>,
}
impl UnitOwnerControlBridgeEvidenceV1 {
    pub(in crate::ownership_checking::compilation_unit) fn seal(
        prior: UnitOwnerCounterTailEvidenceV1,
        next_control: UnitStatementId,
        targets: Vec<UnitSymbolId>,
    ) -> Option<Self> {
        let value = Self {
            prior,
            next_control,
            targets,
        };
        value.is_consistent().then_some(value)
    }
    /// Completed predecessor contract, not a selected dynamic exit instance.
    pub fn prior(&self) -> &UnitOwnerCounterTailEvidenceV1 {
        &self.prior
    }
    /// The next actual control whose entry locations were observed.
    pub fn next_control(&self) -> UnitStatementId {
        self.next_control
    }
    /// Targets in the predecessor root_slots order, never alias evaluation order.
    pub fn targets(&self) -> &[UnitSymbolId] {
        &self.targets
    }
    /// Validate complete, distinct target locations and a wholly live predecessor exit.
    pub fn is_consistent(&self) -> bool {
        self.prior.is_consistent()
            && self.next_control != self.prior.statement()
            && self.next_control.source_unit() == self.prior.statement().source_unit()
            && self.targets.len() == self.prior.root_slots().len()
            && self.targets.len() >= 2
            && self.targets.iter().enumerate().all(|(i, target)| {
                target.source_unit() == self.next_control.source_unit()
                    && !self.targets[..i].contains(target)
                    && self
                        .prior
                        .root_slots()
                        .iter()
                        .all(|root| root.root() != *target)
            })
            && self
                .prior
                .false_exit_partition()
                .is_some_and(|(live, drops)| {
                    drops.is_empty()
                        && live.len() == self.targets.len()
                        && self
                            .prior
                            .root_slots()
                            .iter()
                            .all(|root| live.contains(&root.root()))
                })
    }
}
impl UnitOwnerObservation {
    /// Read-only static alias evidence, mutually exclusive with owner capabilities.
    pub fn control_bridge_evidence_v1(&self) -> Option<&UnitOwnerControlBridgeEvidenceV1> {
        let bridge = self.control_bridge.as_deref()?;
        (bridge.is_consistent()
            && std::sync::Arc::ptr_eq(&self.identity, &bridge.prior.analysis)
            && self.callable == bridge.prior.callable()
            && self.definitions.is_empty()
            && self.events.is_empty()
            && self.counter_batch.is_none()
            && self.counter_tail.is_none()
            && self.closed_loop_batch.is_none()
            && self.multi_root_boundary.is_none()
            && self.loop_value.is_none()
            && self.invariant_loop_value.is_none()
            && self.open_loop_value.is_none()
            && self.choice_value.is_none()
            && matches!(self.status, super::super::UnitOwnerObservationStatus::Unsupported {
                point: UnitDropPoint::AfterStatement(control) | UnitDropPoint::LoopExit(control),
                reason: super::super::UnitOwnerUnsupportedReason::Syntax,
            } if control == bridge.prior.statement()))
        .then_some(bridge)
    }
}

#[cfg(test)]
mod tests {
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    fn source(reverse: bool, extra: bool) -> String {
        let aliases = if reverse {
            "val saved_b = b0; val saved_a = a0; var b1 = saved_b; var a1 = saved_a;"
        } else {
            "var a1 = a0; var b1 = b0;"
        };
        let extra = if extra { "val extra = \"unused\";" } else { "" };
        format!(
            r#"fun run(n: Int) {{
            var n0 = n; var g0 = n0 > 0; var a0 = "a"; var b0 = "b";
            while(g0) {{ val x = replace(&a0, "x"); val y = replace(&b0, "y"); n0 = n0 - 1; g0 = n0 > 0 }};
            var n1 = n; var g1 = n1 > 0; {aliases} {extra}
            while(g1) {{ val x = replace(&a1, "x1"); val y = replace(&b1, "y1"); n1 = n1 - 1; g1 = n1 > 0 }};
            println(a1); println(b1)
        }}"#
        )
    }
    #[test]
    fn control_bridge_product_keeps_original_root_order_and_no_old_permissions() {
        for reverse in [false, true] {
            for extra in [false, true] {
                let text = source(reverse, extra);
                let fixture = Fixture::new(&[("control-bridge-product.ko", &text)]);
                let owned = fixture.shared();
                assert!(owned.diagnostics().is_empty());
                let observation = &owned.owner_observations()[0];
                let product = observation.control_bridge_evidence_v1();
                assert_eq!(product.is_some(), !extra);
                if let Some(bridge) = product {
                    assert!(bridge.is_consistent());
                    assert_eq!(bridge.targets().len(), 2);
                    assert_ne!(bridge.next_control(), bridge.prior().statement());
                    assert_ne!(bridge.targets()[0], bridge.targets()[1]);
                    let target_names = bridge
                        .targets()
                        .iter()
                        .map(|target| {
                            let resolution = fixture.names.names().source_units()
                                [target.source_unit().index()]
                            .resolution();
                            fixture
                                .sources
                                .slice(resolution.symbols()[target.symbol().index()].span())
                                .unwrap()
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(target_names, ["a1", "b1"]);

                    assert!(observation.counter_tail_evidence_v1().is_none());
                    assert!(observation.counter_batch_evidence_v1().is_none());
                    assert!(observation.multi_root_boundary_v1().is_none());
                    assert!(observation.value_observation_v1().is_none());
                }
            }
        }
    }
    // Fault injection is compiled locally and executed only in remote CI.
    #[test]
    fn ci_only_control_bridge_rejects_partial_foreign_collision_and_deferred() {
        let text = source(true, false);
        let fixture = Fixture::new(&[("control-bridge-fault.ko", &text)]);
        let owned = fixture.shared();
        let observation = &owned.owner_observations()[0];
        let bridge = observation.control_bridge_evidence_v1().unwrap();
        let mut partial = bridge.clone();
        partial.targets.pop();
        assert!(!partial.is_consistent());
        let mut duplicate = bridge.clone();
        duplicate.targets[1] = duplicate.targets[0];
        assert!(!duplicate.is_consistent());
        let mut original = bridge.clone();
        original.targets[0] = original.prior.root_slots()[0].root();
        assert!(!original.is_consistent());
        let other = fixture.shared();
        let mut foreign = observation.clone();
        foreign.control_bridge = other.owner_observations()[0].control_bridge.clone();
        assert!(foreign.control_bridge_evidence_v1().is_none());
        let mut mixed = observation.clone();
        mixed.counter_tail = Some(Box::new(bridge.prior.clone()));
        assert!(mixed.control_bridge_evidence_v1().is_none());
        assert!(mixed.counter_tail_evidence_v1().is_none());
        let deferred = super::super::publish_observations(vec![observation.clone()], true, false);
        assert!(deferred[0].control_bridge_evidence_v1().is_none());
    }
}
