//! Atomic closed counter audit group. Member namespaces remain distinct.
use super::*;
use std::sync::Arc;

/// Multiple independently collected closed counter contracts, not runtime induction or lowering permission.
#[derive(Clone, Debug)]
pub struct UnitOwnerCounterBatchEvidenceV1 {
    identity: Arc<()>,
    controls: Vec<UnitStatementId>,
    inputs: Vec<Arc<()>>,
    members: Vec<UnitOwnerCounterTailEvidenceV1>,
}
impl UnitOwnerCounterBatchEvidenceV1 {
    pub(in crate::ownership_checking::compilation_unit) fn seal(
        identity: Arc<()>,
        controls: Vec<UnitStatementId>,
        inputs: Vec<Arc<()>>,
        members: Vec<UnitOwnerCounterTailEvidenceV1>,
    ) -> Option<Self> {
        let value = Self {
            identity,
            controls,
            inputs,
            members,
        };
        value.is_consistent().then_some(value)
    }
    /// All members in source-control order. No projection to the single-counter getter.
    pub fn members(&self) -> &[UnitOwnerCounterTailEvidenceV1] {
        &self.members
    }
    /// Verify exact expected identities, complete dead-root exits and disjoint members.
    pub fn is_consistent(&self) -> bool {
        self.members.len() >= 2
            && self.controls.len() == self.members.len()
            && self.inputs.len() == self.members.len()
            && Arc::ptr_eq(&self.identity, &self.inputs[0])
            && self.members.iter().enumerate().all(|(index, member)| {
                !self.controls[..index].contains(&self.controls[index])
                    && self.inputs[..index]
                        .iter()
                        .all(|prior| !Arc::ptr_eq(prior, &self.inputs[index]))
                    && member.statement() == self.controls[index]
                    && Arc::ptr_eq(&member.analysis, &self.inputs[index])
                    && member.is_consistent()
                    && member.false_exit_partition().is_some_and(|(live, drops)| {
                        live.is_empty() && drops.len() == member.root_slots().len()
                    })
                    && self.members[..index].iter().all(|prior| {
                        prior.callable() == member.callable()
                            && prior.body() != member.body()
                            && !Arc::ptr_eq(&prior.producers.identity, &member.producers.identity)
                            && prior
                                .root_slots()
                                .iter()
                                .all(|a| member.root_slots().iter().all(|b| a.root() != b.root()))
                    })
            })
    }
}
impl UnitOwnerObservation {
    /// Atomic non-authorizing counter audit group; unsupported legacy state is retained.
    pub fn counter_batch_evidence_v1(&self) -> Option<&UnitOwnerCounterBatchEvidenceV1> {
        let batch = self.counter_batch.as_deref()?;
        (self.private_control_chain.is_none()
            && self.nested_normal.is_none()
            && self.control_bridge.is_none()
            && batch.is_consistent()
            && self.counter_tail.is_none()
            && self.closed_loop_batch.is_none()
            && self.multi_root_boundary.is_none()
            && self.loop_value.is_none()
            && self.invariant_loop_value.is_none()
            && self.open_loop_value.is_none()
            && self.choice_value.is_none()
            && self.definitions.is_empty()
            && self.events.is_empty()
            && Arc::ptr_eq(&self.identity, &batch.identity)
            && batch
                .members
                .first()
                .is_some_and(|m| m.callable() == self.callable)
            && matches!(self.status, super::super::UnitOwnerObservationStatus::Unsupported {
                point: UnitDropPoint::AfterStatement(control) | UnitDropPoint::LoopExit(control),
                reason: super::super::UnitOwnerUnsupportedReason::Syntax,
            } if control == batch.controls[0]))
        .then_some(batch)
    }
}

#[cfg(test)]
mod tests {
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    fn source() -> String {
        let segment = |i| {
            format!(
                r#"var n{i} = n; var g{i} = n{i} > 0; var a{i} = "a"; var b{i} = "b";
            while(g{i}) {{ val x = replace(&a{i}, "x"); val y = replace(&b{i}, "y"); n{i} = n{i} - 1; g{i} = n{i} > 0 }};"#
            )
        };
        format!("fun run(n: Int) {{ {} {} }}", segment(0), segment(1))
    }
    #[test]
    fn closed_counter_batch_does_not_project_single_member_permissions() {
        let source = source();
        let fixture = Fixture::new(&[("counter-batch.ko", &source)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty());
        let observation = &owned.owner_observations()[0];
        let batch = observation.counter_batch_evidence_v1().unwrap();
        assert!(batch.is_consistent());
        assert_eq!(batch.members().len(), 2);
        assert!(observation.counter_tail_evidence_v1().is_none());
        assert!(observation.multi_root_boundary_v1().is_none());
        assert!(observation.closed_loop_batch_v1().is_none());
        assert!(observation.loop_observation_v1().is_none());
    }
    #[test]
    fn longer_closed_counter_groups_publish_every_member_or_none() {
        for count in [3, 4] {
            let segments = (0..count).map(|i| format!(r#"var n{i} = n; var g{i} = n{i} > 0; var a{i} = "a"; var b{i} = "b";
                while(g{i}) {{ val x = replace(&a{i}, "x"); val y = replace(&b{i}, "y"); n{i} = n{i} - 1; g{i} = n{i} > 0 }};"#)).collect::<Vec<_>>().join(" ");
            let text = format!("fun run(n: Int) {{ {segments} }}");
            for broken in [false, true] {
                let source = if broken {
                    text.replace("replace(&b1,", "replace(&a1,")
                } else {
                    text.clone()
                };
                let fixture = Fixture::new(&[("long-counter-batch.ko", &source)]);
                let owned = fixture.shared();
                assert!(owned.diagnostics().is_empty());
                let observation = &owned.owner_observations()[0];
                assert!(observation.counter_tail_evidence_v1().is_none());
                if broken {
                    assert!(observation.counter_batch_evidence_v1().is_none());
                } else {
                    let batch = observation.counter_batch_evidence_v1().unwrap();
                    assert!(batch.is_consistent());
                    assert_eq!(batch.members().len(), count);
                }
            }
        }
    }
    // Local compilation only. Remote CI executes mutations of sealed products.
    #[test]
    fn ci_only_counter_batch_rejects_partial_reordered_foreign_and_colliding_members() {
        let source = source();
        let fixture = Fixture::new(&[("counter-batch-fault.ko", &source)]);
        let owned = fixture.shared();
        let observation = &owned.owner_observations()[0];
        let batch = observation.counter_batch_evidence_v1().unwrap();
        let mut empty_controls = observation.clone();
        empty_controls
            .counter_batch
            .as_mut()
            .unwrap()
            .controls
            .clear();
        assert!(empty_controls.counter_batch_evidence_v1().is_none());
        let mut missing_control = batch.clone();
        missing_control.controls.pop();
        assert!(!missing_control.is_consistent());
        let mut missing_input = batch.clone();
        missing_input.inputs.pop();
        assert!(!missing_input.is_consistent());
        let mut partial = batch.clone();
        partial.members.pop();
        assert!(!partial.is_consistent());
        let mut reordered = batch.clone();
        reordered.members.reverse();
        assert!(!reordered.is_consistent());
        let mut duplicate = batch.clone();
        duplicate.members[1] = duplicate.members[0].clone();
        assert!(!duplicate.is_consistent());
        let other = fixture.shared();
        let mut mixed = batch.clone();
        mixed.members[1] = other.owner_observations()[0]
            .counter_batch_evidence_v1()
            .unwrap()
            .members[1]
            .clone();
        assert!(!mixed.is_consistent());
        let mut foreign = observation.clone();
        foreign.counter_batch = other.owner_observations()[0].counter_batch.clone();
        assert!(foreign.counter_batch_evidence_v1().is_none());
        let mut collision = observation.clone();
        collision.counter_tail = Some(Box::new(batch.members[0].clone()));
        assert!(collision.counter_batch_evidence_v1().is_none());
        assert!(collision.counter_tail_evidence_v1().is_none());
        let mut complete = observation.clone();
        complete.status = crate::ownership_checking::UnitOwnerObservationStatus::Complete;
        assert!(complete.counter_batch_evidence_v1().is_none());
        assert!(complete.value_observation_v1().is_none());
        let choices = Fixture::new(&[(
            "choice-batch-fault.ko",
            "fun choose(flag: Boolean) { val result = if(flag) \"left\" else (\"right\") }",
        )]);
        let choices_owned = choices.shared();
        let mut choice = choices_owned
            .owner_observations()
            .iter()
            .find(|o| o.choice_value.is_some())
            .unwrap()
            .clone();
        assert!(choice.value_observation_v1().is_some());
        choice.counter_batch = Some(Box::new(batch.clone()));
        assert!(choice.counter_batch_evidence_v1().is_none());
        assert!(choice.value_observation_v1().is_none());
        let deferred = super::super::publish_observations(vec![observation.clone()], true, false);
        assert!(deferred[0].counter_batch_evidence_v1().is_none());
    }
}
