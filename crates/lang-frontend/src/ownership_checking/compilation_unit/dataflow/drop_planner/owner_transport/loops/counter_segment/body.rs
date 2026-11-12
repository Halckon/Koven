//! Formal Header obligations, populated only by actual next-body events.
use super::super::root_incoming::body_plan::BodyPlan;
use super::*;
use crate::ownership_checking::{
    UnitDropFact, UnitDropPoint, UnitDropTarget, UnitOwnerMultiRootNormalTransportV1,
    UnitOwnerRootReplaceStepV1,
};

pub(in super::super) struct Body {
    control: UnitStatementId,
    roots: Vec<UnitSymbolId>,
    plan: BodyPlan,
    catalog: UnitOwnerProducerCatalogV1,
    steps: Vec<UnitOwnerRootReplaceStepV1>,
    phase: u8, // 0 ready, 1 actual literal, 2 Replace, 3 bound
    valid: bool,
    frozen: bool,
    exit_seen: bool,
    suffix: Option<super::super::root_incoming::root_suffix::RootSuffix>,
    exit_partition: Option<(Vec<UnitSymbolId>, Vec<UnitDropFact>)>,
    reset: bool,
    completed: Option<UnitOwnerMultiRootNormalTransportV1>,
}
impl Body {
    pub(in super::super) fn begin(
        checker: &Checker<'_>,
        bridge: &crate::ownership_checking::UnitOwnerControlBridgeEvidenceV1,
    ) -> Option<Self> {
        Self::begin_at(
            checker,
            bridge.next_control(),
            bridge.next_condition(),
            bridge.next_body(),
            bridge.targets().to_vec(),
            bridge.prior().callable(),
        )
    }
    pub(in super::super) fn begin_at(
        checker: &Checker<'_>,
        control: UnitStatementId,
        condition: UnitExpressionId,
        body: UnitStatementId,
        roots: Vec<UnitSymbolId>,
        callable: crate::ownership_checking::UnitOwnerCallable,
    ) -> Option<Self> {
        let plan = BodyPlan::admit(checker, control, &roots, Some((condition, body)))?;
        let reset = plan.reset().is_none();
        Some(Self {
            control,
            roots,
            plan,
            catalog: UnitOwnerProducerCatalogV1 {
                identity: std::sync::Arc::new(()),
                callable,
                definitions: Vec::new(),
            },
            steps: Vec::new(),
            phase: 0,
            valid: true,
            frozen: false,
            exit_seen: false,
            suffix: None,
            exit_partition: None,
            reset,
            completed: None,
        })
    }
    pub(in super::super) fn literal(&mut self, id: UnitExpressionId) {
        if self.frozen {
            return;
        }

        #[cfg(test)]
        if tests::omit(1) {
            return;
        }

        if !self.valid {
            return;
        }
        let i = self.steps.len();
        if self.phase != 0
            || self.catalog.definitions.len() != i
            || self.plan.planned.get(i).map(|p| p.2) != Some(id)
        {
            self.valid = false;
            return;
        }
        self.catalog.definitions.push(UnitOwnerDefinition {
            id: UnitOwnerId {
                identity: self.catalog.identity.clone(),
                callable: self.catalog.callable,
                index: i,
            },
            origin: UnitOwnerOrigin::String(id),
        });
        self.phase = 1;
    }
    pub(in super::super) fn replace(
        &mut self,
        call: UnitExpressionId,
        root: UnitSymbolId,
        producer: UnitExpressionId,
    ) {
        if self.frozen {
            return;
        }

        #[cfg(test)]
        if tests::omit(2) {
            return;
        }

        self.valid &= self.phase == 1
            && self.plan.planned.get(self.steps.len()) == Some(&(call, root, producer));
        if self.valid {
            self.phase = 2;
        }
    }
    pub(in super::super) fn bind(&mut self, call: UnitExpressionId, binding: UnitSymbolId) {
        if self.frozen {
            return;
        }

        #[cfg(test)]
        if tests::omit(3) {
            return;
        }

        let i = self.steps.len();
        self.valid &= self.phase == 2
            && self.plan.planned.get(i).map(|p| p.0) == Some(call)
            && self.plan.bindings.get(i).map(|p| p.0) == Some(binding);
        if self.valid {
            self.phase = 3;
        }
    }
    pub(in super::super) fn drop_result(&mut self, fact: UnitDropFact) {
        if self.frozen {
            return;
        }

        #[cfg(test)]
        if tests::omit(4) {
            return;
        }

        if !self.valid {
            return;
        }
        let UnitDropTarget::Named(binding) = fact.target() else {
            self.valid = false;
            return;
        };
        let i = self.steps.len();
        if self.phase != 3
            || self.plan.bindings.get(i).is_none_or(|(b, s)| {
                *b != binding || fact.point() != UnitDropPoint::AfterStatement(*s)
            })
        {
            self.valid = false;
            return;
        }
        let Some((call, root, _)) = self.plan.planned.get(i) else {
            self.valid = false;
            return;
        };
        let Some(definition) = self.catalog.definitions.get(i) else {
            self.valid = false;
            return;
        };
        self.steps.push(UnitOwnerRootReplaceStepV1 {
            call: *call,
            root: *root,
            producer: definition.id.clone(),
            retired_result: fact,
        });
        self.phase = 0;
    }
    fn ready(&self) -> bool {
        self.valid
            && self.phase == 0
            && self.steps.len() == self.roots.len()
            && self.catalog.definitions.len() == self.roots.len()
    }
    pub(in super::super) fn assignment(&mut self, id: UnitExpressionId, state: &ValueState) {
        if self.frozen {
            return;
        }

        #[cfg(test)]
        if tests::omit(6) || (self.plan.reset() == Some(id) && tests::omit(7)) {
            return;
        }
        let ready = self.ready() && self.matches_state(state);
        if !self.plan.complete_tail(id, ready) {
            self.valid = false;
        }
        if self.plan.reset() == Some(id) {
            self.valid &= ready && !self.reset;
            self.reset = self.valid;
        }
    }
    fn matches_state(&self, state: &ValueState) -> bool {
        boundary_values(state).is_some_and(|values| {
            values.len() == self.roots.len()
                && values
                    .iter()
                    .all(|v| self.roots.iter().filter(|r| **r == v.symbol).count() == 1)
        })
    }
    pub(in super::super) fn finish(&mut self, state: &ValueState, normal: bool) {
        if self.frozen {
            self.valid = false;
            self.completed = None;
            return;
        }
        self.frozen = true;

        #[cfg(test)]
        if tests::omit(5) {
            return;
        }

        self.completed = None;
        if !normal || !self.ready() || !self.reset || !self.matches_state(state) {
            self.valid = false;
            return;
        }
        self.completed = UnitOwnerMultiRootNormalTransportV1::seal(
            self.control,
            self.roots.clone(),
            self.catalog.clone(),
            self.steps.clone(),
        );
    }
    pub(in super::super) fn false_exit(
        &mut self,
        control: UnitStatementId,
        condition: UnitExpressionId,
        state: &ValueState,
        drops: &[PlannerDropFact],
        possible: bool,
    ) {
        self.exit_partition = None;
        self.suffix = None;
        if self.exit_seen {
            return;
        }
        self.exit_seen = true;
        #[cfg(test)]
        if tests::omit(8) {
            return;
        }
        if !possible
            || control != self.control
            || condition != self.plan.condition
            || self.counter_tail().is_none()
        {
            return;
        }
        self.exit_partition = super::super::root_incoming::root_exit::actual_partition(
            control,
            &self.roots,
            state,
            drops,
        );
    }
    pub(in super::super) fn begin_suffix(&mut self, scope_depth: usize) {
        let Some((live, _)) = self.exit_partition.as_ref() else {
            return;
        };
        if !live.is_empty() {
            self.suffix = Some(
                super::super::root_incoming::root_suffix::RootSuffix::new_strict(
                    live.clone(),
                    scope_depth,
                ),
            );
        }
    }
    pub(in super::super) fn suffix_mut(
        &mut self,
    ) -> Option<&mut super::super::root_incoming::root_suffix::RootSuffix> {
        self.suffix.as_mut()
    }
    pub(in super::super) fn suffix_result(&self) -> Option<Vec<(UnitSymbolId, UnitDropFact)>> {
        self.exit_partition()?;
        self.suffix.as_ref()?.completed_snapshot()
    }
    pub(in super::super) fn exit_partition(
        &self,
    ) -> Option<(Vec<UnitSymbolId>, Vec<UnitDropFact>)> {
        self.counter_tail()?;
        self.exit_partition.clone()
    }
    pub(in super::super) fn counter_tail(
        &self,
    ) -> Option<(UnitSymbolId, UnitSymbolId, [UnitExpressionId; 4])> {
        self.result()?;
        self.plan.completed_counter()
    }
    pub(in super::super) fn result(&self) -> Option<UnitOwnerMultiRootNormalTransportV1> {
        self.valid.then(|| self.completed.clone()).flatten()
    }
}
impl ControlTransactions {
    pub(in super::super) fn bridge_body_mut(
        &mut self,
        control: UnitStatementId,
    ) -> Option<&mut Body> {
        if self.root_branches_invalid || !self.root_branch_arms.is_empty() {
            return None;
        }
        if self.nested.as_ref().is_some_and(|n| n.control == control) {
            return self.nested.as_mut().filter(|n| n.valid)?.sink.as_mut();
        }
        self.counter_segments
            .iter_mut()
            .find(|s| !s.invalid && s.captured && s.statements.last() == Some(&control))?
            .normal
            .as_mut()
    }
    pub(in super::super) fn begin_bridge_normal(
        &mut self,
        checker: &Checker<'_>,
        control: UnitStatementId,
    ) {
        if self.root_branches_invalid || !self.root_branch_arms.is_empty() {
            return;
        }
        if self.nested.as_ref().is_some_and(|n| n.control == control) {
            if let Some(n) = &mut self.nested {
                n.begin(checker);
            }
            return;
        }
        let Some(s) = self
            .counter_segments
            .iter_mut()
            .find(|s| !s.invalid && s.captured && s.statements.last() == Some(&control))
        else {
            return;
        };
        let Some((condition, body)) = s.bridge_body else {
            return;
        };
        if s.normal_started {
            s.normal = None;
            return;
        }
        s.normal_started = true;
        s.normal = if let Some(bridge) = &s.bridge {
            bridge
                .evidence(control, condition, body)
                .and_then(|e| Body::begin(checker, &e))
        } else {
            s.continuation
                .as_ref()
                .and_then(|c| c.body_inputs())
                .and_then(|(roots, callable)| {
                    Body::begin_at(checker, control, condition, body, roots, callable)
                })
        };
    }
}

#[cfg(test)]
mod tests {
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    thread_local! { static PHASE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) }; static HITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
    pub(super) fn omit(phase: u8) -> bool {
        if PHASE.get() == phase && HITS.get() == 0 {
            HITS.set(1);
            true
        } else {
            false
        }
    }
    #[test]
    fn ci_only_bridge_normal_requires_each_actual_event() {
        let source = r#"fun run(n: Int) {
            var n0 = n; var g0 = n0 > 0; var a0 = "a"; var b0 = "b";
            while(g0) { val x = replace(&a0, "x"); val y = replace(&b0, "y"); n0 = n0 - 1; g0 = n0 > 0 };
            var n1 = n; var g1 = n1 > 0; var a1 = a0; var b1 = b0;
            while(g1) { val x = replace(&a1, "x1"); val y = replace(&b1, "y1"); n1 = n1 - 1; g1 = n1 > 0 };
            println(a1); println(b1)
        }"#;
        let fixture = Fixture::new(&[("bridge-normal-fault.ko", source)]);
        let baseline = fixture.shared();
        assert!(
            baseline.owner_observations()[0]
                .control_bridge_evidence_v1()
                .unwrap()
                .next_normal_transport()
                .is_some()
        );
        HITS.set(0);
        PHASE.set(8);
        let missing_exit = fixture.shared();
        PHASE.set(0);
        assert_eq!(HITS.get(), 1);
        let bridge = missing_exit.owner_observations()[0]
            .control_bridge_evidence_v1()
            .unwrap();
        assert!(bridge.next_normal_transport().is_some());
        assert!(bridge.next_counter_tail().is_some());
        assert!(bridge.next_false_exit_partition().is_none());
        for phase in 1..=7 {
            HITS.set(0);
            PHASE.set(phase);
            let owned = fixture.shared();
            PHASE.set(0);
            assert_eq!(HITS.get(), 1);
            let bridge = owned.owner_observations()[0]
                .control_bridge_evidence_v1()
                .unwrap();
            assert!(bridge.next_normal_transport().is_none());
            assert!(bridge.next_counter_tail().is_none());
        }
    }
}
