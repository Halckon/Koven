//! Non-authorizing scalar-tail audit evidence. No owner/header capability is exposed.
use super::*;
use crate::name_resolution::UnitSymbolId;
use crate::ownership_checking::UnitOwnerProducerCatalogV1;

/// Actual completed source identities for a bounded countdown audit, not permission.
#[derive(Clone, Debug)]
pub struct UnitOwnerCounterTailEvidenceV1 {
    pub(in crate::ownership_checking::compilation_unit) analysis: std::sync::Arc<()>,
    pub(in crate::ownership_checking::compilation_unit) callable: UnitOwnerCallable,
    pub(in crate::ownership_checking::compilation_unit) control: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) condition: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) body: UnitStatementId,
    pub(in crate::ownership_checking::compilation_unit) guard: UnitSymbolId,
    pub(in crate::ownership_checking::compilation_unit) counter: UnitSymbolId,
    pub(in crate::ownership_checking::compilation_unit) initializer: UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit) assignments: [UnitExpressionId; 4],
    pub(in crate::ownership_checking::compilation_unit) steps: Vec<UnitOwnerRootReplaceStepV1>,
    pub(in crate::ownership_checking::compilation_unit) producers: UnitOwnerProducerCatalogV1,
    pub(in crate::ownership_checking::compilation_unit) roots: UnitOwnerMultiRootBoundaryV1,
}
impl UnitOwnerCounterTailEvidenceV1 {
    /// Source-qualified callable owning this evidence.
    pub fn callable(&self) -> UnitOwnerCallable {
        self.callable
    }
    /// Exact while statement.
    pub fn statement(&self) -> UnitStatementId {
        self.control
    }
    /// Actual Boolean Name loop condition.
    pub fn condition(&self) -> UnitExpressionId {
        self.condition
    }
    /// Exact source body.
    pub fn body(&self) -> UnitStatementId {
        self.body
    }
    /// Mutable Boolean guard binding.
    pub fn guard(&self) -> UnitSymbolId {
        self.guard
    }
    /// Mutable Int counter binding.
    pub fn counter(&self) -> UnitSymbolId {
        self.counter
    }
    /// Pure initial counter comparison source.
    pub fn initializer(&self) -> UnitExpressionId {
        self.initializer
    }
    /// Decrement assignment, arithmetic RHS, guard assignment, comparison RHS.
    pub fn assignments(&self) -> [UnitExpressionId; 4] {
        self.assignments
    }
    /// Actual ordered root Replace and returned-value retirement facts.
    pub fn steps(&self) -> &[UnitOwnerRootReplaceStepV1] {
        &self.steps
    }
    /// Original body producer catalog; not a header permission.
    pub fn body_catalog(&self) -> &UnitOwnerProducerCatalogV1 {
        &self.producers
    }
    /// Original entry tokens, not loop activation permissions.
    pub fn input_catalog(&self) -> &UnitOwnerProducerCatalogV1 {
        self.roots.input_catalog()
    }
    /// Whole ordered static entry/first-body vector; not subsequent runtime generations.
    pub fn root_slots(&self) -> &[UnitOwnerRootBoundarySlotV1] {
        self.roots.slots()
    }
    /// Actual condition-false live/retired partition, if completely observed.
    pub fn false_exit_partition(
        &self,
    ) -> Option<(&[UnitSymbolId], &[crate::ownership_checking::UnitDropFact])> {
        self.roots.condition_false_partition_parts()
    }
    /// Actual completed live-root suffix retirement facts, not exit permission.
    pub fn false_exit_suffix(
        &self,
    ) -> Option<&[(UnitSymbolId, crate::ownership_checking::UnitDropFact)]> {
        self.roots.condition_false_suffix()
    }
    /// Structural source/catalog checks, not dynamic reachability or induction.
    pub fn is_consistent(&self) -> bool {
        let source = self.control.source_unit();
        self.roots.is_consistent()
            && self.roots.statement() == self.control
            && self.roots.condition() == self.condition
            && self.roots.body() == self.body
            && self.roots.guard() == self.guard
            && self.roots.guard_reset().is_none()
            && std::sync::Arc::ptr_eq(&self.analysis, &self.input_catalog().identity)
            && self.roots.body_catalog().definitions() == self.producers.definitions()
            && self
                .roots
                .normal_transport()
                .is_some_and(|rule| rule.steps() == self.steps)
            && self.guard != self.counter
            && self.guard.source_unit() == source
            && self.counter.source_unit() == source
            && self.condition.source_unit() == source
            && self.body.source_unit() == source
            && self.initializer.source_unit() == source
            && self
                .assignments
                .iter()
                .enumerate()
                .all(|(i, id)| id.source_unit() == source && !self.assignments[..i].contains(id))
            && self.producers.is_consistent()
            && self.producers.callable() == self.callable
            && self.steps.len() >= 2
            && self.steps.iter().enumerate().all(|(i, step)| {
                step.header_root() != self.guard
                    && step.header_root() != self.counter
                    && step.header_root().source_unit() == source
                    && step.call().source_unit() == source
                    && self.producers.definition(step.producer()).is_some()
                    && !self.steps[..i]
                        .iter()
                        .any(|known| known.header_root() == step.header_root())
            })
    }
}
impl UnitOwnerObservation {
    /// Additional audit input only; does not make Unsupported observations Complete.
    pub fn counter_tail_evidence_v1(&self) -> Option<&UnitOwnerCounterTailEvidenceV1> {
        let value = self.counter_tail.as_deref()?;
        (std::sync::Arc::ptr_eq(&value.analysis, &self.identity)
            && self.definitions.is_empty() && self.events.is_empty()
            && matches!(self.status, crate::ownership_checking::UnitOwnerObservationStatus::Unsupported {point: UnitDropPoint::AfterStatement(control) | UnitDropPoint::LoopExit(control), reason: crate::ownership_checking::UnitOwnerUnsupportedReason::Syntax} if control == value.control)
            && value.callable == self.callable
            && value.is_consistent()
            && self.multi_root_boundary.is_none()
            && self.loop_value.is_none()
            && self.closed_loop_batch.is_none() && self.control_bridge.is_none() && self.counter_batch.is_none()
            && self.open_loop_value.is_none()
            && self.invariant_loop_value.is_none()
            && self.choice_value.is_none())
        .then_some(value)
    }
}

#[cfg(test)]
mod tests {
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    #[test]
    fn counter_audit_preserves_whole_entry_output_and_exit_partition() {
        for roots in [2, 3] {
            for live in [0, 1, roots] {
                let prefix = (0..roots)
                    .map(|i| format!("var r{i} = \"entry{i}\";"))
                    .collect::<String>();
                let body = (0..roots)
                    .rev()
                    .map(|i| format!("val old{i} = replace(&r{i}, \"body{i}\");"))
                    .collect::<String>();
                let suffix = (0..live)
                    .rev()
                    .map(|i| format!("val end{i} = r{i};"))
                    .collect::<String>();
                let text = format!(
                    "fun run(n: Int) {{ var left = n; var again = left > 0; {prefix} while(again) {{ {body} left = left - 1; again = left > 0; }}; {suffix} }}"
                );
                let fixture = Fixture::new(&[("counter-vector.ko", &text)]);
                let owned = fixture.shared();
                let observation = owned
                    .owner_observations()
                    .iter()
                    .find(|o| o.counter_tail_evidence_v1().is_some())
                    .unwrap();
                assert!(observation.multi_root_boundary_v1().is_none());
                let audit = observation.counter_tail_evidence_v1().unwrap();
                assert_eq!(audit.root_slots().len(), roots);
                for slot in audit.root_slots() {
                    assert_ne!(slot.entry(), slot.first_body_output());
                    assert!(audit.input_catalog().definition(slot.entry()).is_some());
                    assert!(
                        audit
                            .body_catalog()
                            .definition(slot.first_body_output())
                            .is_some()
                    );
                    let step = audit
                        .steps()
                        .iter()
                        .find(|s| s.header_root() == slot.root())
                        .unwrap();
                    assert_eq!(step.returned_value_drop(), slot.returned_value_drop());
                }
                let (actual_live, drops) = audit.false_exit_partition().unwrap();
                assert_eq!(actual_live.len(), live);
                assert_eq!(drops.len(), roots - live);
                if live == 0 {
                    assert!(audit.false_exit_suffix().is_none());
                } else {
                    let suffix = audit.false_exit_suffix().unwrap();
                    assert_eq!(suffix.len(), live);
                    assert!(suffix.iter().all(|(root, _)| actual_live.contains(root)));
                }
            }
        }
    }

    #[test]
    fn ci_only_counter_audit_rejects_deferred_foreign_and_complete_observations() {
        let fixture = Fixture::new(&[(
            "counter-evidence.ko",
            "fun run(n: Int) { var left = n; var again = left > 0; var a = \"a\"; var b = \"b\"; while(again) { val x = replace(&a, \"x\"); val y = replace(&b, \"y\"); left = left - 1; again = left > 0 } }",
        )]);
        let owned = fixture.shared();
        let original = owned
            .owner_observations()
            .iter()
            .find(|o| o.counter_tail_evidence_v1().is_some())
            .unwrap();
        let mut altered = original.clone();
        altered.counter_tail.as_mut().unwrap().steps.reverse();
        assert!(altered.counter_tail_evidence_v1().is_none());
        let mut mismatched = original.clone();
        mismatched.counter_tail.as_mut().unwrap().roots = fixture
            .shared()
            .owner_observations()
            .iter()
            .find_map(|o| o.counter_tail_evidence_v1())
            .unwrap()
            .roots
            .clone();
        assert!(mismatched.counter_tail_evidence_v1().is_none());
        let deferred = super::super::publish_observations(vec![original.clone()], true, false);
        assert!(deferred[0].counter_tail_evidence_v1().is_none());
        let other = fixture.shared();
        let mut foreign = original.clone();
        foreign.counter_tail = other
            .owner_observations()
            .iter()
            .find(|o| o.counter_tail_evidence_v1().is_some())
            .unwrap()
            .counter_tail
            .clone();
        assert!(foreign.counter_tail_evidence_v1().is_none());
        let mut complete = original.clone();
        complete.status = crate::ownership_checking::UnitOwnerObservationStatus::Complete;
        assert!(complete.counter_tail_evidence_v1().is_none());
    }
}
