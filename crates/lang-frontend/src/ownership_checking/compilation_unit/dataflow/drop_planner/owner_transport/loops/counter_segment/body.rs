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
    reset: bool,
    completed: Option<UnitOwnerMultiRootNormalTransportV1>,
}
impl Body {
    pub(in super::super) fn begin(
        checker: &Checker<'_>,
        bridge: &crate::ownership_checking::UnitOwnerControlBridgeEvidenceV1,
    ) -> Option<Self> {
        let roots = bridge.targets().to_vec();
        let plan = BodyPlan::admit(
            checker,
            bridge.next_control(),
            &roots,
            Some((bridge.next_condition(), bridge.next_body())),
        )?;
        let reset = plan.reset().is_none();
        Some(Self {
            control: bridge.next_control(),
            roots,
            plan,
            catalog: UnitOwnerProducerCatalogV1 {
                identity: std::sync::Arc::new(()),
                callable: bridge.prior().callable(),
                definitions: Vec::new(),
            },
            steps: Vec::new(),
            phase: 0,
            valid: true,
            frozen: false,
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
        s.normal = s
            .bridge
            .as_ref()
            .and_then(|b| b.evidence(control, condition, body))
            .and_then(|e| Body::begin(checker, &e));
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
        for phase in 1..=5 {
            HITS.set(0);
            PHASE.set(phase);
            let owned = fixture.shared();
            PHASE.set(0);
            assert_eq!(HITS.get(), 1);
            let bridge = owned.owner_observations()[0]
                .control_bridge_evidence_v1()
                .unwrap();
            assert!(bridge.next_normal_transport().is_none());
        }
    }
}
