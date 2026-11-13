//! Completed planner tail identities only. Checked CFG success/abort needs a consumer.
use super::*;

/// Actual completed countdown tail attached to a bridge's normal-body rule.
/// This is not an exit contract or a concrete header owner token.
#[derive(Clone, Debug)]
pub struct UnitOwnerBridgeCounterTailV1 {
    control: UnitStatementId,
    callable: UnitOwnerCallable,
    guard: UnitSymbolId,
    counter: UnitSymbolId,
    assignments: [UnitExpressionId; 4],
    steps: Vec<UnitOwnerRootReplaceStepV1>,
    manifest: (UnitSymbolId, UnitSymbolId, [UnitExpressionId; 4]),
}
impl UnitOwnerBridgeCounterTailV1 {
    fn seal(
        rule: &UnitOwnerMultiRootNormalTransportV1,
        guard: UnitSymbolId,
        counter: UnitSymbolId,
        assignments: [UnitExpressionId; 4],
    ) -> Option<Self> {
        let value = Self {
            control: rule.statement(),
            callable: rule.body_catalog().callable(),
            guard,
            counter,
            assignments,
            steps: rule.steps().to_vec(),
            manifest: (guard, counter, assignments),
        };
        value.matches(rule).then_some(value)
    }
    /// Exact second control, not the preceding bridge control.
    pub fn statement(&self) -> UnitStatementId {
        self.control
    }
    /// Callable of the actual completed normal-body catalog.
    pub fn callable(&self) -> UnitOwnerCallable {
        self.callable
    }
    /// Boolean Name guard whose actual reset completed.
    pub fn guard(&self) -> UnitSymbolId {
        self.guard
    }
    /// Integer counter consumed by the checked countdown tail.
    pub fn counter(&self) -> UnitSymbolId {
        self.counter
    }
    /// Actual decrement assignment, subtraction, reset assignment, comparison, in that order.
    pub fn assignments(&self) -> [UnitExpressionId; 4] {
        self.assignments
    }
    fn matches(&self, rule: &UnitOwnerMultiRootNormalTransportV1) -> bool {
        rule.is_consistent()
            && self.control == rule.statement()
            && self.callable == rule.body_catalog().callable()
            && self.steps == rule.steps()
            && self.manifest == (self.guard, self.counter, self.assignments)
            && self.guard != self.counter
            && [self.guard, self.counter].iter().all(|s| {
                s.source_unit() == self.control.source_unit() && !rule.header_roots().contains(s)
            })
            && self.assignments.iter().enumerate().all(|(i, id)| {
                id.source_unit() == self.control.source_unit()
                    && !self.assignments[..i].contains(id)
            })
    }
}
impl UnitOwnerControlBridgeEvidenceV1 {
    pub(in crate::ownership_checking::compilation_unit) fn with_next_counter_tail(
        mut self,
        sites: Option<(UnitSymbolId, UnitSymbolId, [UnitExpressionId; 4])>,
    ) -> Option<Self> {
        if self.next_tail.is_some() {
            return None;
        }
        if let Some((guard, counter, assignments)) = sites {
            self.next_tail = Some(UnitOwnerBridgeCounterTailV1::seal(
                self.next_normal.as_ref()?,
                guard,
                counter,
                assignments,
            )?);
        }
        self.is_consistent().then_some(self)
    }
    /// Completed planner tail identities. Does not prove checked arithmetic CFG or loop exits.
    pub fn next_counter_tail(&self) -> Option<&UnitOwnerBridgeCounterTailV1> {
        self.next_tail.as_ref()
    }
    pub(super) fn tail_matches_normal(&self) -> bool {
        self.next_tail.as_ref().is_none_or(|tail| {
            self.next_normal
                .as_ref()
                .is_some_and(|rule| tail.matches(rule))
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    #[test]
    fn bridge_counter_tail_tracks_completed_sites_and_normal_identity() {
        for reverse in [false, true] {
            let source = super::super::tests::source(reverse, false);
            let fixture = Fixture::new(&[("bridge-completed-tail.ko", &source)]);
            let owned = fixture.shared();
            assert!(owned.diagnostics().is_empty());
            let bridge = owned.owner_observations()[0]
                .control_bridge_evidence_v1()
                .unwrap();
            let rule = bridge.next_normal_transport().unwrap();
            let tail = bridge
                .next_counter_tail()
                .expect("actual two-assignment completion");
            assert_eq!(tail.statement(), bridge.next_control());
            assert_eq!(tail.callable(), rule.body_catalog().callable());
            let texts = tail.assignments().map(|id| {
                let node = fixture.parsed[0]
                    .ast()
                    .expressions()
                    .get(id.expression())
                    .unwrap();
                fixture.sources.slice(node.span()).unwrap()
            });
            assert_eq!(texts, ["n1 = n1 - 1", "n1 - 1", "g1 = n1 > 0", "n1 > 0"]);
            let resolution = fixture.names.names().source_units()
                [tail.guard().source_unit().index()]
            .resolution();
            assert_eq!(
                fixture
                    .sources
                    .slice(resolution.symbols()[tail.guard().symbol().index()].span())
                    .unwrap(),
                "g1"
            );
            assert_eq!(
                fixture
                    .sources
                    .slice(resolution.symbols()[tail.counter().symbol().index()].span())
                    .unwrap(),
                "n1"
            );
            assert!(
                owned.owner_observations()[0]
                    .counter_tail_evidence_v1()
                    .is_none()
            );
            assert!(
                owned.owner_observations()[0]
                    .multi_root_boundary_v1()
                    .is_none()
            );
        }
    }
    #[test]
    fn ci_only_bridge_counter_tail_rejects_foreign_or_detached_sites() {
        let source = super::super::tests::source(false, false);
        let fixture = Fixture::new(&[("bridge-tail-fault.ko", &source)]);
        let first = fixture.shared();
        let second = fixture.shared();
        let bridge = first.owner_observations()[0]
            .control_bridge_evidence_v1()
            .unwrap();
        assert!(bridge.next_counter_tail().is_some());
        let mut foreign = bridge.clone();
        foreign.next_tail = second.owner_observations()[0]
            .control_bridge_evidence_v1()
            .unwrap()
            .next_tail
            .clone();
        assert!(!foreign.is_consistent());
        let mut detached = bridge.clone();
        detached.next_normal = None;
        detached.normal_steps = None;
        assert!(!detached.is_consistent());
        let mut altered = bridge.clone();
        altered.next_tail.as_mut().unwrap().assignments.swap(0, 2);
        assert!(!altered.is_consistent());
        let mut altered = bridge.clone();
        altered.next_tail.as_mut().unwrap().guard = altered.targets[0];
        assert!(!altered.is_consistent());
    }
}
