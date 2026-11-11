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
    /// Structural source/catalog checks, not dynamic reachability or induction.
    pub fn is_consistent(&self) -> bool {
        let source = self.control.source_unit();
        self.guard != self.counter
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
            && self.closed_loop_batch.is_none()
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
