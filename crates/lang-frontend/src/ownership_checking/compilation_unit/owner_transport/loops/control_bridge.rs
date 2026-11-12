//! Static, whole-vector alias bridge; not a dynamic exit token or next-loop permission.
use super::*;
use crate::name_resolution::UnitSymbolId;

/// Actual aliases from one complete live exit, retaining both static predecessor sources.
#[derive(Clone, Debug)]
pub struct UnitOwnerControlBridgeEvidenceV1 {
    prior: UnitOwnerCounterTailEvidenceV1,
    next_control: UnitStatementId,
    next_condition: UnitExpressionId,
    next_body: UnitStatementId,
    sources: Vec<UnitSymbolId>,
    targets: Vec<UnitSymbolId>,
}
impl UnitOwnerControlBridgeEvidenceV1 {
    pub(in crate::ownership_checking::compilation_unit) fn seal(
        prior: UnitOwnerCounterTailEvidenceV1,
        next_control: UnitStatementId,
        next_condition: UnitExpressionId,
        next_body: UnitStatementId,
        sources: Vec<UnitSymbolId>,
        targets: Vec<UnitSymbolId>,
    ) -> Option<Self> {
        let value = Self {
            prior,
            next_control,
            next_condition,
            next_body,
            sources,
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
    /// Actual successfully completed condition expression of the next loop.
    pub fn next_condition(&self) -> UnitExpressionId {
        self.next_condition
    }
    /// Actual planner body-entry target, not proof that the body completed.
    pub fn next_body(&self) -> UnitStatementId {
        self.next_body
    }
    /// Live predecessor roots in original root_slots order; retired roots are excluded.
    pub fn sources(&self) -> &[UnitSymbolId] {
        &self.sources
    }
    /// Targets correspond one-to-one to sources, never alias evaluation order.
    pub fn targets(&self) -> &[UnitSymbolId] {
        &self.targets
    }
    /// Validate the exact live subset and distinct targets against the sealed exit partition.
    pub fn is_consistent(&self) -> bool {
        self.prior.is_consistent()
            && self.next_control != self.prior.statement()
            && self.next_control.source_unit() == self.prior.statement().source_unit()
            && self.next_condition.source_unit() == self.next_control.source_unit()
            && self.next_body.source_unit() == self.next_control.source_unit()
            && self.next_body != self.next_control
            && self.targets.len() == self.sources.len()
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
            && self.prior.false_exit_partition().is_some_and(|(live, _)| {
                live.len() == self.sources.len()
                    && self
                        .prior
                        .root_slots()
                        .iter()
                        .filter(|root| live.contains(&root.root()))
                        .map(|root| root.root())
                        .eq(self.sources.iter().copied())
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
                    assert_eq!(bridge.sources().len(), 2);
                    assert_ne!(bridge.next_control(), bridge.prior().statement());
                    let condition = fixture.parsed[0]
                        .ast()
                        .expressions()
                        .get(bridge.next_condition().expression())
                        .unwrap();
                    assert_eq!(fixture.sources.slice(condition.span()).unwrap(), "g1");
                    let body = fixture.parsed[0]
                        .ast()
                        .statements()
                        .get(bridge.next_body().statement())
                        .unwrap();
                    assert!(
                        fixture
                            .sources
                            .slice(body.span())
                            .unwrap()
                            .contains("replace(&a1,")
                    );

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
    pub(super) fn partial_source(one_live: bool) -> String {
        source(false, false)
            .replace("var b0 = \"b\";", "var b0 = \"b\"; var c0 = \"c\";")
            .replace(
                "val y = replace(&b0, \"y\");",
                "val y = replace(&b0, \"y\"); val z = replace(&c0, \"z\");",
            )
            .replace(
                "var b1 = b0;",
                if one_live {
                    "var b1 = \"fresh\";"
                } else {
                    "var b1 = c0;"
                },
            )
    }
    #[test]
    fn control_bridge_partial_exit_keeps_only_live_sources() {
        for one_live in [false, true] {
            let text = partial_source(one_live);
            let fixture = Fixture::new(&[("partial-control-bridge.ko", &text)]);
            let owned = fixture.shared();
            assert!(owned.diagnostics().is_empty());
            let observation = &owned.owner_observations()[0];
            let bridge = observation.control_bridge_evidence_v1();
            assert_eq!(bridge.is_some(), !one_live);
            if let Some(bridge) = bridge {
                assert_eq!(bridge.prior().root_slots().len(), 3);
                assert_eq!(bridge.targets().len(), 2);
                assert_eq!(bridge.sources().len(), 2);
                let (live, drops) = bridge.prior().false_exit_partition().unwrap();
                assert_eq!(live.len(), 2);
                assert_eq!(drops.len(), 1);
                let names = bridge
                    .sources()
                    .iter()
                    .map(|symbol| {
                        let resolution = fixture.names.names().source_units()
                            [symbol.source_unit().index()]
                        .resolution();
                        fixture
                            .sources
                            .slice(resolution.symbols()[symbol.symbol().index()].span())
                            .unwrap()
                    })
                    .collect::<Vec<_>>();
                assert_eq!(names, ["a0", "c0"]);

                assert!(observation.counter_tail_evidence_v1().is_none());
                assert!(observation.counter_batch_evidence_v1().is_none());
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

#[cfg(test)]
mod partial_fault_tests {
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    #[test]
    fn ci_only_control_bridge_rejects_missing_or_retired_sources() {
        let text = super::tests::partial_source(false);
        let fixture = Fixture::new(&[("partial-bridge-fault.ko", &text)]);
        let owned = fixture.shared();
        let bridge = owned.owner_observations()[0]
            .control_bridge_evidence_v1()
            .unwrap();
        let mut missing = bridge.clone();
        missing.sources.pop();
        assert!(!missing.is_consistent());
        let mut swapped = bridge.clone();
        swapped.sources.reverse();
        assert!(!swapped.is_consistent());
        let retired = bridge
            .prior
            .root_slots()
            .iter()
            .find(|slot| !bridge.sources.contains(&slot.root()))
            .unwrap()
            .root();
        let mut revived = bridge.clone();
        revived.sources[1] = retired;
        assert!(!revived.is_consistent());
    }
}
