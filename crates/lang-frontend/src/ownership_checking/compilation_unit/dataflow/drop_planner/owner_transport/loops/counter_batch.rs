//! Freeze each member at its real closed exit; publish the entire group or none.
use super::*;
use crate::ownership_checking::{UnitOwnerCounterBatchEvidenceV1, UnitOwnerCounterTailEvidenceV1};
use std::sync::Arc;

pub(super) struct Batch {
    identity: Arc<()>,
    controls: [UnitStatementId; 2],
    inputs: [Arc<()>; 2],
    members: Vec<UnitOwnerCounterTailEvidenceV1>,
    failed: bool,
}
impl Batch {
    pub(super) fn new(identity: Arc<()>, controls: [UnitStatementId; 2], second: Arc<()>) -> Self {
        Self {
            inputs: [identity.clone(), second],
            identity,
            controls,
            members: Vec::new(),
            failed: false,
        }
    }
    fn complete(
        &mut self,
        control: UnitStatementId,
        evidence: Option<UnitOwnerCounterTailEvidenceV1>,
    ) {
        if self.failed {
            return;
        }
        let valid = self.controls.get(self.members.len()) == Some(&control)
            && evidence.as_ref().is_some_and(|e| {
                e.statement() == control
                    && e.is_consistent()
                    && Arc::ptr_eq(&e.analysis, &self.inputs[self.members.len()])
                    && e.false_exit_partition().is_some_and(|(live, drops)| {
                        live.is_empty() && drops.len() == e.root_slots().len()
                    })
            });
        if !valid {
            self.failed = true;
            self.members.clear();
        } else if let Some(evidence) = evidence {
            self.members.push(evidence);
        }
    }
    fn seal(self) -> Option<UnitOwnerCounterBatchEvidenceV1> {
        if self.failed {
            return None;
        }
        UnitOwnerCounterBatchEvidenceV1::seal(
            self.identity,
            self.controls,
            self.inputs,
            self.members,
        )
    }
}
impl ControlTransactions {
    pub(in super::super::super) fn complete_counter_member(
        &mut self,
        control: UnitStatementId,
        state: &ValueState,
        continued: bool,
    ) {
        if self.counter_batch.is_none() {
            return;
        }
        let evidence = (continued && !self.root_branches_invalid && empty_boundary(state))
            .then(|| self.private_counter_evidence(control))
            .flatten();
        if let Some(batch) = &mut self.counter_batch {
            batch.complete(control, evidence);
        }
    }
    pub(in super::super::super) fn take_counter_batch(
        &mut self,
    ) -> Option<UnitOwnerCounterBatchEvidenceV1> {
        if self.root_branches_invalid {
            self.counter_batch = None;
        }
        self.counter_batch.take()?.seal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    // Only remote CI executes injected collector completion order failures.
    #[test]
    fn ci_only_counter_batch_completion_failure_is_sticky() {
        let segment = |i| {
            format!(
                r#"var n{i} = n; var g{i} = n{i} > 0; var a{i} = "a"; var b{i} = "b";
            while(g{i}) {{ val x = replace(&a{i}, "x"); val y = replace(&b{i}, "y"); n{i} = n{i} - 1; g{i} = n{i} > 0 }};"#
            )
        };
        let source = format!("fun run(n: Int) {{ {} {} }}", segment(0), segment(1));
        let fixture = Fixture::new(&[("counter-batch-order-fault.ko", &source)]);
        let owned = fixture.shared();
        let members = owned.owner_observations()[0]
            .counter_batch_evidence_v1()
            .unwrap()
            .members();
        let controls = [members[0].statement(), members[1].statement()];
        for mode in 0..3 {
            let mut batch = Batch::new(
                members[0].analysis.clone(),
                controls,
                members[1].analysis.clone(),
            );
            batch.complete(controls[0], Some(members[0].clone()));
            match mode {
                0 => batch.complete(controls[0], Some(members[0].clone())),
                1 => batch.complete(controls[1], None),
                _ => {
                    assert!(batch.seal().is_none());
                    continue;
                }
            }
            batch.complete(controls[1], Some(members[1].clone()));
            assert!(batch.failed);
            assert!(batch.members.is_empty());
            assert!(batch.seal().is_none());
        }
    }
}
