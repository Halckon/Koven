//! First static body boundary only. No header activation or lowering capability.
use super::*;
use crate::name_resolution::UnitSymbolId;
use crate::ownership_checking::{
    UnitDropFact, UnitDropTarget, UnitOwnerId, UnitOwnerOrigin, UnitOwnerProducerCatalogV1,
};

/// One real root's first-traversal transport and returned-old-value cleanup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitOwnerRootBoundarySlotV1 {
    pub(in crate::ownership_checking::compilation_unit) root: UnitSymbolId,
    pub(in crate::ownership_checking::compilation_unit) entry: UnitOwnerId,
    pub(in crate::ownership_checking::compilation_unit) normal: UnitOwnerId,
    pub(in crate::ownership_checking::compilation_unit) producer: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) retirement: UnitDropFact,
}
impl UnitOwnerRootBoundarySlotV1 {
    /// Actual source-qualified root.
    pub fn root(&self) -> UnitSymbolId {
        self.root
    }
    /// Original input catalog token; not a runtime instance.
    pub fn entry(&self) -> UnitOwnerId {
        self.entry.clone()
    }
    /// Actual first-body producer catalog token; not a repeated activation.
    pub fn first_body_output(&self) -> UnitOwnerId {
        self.normal.clone()
    }
    /// Actual direct String completion site.
    pub fn producer(&self) -> UnitExpressionId {
        self.producer
    }
    /// Actual first-body returned-old-value cleanup fact.
    pub fn returned_value_drop(&self) -> UnitDropFact {
        self.retirement
    }
}

/// Read-only static boundary evidence, NOT a loop transport/activation capability.
/// No reachability, subsequent-iteration, joint-lifetime or suffix-consumption claim.
#[derive(Clone, Debug)]
pub struct UnitOwnerMultiRootBoundaryV1 {
    control: UnitStatementId,
    condition: UnitExpressionId,
    body: UnitStatementId,
    guard: UnitSymbolId,
    guard_reset: Option<UnitExpressionId>,
    inputs: UnitOwnerProducerCatalogV1,
    producers: UnitOwnerProducerCatalogV1,
    slots: Vec<UnitOwnerRootBoundarySlotV1>,
    manifest: BoundaryManifest,
    normal_transport: Option<UnitOwnerMultiRootNormalTransportV1>,
    exit_drops: Option<Vec<UnitDropFact>>,
    exit_partition: Option<(Vec<UnitSymbolId>, Vec<UnitDropFact>)>,
    suffix: Option<Vec<(UnitSymbolId, UnitDropFact)>>,
}
// Original evidence map is private to this module, independent from exposed slot clones.
#[derive(Clone, Debug)]
struct BoundaryManifest {
    sites: (
        UnitStatementId,
        UnitExpressionId,
        UnitStatementId,
        UnitSymbolId,
    ),
    slots: Vec<UnitOwnerRootBoundarySlotV1>,
    inputs: Vec<crate::ownership_checking::UnitOwnerDefinition>,
    producers: Vec<crate::ownership_checking::UnitOwnerDefinition>,
    normal_steps: Option<Vec<UnitOwnerRootReplaceStepV1>>,
    guard_reset: Option<UnitExpressionId>,
    exit_drops: Option<Vec<UnitDropFact>>,
    exit_partition: Option<(Vec<UnitSymbolId>, Vec<UnitDropFact>)>,
    suffix: Option<Vec<(UnitSymbolId, UnitDropFact)>>,
}
impl UnitOwnerMultiRootBoundaryV1 {
    // Trusted factory: only the validated planner RootBoundaryEquation calls this.
    // The manifest detects later corruption; it is not an independent CFG derivation.
    pub(in crate::ownership_checking::compilation_unit) fn seal(
        control: UnitStatementId,
        condition: UnitExpressionId,
        body: UnitStatementId,
        guard: UnitSymbolId,
        inputs: UnitOwnerProducerCatalogV1,
        producers: UnitOwnerProducerCatalogV1,
        slots: Vec<UnitOwnerRootBoundarySlotV1>,
    ) -> Option<Self> {
        let manifest = BoundaryManifest {
            sites: (control, condition, body, guard),
            slots: slots.clone(),
            inputs: inputs.definitions().to_vec(),
            producers: producers.definitions().to_vec(),
            normal_steps: None,
            guard_reset: None,
            exit_drops: None,
            exit_partition: None,
            suffix: None,
        };
        let value = Self {
            control,
            condition,
            body,
            guard,
            guard_reset: None,
            inputs,
            producers,
            slots,
            manifest,
            normal_transport: None,
            exit_drops: None,
            exit_partition: None,
            suffix: None,
        };
        value.is_consistent().then_some(value)
    }
    /// Actual completed terminal assignment of false to the Boolean guard, if present.
    pub fn guard_reset(&self) -> Option<UnitExpressionId> {
        self.guard_reset
    }
    pub(in crate::ownership_checking::compilation_unit) fn with_guard_reset(
        mut self,
        reset: Option<UnitExpressionId>,
    ) -> Option<Self> {
        if self.guard_reset.is_some() || self.manifest.guard_reset.is_some() {
            return None;
        }
        self.guard_reset = reset;
        self.manifest.guard_reset = reset;
        self.is_consistent().then_some(self)
    }
    pub(in crate::ownership_checking::compilation_unit) fn with_normal_transport(
        mut self,
        rule: UnitOwnerMultiRootNormalTransportV1,
        actual_steps: Vec<UnitOwnerRootReplaceStepV1>,
    ) -> Option<Self> {
        if self.normal_transport.is_some() || self.manifest.normal_steps.is_some() {
            return None;
        }
        self.manifest.normal_steps = Some(actual_steps);
        self.normal_transport = Some(rule);
        self.is_consistent().then_some(self)
    }
    pub(in crate::ownership_checking::compilation_unit) fn with_condition_false_suffix(
        mut self,
        receipts: Vec<(UnitSymbolId, UnitDropFact)>,
    ) -> Option<Self> {
        if self.suffix.is_some() {
            return None;
        }
        self.manifest.suffix = Some(receipts.clone());
        self.suffix = Some(receipts);
        self.is_consistent().then_some(self)
    }
    pub(super) fn condition_false_suffix(&self) -> Option<&[(UnitSymbolId, UnitDropFact)]> {
        self.suffix.as_deref()
    }
    fn valid_suffix(&self) -> bool {
        self.suffix == self.manifest.suffix
            && self.suffix.as_ref().is_none_or(|receipts| {
                self.exit_partition.as_ref().is_some_and(|(live, _)| {
                    !live.is_empty()
                        && receipts.len() == live.len()
                        && live.iter().all(|root| {
                            receipts.iter().filter(|(known, _)| known == root).count() == 1
                        })
                        && receipts.iter().enumerate().all(|(i, (_, fact))| {
                            let UnitDropTarget::Named(binding) = fact.target() else {
                                return false;
                            };
                            let UnitDropPoint::AfterStatement(statement) = fact.point() else {
                                return false;
                            };
                            binding.source_unit() == self.control.source_unit()
                                && statement.source_unit() == self.control.source_unit()
                                && statement != self.control
                                && statement != self.body
                                && !self.slots.iter().any(|slot| slot.root == binding)
                                && receipts[..i].iter().all(|(_, prior)| {
                                    prior.target() != fact.target() && prior.point() != fact.point()
                                })
                        })
                })
            })
    }
    pub(in crate::ownership_checking::compilation_unit) fn with_condition_false_partition(
        mut self,
        live: Vec<UnitSymbolId>,
        drops: Vec<UnitDropFact>,
    ) -> Option<Self> {
        if self.exit_partition.is_some() {
            return None;
        }
        self.exit_partition = Some((live, drops));
        self.manifest.exit_partition = self.exit_partition.clone();
        self.is_consistent().then_some(self)
    }
    pub(super) fn condition_false_partition_parts(
        &self,
    ) -> Option<(&[UnitSymbolId], &[UnitDropFact])> {
        self.exit_partition
            .as_ref()
            .map(|(live, drops)| (live.as_slice(), drops.as_slice()))
    }
    fn valid_exit_partition(&self) -> bool {
        self.exit_partition == self.manifest.exit_partition
            && self.exit_partition.as_ref().is_none_or(|(live, drops)| {
                self.normal_transport.is_some()
                    && live.len() + drops.len() == self.slots.len()
                    && live.iter().enumerate().all(|(i, root)| {
                        !live[..i].contains(root)
                            && self.slots.iter().any(|slot| slot.root == *root)
                    })
                    && self.slots.iter().all(|slot| {
                        usize::from(live.contains(&slot.root))
                            + drops
                                .iter()
                                .filter(|fact| {
                                    fact.target() == UnitDropTarget::Named(slot.root)
                                        && fact.point() == UnitDropPoint::LoopExit(self.control)
                                })
                                .count()
                            == 1
                    })
            })
    }
    pub(in crate::ownership_checking::compilation_unit) fn with_condition_false_drops(
        mut self,
        drops: Vec<UnitDropFact>,
    ) -> Option<Self> {
        if self.exit_drops.is_some() {
            return None;
        }
        self.manifest.exit_drops = Some(drops.clone());
        self.exit_drops = Some(drops);
        self.is_consistent().then_some(self)
    }
    pub(super) fn condition_false_drops(&self) -> Option<&[UnitDropFact]> {
        self.exit_drops.as_deref()
    }
    fn valid_exit_drops(&self) -> bool {
        self.exit_drops == self.manifest.exit_drops
            && self.exit_drops.as_ref().is_none_or(|drops| {
                self.normal_transport.is_some()
                    && self
                        .exit_partition
                        .as_ref()
                        .is_none_or(|(live, partition_drops)| {
                            live.is_empty() && partition_drops == drops
                        })
                    && drops.len() == self.slots.len()
                    && self.slots.iter().all(|slot| {
                        drops
                            .iter()
                            .filter(|fact| {
                                fact.target() == UnitDropTarget::Named(slot.root)
                                    && fact.point() == UnitDropPoint::LoopExit(self.control)
                            })
                            .count()
                            == 1
                    })
            })
    }
    pub(super) fn normal_transport(&self) -> Option<&UnitOwnerMultiRootNormalTransportV1> {
        self.normal_transport.as_ref()
    }
    fn matches_normal_transport(&self) -> bool {
        if self.normal_transport.is_none() {
            return self.manifest.normal_steps.is_none();
        }
        self.normal_transport.as_ref().is_some_and(|rule| {
            self.manifest.normal_steps.as_deref() == Some(rule.steps())
                && rule.is_consistent()
                && rule.statement() == self.control
                && rule.header_roots().len() == self.slots.len()
                && rule
                    .header_roots()
                    .iter()
                    .zip(&self.slots)
                    .all(|(root, slot)| *root == slot.root)
                && rule
                    .normal_outputs()
                    .iter()
                    .zip(&self.slots)
                    .all(|((root, owner), slot)| *root == slot.root && *owner == slot.normal)
                && rule.steps().iter().all(|step| {
                    self.slots.iter().any(|slot| {
                        slot.root == step.header_root()
                            && slot.normal == step.producer()
                            && slot.retirement == step.returned_value_drop()
                    })
                })
        })
    }

    /// Actual While control statement.
    pub fn statement(&self) -> UnitStatementId {
        self.control
    }
    /// Actual While condition expression.
    pub fn condition(&self) -> UnitExpressionId {
        self.condition
    }
    /// Actual While body statement.
    pub fn body(&self) -> UnitStatementId {
        self.body
    }
    /// Resolved Boolean Name binding.
    pub fn guard(&self) -> UnitSymbolId {
        self.guard
    }
    /// Complete original catalog, including definitions no longer live.
    pub fn input_catalog(&self) -> &UnitOwnerProducerCatalogV1 {
        &self.inputs
    }
    /// Actual first-body producer catalog.
    pub fn body_catalog(&self) -> &UnitOwnerProducerCatalogV1 {
        &self.producers
    }
    /// Slots retain entry-root order; they are not independently selectable paths.
    pub fn slots(&self) -> &[UnitOwnerRootBoundarySlotV1] {
        &self.slots
    }
    /// Structural/catalog verification, not an induction or runtime trace validator.
    pub fn is_consistent(&self) -> bool {
        let source = self.control.source_unit();
        self.guard_reset == self.manifest.guard_reset && self.guard_reset.is_none_or(|id| id.source_unit() == self.control.source_unit()) && self.valid_suffix() && self.valid_exit_partition() && self.valid_exit_drops() && self.matches_normal_transport() && self.manifest.sites == (self.control, self.condition, self.body, self.guard)
            && self.manifest.slots == self.slots
            && self.manifest.inputs == self.inputs.definitions()
            && self.manifest.producers == self.producers.definitions()
            && self.slots.len() >= 2
            && self.inputs.is_consistent() && self.producers.is_consistent()
            && self.inputs.callable() == self.producers.callable()
            && self.producers.definitions().len() == self.slots.len()
            && !std::sync::Arc::ptr_eq(&self.inputs.identity, &self.producers.identity)
            && self.condition.source_unit() == source && self.body.source_unit() == source
            && self.guard.source_unit() == source && self.body != self.control
            && self.slots.iter().enumerate().all(|(index, slot)| {
                slot.root.source_unit() == source && slot.root != self.guard
                    && slot.producer.source_unit() == source
                    && self.inputs.definition(slot.entry.clone()).is_some()
                    && self.producers.definition(slot.normal.clone()).is_some_and(|definition| definition.origin == UnitOwnerOrigin::String(slot.producer))
                    && matches!(slot.retirement.target(), UnitDropTarget::Named(binding) if binding.source_unit() == source && !self.slots.iter().any(|slot| slot.root == binding))
                    && matches!(slot.retirement.point(), UnitDropPoint::AfterStatement(statement) if statement.source_unit() == source && statement != self.control && statement != self.body)
                    && self.slots[..index].iter().all(|prior| prior.root != slot.root && prior.entry != slot.entry && prior.normal != slot.normal && prior.retirement.target() != slot.retirement.target())
            })
    }
}
impl UnitOwnerObservation {
    /// Separate boundary evidence; never grants a header/loop lowering capability.
    pub fn multi_root_boundary_v1(&self) -> Option<&UnitOwnerMultiRootBoundaryV1> {
        let boundary = self.multi_root_boundary.as_deref()?;
        (self.closed_loop_batch.is_none()
            && self.control_bridge.is_none()
            && self.counter_batch.is_none()
            && self.loop_value.is_none()
            && self.invariant_loop_value.is_none()
            && self.open_loop_value.is_none()
            && self.choice_value.is_none()
            && self.definitions.is_empty()
            && self.events.is_empty()
            && matches!(self.status, super::super::UnitOwnerObservationStatus::Unsupported {
                point: UnitDropPoint::AfterStatement(control) | UnitDropPoint::LoopExit(control),
                reason: super::super::UnitOwnerUnsupportedReason::Syntax,
            } if control == boundary.control)
            && boundary.inputs.callable() == self.callable
            && std::sync::Arc::ptr_eq(&boundary.inputs.identity, &self.identity)
            && boundary.is_consistent())
        .then_some(boundary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    const SOURCE: &str = "fun run(flag: Boolean) { val earlier = \"retired\"; var a = \"a\"; var b = \"b\"; while(flag) { val x = replace(&a, \"x\"); val y = replace(&b, \"y\") }; val keptA = a; val keptB = b }";

    #[test]
    fn real_multi_root_boundary_publishes_without_loop_capability() {
        let fixture = Fixture::new(&[("public-root-boundary.ko", SOURCE)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty());
        assert!(owned.clone().validate().is_ok());
        let observation = &owned.owner_observations()[0];
        let boundary = observation.multi_root_boundary_v1().expect("boundary only");
        assert!(boundary.is_consistent());
        assert!(
            observation
                .multi_root_normal_transport_v1()
                .unwrap()
                .is_consistent()
        );
        assert_eq!(boundary.slots().len(), 2);
        assert_eq!(boundary.input_catalog().definitions().len(), 3);
        assert_eq!(boundary.body_catalog().definitions().len(), 2);
        for slot in boundary.slots() {
            assert!(boundary.input_catalog().definition(slot.entry()).is_some());
            assert_eq!(
                boundary
                    .body_catalog()
                    .definition(slot.first_body_output())
                    .unwrap()
                    .origin(),
                UnitOwnerOrigin::String(slot.producer())
            );
            assert_ne!(slot.entry(), slot.first_body_output());
        }
        assert!(observation.value_observation_v1().is_none());
        assert!(observation.loop_observation_v1().is_none());
        assert!(observation.closed_loop_batch_v1().is_none());
        assert!(observation.invariant_loop_observation_v1().is_none());
        assert!(observation.open_loop_observation_v1().is_none());
    }

    #[test]
    fn ci_only_multi_root_boundary_rejects_damage_and_retracts_publication() {
        let fixture = Fixture::new(&[("public-root-boundary-fault.ko", SOURCE)]);
        let owned = fixture.shared();
        let observation = &owned.owner_observations()[0];
        let boundary = observation.multi_root_boundary_v1().unwrap();
        let mut partial = boundary.clone();
        partial.slots.pop();
        assert!(!partial.is_consistent());
        let mut duplicate = boundary.clone();
        duplicate.slots[1] = duplicate.slots[0].clone();
        assert!(!duplicate.is_consistent());
        let mut wrong_drop = boundary.clone();
        let saved = wrong_drop.slots[0].retirement;
        wrong_drop.slots[0].retirement = wrong_drop.slots[1].retirement;
        wrong_drop.slots[1].retirement = saved;
        assert!(!wrong_drop.is_consistent());
        let mut wrong_entry = boundary.clone();
        let saved = wrong_entry.slots[0].entry.clone();
        wrong_entry.slots[0].entry = wrong_entry.slots[1].entry.clone();
        wrong_entry.slots[1].entry = saved;
        assert!(!wrong_entry.is_consistent());
        let mut wrong_sites = boundary.clone();
        wrong_sites.condition = wrong_sites.slots[0].producer;
        assert!(!wrong_sites.is_consistent());
        let mut wrong_body = boundary.clone();
        wrong_body.body = match wrong_body.slots[0].retirement.point() {
            UnitDropPoint::AfterStatement(statement) => statement,
            _ => panic!("drop"),
        };
        assert!(!wrong_body.is_consistent());
        let mut wrong_guard = boundary.clone();
        wrong_guard.guard = match wrong_guard.slots[0].retirement.target() {
            UnitDropTarget::Named(binding) => binding,
            _ => panic!("binding"),
        };
        assert!(!wrong_guard.is_consistent());
        let actual_rule = boundary.normal_transport().unwrap();
        let mut reordered_steps = actual_rule.steps().to_vec();
        reordered_steps.reverse();
        let reordered_rule = UnitOwnerMultiRootNormalTransportV1::seal(
            boundary.control,
            actual_rule.header_roots().to_vec(),
            boundary.producers.clone(),
            reordered_steps,
        )
        .unwrap();
        let mut reordered_attachment = boundary.clone();
        reordered_attachment.normal_transport = Some(reordered_rule);
        assert!(!reordered_attachment.is_consistent());
        let mut wrong_calls = actual_rule.steps().to_vec();
        wrong_calls[0].call = boundary.condition;
        let wrong_rule = UnitOwnerMultiRootNormalTransportV1::seal(
            boundary.control,
            actual_rule.header_roots().to_vec(),
            boundary.producers.clone(),
            wrong_calls,
        )
        .unwrap();
        let mut wrong_attachment = boundary.clone();
        wrong_attachment.normal_transport = Some(wrong_rule);
        assert!(!wrong_attachment.is_consistent());
        let reanalyzed = fixture.shared();
        let other = &reanalyzed.owner_observations()[0];
        let mut foreign = observation.clone();
        foreign.multi_root_boundary = other.multi_root_boundary.clone();
        assert!(foreign.multi_root_boundary_v1().is_none());
        let simple_fixture = Fixture::new(&[(
            "plain-for-collision.ko",
            "fun plain() { val value = \"x\" }",
        )]);
        let simple = simple_fixture.shared();
        let mut collision = simple.owner_observations()[0].clone();
        assert!(collision.value_observation_v1().is_some());
        collision.multi_root_boundary = observation.multi_root_boundary.clone();
        assert!(collision.value_observation_v1().is_none());
        assert!(collision.multi_root_boundary_v1().is_none());
        let mut mixed = boundary.clone();
        mixed.producers = other.multi_root_boundary_v1().unwrap().producers.clone();
        assert!(!mixed.is_consistent());
        let deferred =
            super::super::super::publish_observations(vec![observation.clone()], true, false);
        assert!(deferred[0].multi_root_boundary_v1().is_none());
        assert!(deferred[0].multi_root_normal_transport_v1().is_none());
        assert!(
            super::super::super::publish_observations(vec![observation.clone()], false, true)
                .is_empty()
        );
    }
}

#[cfg(test)]
mod exit_fault_tests {
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    #[test]
    fn ci_only_false_exit_rejects_missing_repeated_or_reordered_cleanup() {
        let fixture = Fixture::new(&[(
            "exit-fault.ko",
            "fun run(flag: Boolean) { var a = \"a\"; var b = \"b\"; while(flag) { val x = replace(&a, \"x\"); val y = replace(&b, \"y\") } }",
        )]);
        let owned = fixture.shared();
        let boundary = owned.owner_observations()[0]
            .multi_root_boundary_v1()
            .unwrap();
        assert!(boundary.exit_drops.is_some());
        let mut contradictory = boundary.clone();
        let root = contradictory.slots[0].root;
        let (live, drops) = contradictory.exit_partition.as_mut().unwrap();
        live.push(root);
        drops
            .retain(|fact| fact.target() != crate::ownership_checking::UnitDropTarget::Named(root));
        contradictory.manifest.exit_partition = contradictory.exit_partition.clone();
        assert!(!contradictory.is_consistent());

        let mut overlap = boundary.clone();
        let root = overlap.slots[0].root;
        overlap.exit_partition.as_mut().unwrap().0.push(root);
        assert!(!overlap.is_consistent());
        let mut missing_partition = boundary.clone();
        missing_partition.exit_partition.as_mut().unwrap().1.pop();
        assert!(!missing_partition.is_consistent());

        let mut missing = boundary.clone();
        missing.exit_drops.as_mut().unwrap().pop();
        assert!(!missing.is_consistent());
        let mut repeated = boundary.clone();
        let drops = repeated.exit_drops.as_mut().unwrap();
        drops[1] = drops[0];
        assert!(!repeated.is_consistent());
        let mut reordered = boundary.clone();
        reordered.exit_drops.as_mut().unwrap().reverse();
        assert!(!reordered.is_consistent());
    }
    #[test]
    fn ci_only_guard_reset_manifest_cannot_be_removed_or_relabelled() {
        let fixture = Fixture::new(&[(
            "guard-reset-manifest.ko",
            "fun run(flag: Boolean) { var again = flag; var a = \"a\"; var b = \"b\"; while(again) { val x = replace(&a, \"x\"); val y = replace(&b, \"y\"); again = false } }",
        )]);
        let owned = fixture.shared();
        let boundary = owned.owner_observations()[0]
            .multi_root_boundary_v1()
            .unwrap();
        assert!(boundary.guard_reset.is_some());
        assert!(boundary.clone().with_guard_reset(None).is_none());
        let mut missing = boundary.clone();
        missing.guard_reset = None;
        assert!(!missing.is_consistent());
        let mut relabelled = boundary.clone();
        relabelled.guard_reset = Some(boundary.condition);
        assert!(!relabelled.is_consistent());
        let mut manifest = boundary.clone();
        manifest.manifest.guard_reset = None;
        assert!(!manifest.is_consistent());
    }
}
