//! One actually entered nested normal body. No entry owner or loop-exit authority.
use super::*;
use crate::ownership_checking::UnitOwnerNestedNormalBodyV1;
pub(super) struct Nested {
    pub(super) control: UnitStatementId,
    parent: UnitStatementId,
    condition: UnitExpressionId,
    body: UnitStatementId,
    identity: std::sync::Arc<()>,
    callable: UnitOwnerCallable,
    roots: Vec<UnitSymbolId>,
    conditioned: bool,
    body_entered: bool,
    left: bool,
    exit_seen: bool,
    exit_partition: Option<(
        Vec<UnitSymbolId>,
        Vec<crate::ownership_checking::UnitDropFact>,
    )>,
    pub(super) valid: bool,
    pub(super) sink: Option<counter_segment::body::Body>,
}
impl Nested {
    fn matches(&self, state: &ValueState) -> bool {
        boundary_values(state).is_some_and(|values| {
            values.len() == self.roots.len()
                && values
                    .iter()
                    .all(|v| self.roots.iter().filter(|r| **r == v.symbol).count() == 1)
        })
    }
    pub(super) fn begin(&mut self, checker: &Checker<'_>) {
        if !self.valid || !self.body_entered || self.sink.is_some() {
            self.valid = false;
            return;
        }
        self.sink = counter_segment::body::Body::begin_at(
            checker,
            self.control,
            self.condition,
            self.body,
            self.roots.clone(),
            self.callable,
        );
        self.valid &= self.sink.is_some();
    }
}
impl ControlTransactions {
    pub(in super::super::super) fn nested_enter(
        &mut self,
        sites: (UnitStatementId, UnitExpressionId, UnitStatementId),
        parent: Option<UnitStatementId>,
        depth: usize,
        state: &ValueState,
        observation: Option<&UnitOwnerObservation>,
    ) {
        let Some(parent) = parent else {
            return;
        };
        if self.nested_invalid
            || depth != 2
            || self.root_branches_invalid
            || !self.root_branch_arms.is_empty()
            || self.nested.is_some()
        {
            self.nested_invalid = true;
            self.nested = None;
            return;
        }
        if !self.is_none() {
            return;
        }
        let Some(o) = observation else {
            return;
        };
        let Some(values) = boundary_values(state) else {
            return;
        };
        let roots = values.iter().map(|v| v.symbol).collect::<Vec<_>>();
        if roots.len() < 2
            || roots
                .iter()
                .enumerate()
                .any(|(i, r)| roots[..i].contains(r) || r.source_unit() != sites.0.source_unit())
        {
            return;
        }
        self.nested = Some(Nested {
            control: sites.0,
            parent,
            condition: sites.1,
            body: sites.2,
            identity: o.identity.clone(),
            callable: o.callable,
            roots,
            conditioned: false,
            body_entered: false,
            left: false,
            exit_seen: false,
            exit_partition: None,
            valid: true,
            sink: None,
        });
    }
    pub(super) fn nested_condition(
        &mut self,
        control: UnitStatementId,
        condition: UnitExpressionId,
        state: &ValueState,
    ) {
        #[cfg(test)]
        if let Some(n) = self
            .nested
            .as_ref()
            .filter(|n| n.valid && !n.left && n.control == control && n.condition == condition)
        {
            if fault(control, 1) {
                return;
            }
            if fault(control, 3) {
                let body = n.body;
                self.nested_body_entry(control, condition, body, state);
            }
        }
        if let Some(n) = self.nested.as_mut().filter(|n| n.control == control) {
            n.valid &= !n.conditioned && condition == n.condition && n.matches(state);
            n.conditioned = n.valid;
        }
    }
    pub(super) fn nested_body_entry(
        &mut self,
        control: UnitStatementId,
        condition: UnitExpressionId,
        body: UnitStatementId,
        state: &ValueState,
    ) {
        #[cfg(test)]
        if self.nested.as_ref().is_some_and(|n| {
            n.valid && !n.left && n.control == control && n.condition == condition && n.body == body
        }) && fault(control, 2)
        {
            return;
        }
        if let Some(n) = self.nested.as_mut().filter(|n| n.control == control) {
            n.valid &= n.conditioned
                && !n.body_entered
                && condition == n.condition
                && body == n.body
                && n.matches(state);
            n.body_entered = n.valid;
        }
    }
    // Only the actual implicit-false cleanup event may populate this audit fact.
    pub(super) fn nested_false_exit(
        &mut self,
        control: UnitStatementId,
        condition: UnitExpressionId,
        state: &ValueState,
        drops: &[PlannerDropFact],
        possible: bool,
    ) {
        let Some(n) = self.nested.as_mut().filter(|n| n.control == control) else {
            return;
        };
        #[cfg(test)]
        if fault(control, 5) {
            return;
        }
        #[cfg(test)]
        if fault(control, 6) {
            n.exit_seen = true;
        }
        let repeated = n.exit_seen;
        n.exit_seen = true;
        n.exit_partition = None;
        if repeated
            || !possible
            || !n.valid
            || n.left
            || !n.conditioned
            || !n.body_entered
            || n.condition != condition
            || n.sink.as_ref().and_then(|s| s.result()).is_none()
        {
            return;
        }
        n.exit_partition =
            root_incoming::root_exit::actual_partition(control, &n.roots, state, drops);
    }
    pub(in super::super::super) fn nested_leave(
        &mut self,
        control: UnitStatementId,
        success: bool,
    ) {
        if let Some(n) = &mut self.nested {
            if control == n.control {
                n.valid &= success && !n.left;
                n.left = true;
            }
            if control == n.parent {
                n.valid &= success && n.left;
            }
        }
    }
    #[cfg(test)]
    pub(super) fn nested_skip_finish(&self, control: UnitStatementId) -> bool {
        self.nested
            .as_ref()
            .is_some_and(|n| n.valid && !n.left && n.control == control && n.sink.is_some())
            && fault(control, 4)
    }
    pub(in super::super::super) fn take_nested_normal(
        &mut self,
    ) -> Option<UnitOwnerNestedNormalBodyV1> {
        if self.nested_invalid || self.root_branches_invalid || !self.root_branch_arms.is_empty() {
            return None;
        }
        let n = self.nested.take()?;
        if !n.valid || !n.left || !n.body_entered {
            return None;
        }
        UnitOwnerNestedNormalBodyV1::seal(
            n.identity,
            n.callable,
            (n.parent, n.control, n.condition, n.body),
            n.sink?.result()?,
            n.exit_partition,
        )
    }
}

#[cfg(test)]
thread_local! { static FAULT: std::cell::Cell<Option<(UnitStatementId,u8)>> = const { std::cell::Cell::new(None) }; static HITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
#[cfg(test)]
fn fault(control: UnitStatementId, phase: u8) -> bool {
    if FAULT.get() == Some((control, phase)) {
        HITS.set(HITS.get() + 1);
        true
    } else {
        false
    }
}
#[cfg(test)]
mod tests {
    use super::{FAULT, HITS};
    use crate::ownership_checking::compilation_unit::formed_tests::Fixture;
    const SOURCE: &str = r#"fun run(outer: Int, inner: Int, own seed: String) {
        var a=seed; var b="b"; var n0=outer; var g0=n0>0;
        while(g0) {
            var n1=inner; var g1=n1>0;
            while(g1) { val x=replace(&a,"inner-a"); val y=replace(&b,"inner-b"); n1=n1-1; g1=n1>0 };
            val old=replace(&b,"outer-b"); n0=n0-1; g0=n0>0
        };
        println(a); println(b)
    }"#;
    #[test]
    fn nested_normal_body_uses_real_child_events_without_exit_authority() {
        let fixture = Fixture::new(&[("nested-normal.ko", SOURCE)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty());
        let o = owned
            .owner_observations()
            .iter()
            .find(|o| o.nested_normal_body_v1().is_some())
            .expect("actual child body");
        let n = o.nested_normal_body_v1().unwrap();
        assert!(o.is_consistent());
        let (live, drops) = n.false_exit_partition().expect("actual false cleanup");
        assert_eq!(live.len(), 2);
        assert!(drops.is_empty());
        assert!(live.iter().all(|r| n.normal().header_roots().contains(r)));
        assert_ne!(n.parent(), n.control());
        assert_eq!(n.normal().statement(), n.control());
        assert_eq!(n.normal().steps().len(), 2);
        assert_eq!(n.normal().body_catalog().definitions().len(), 2);
        assert!(o.control_chain_bodies_v1().is_none());
        assert!(o.counter_tail_evidence_v1().is_none());
        assert!(o.multi_root_boundary_v1().is_none());
        let deferred =
            crate::ownership_checking::compilation_unit::owner_transport::publish_observations(
                owned.owner_observations().to_vec(),
                true,
                false,
            );
        assert!(deferred.iter().all(|o| o.nested_normal_body_v1().is_none()));
    }
    #[test]
    fn deeper_or_multiple_children_preserve_analysis_without_nested_permission() {
        let child = r#"while(g1) { val x=replace(&a,"inner-a"); val y=replace(&b,"inner-b"); n1=n1-1; g1=n1>0 }"#;
        assert!(SOURCE.contains(child));
        let sibling = r#"var n2=inner; var g2=n2>0;
            while(g2) { val x=replace(&a,"sibling-a"); val y=replace(&b,"sibling-b"); n2=n2-1; g2=n2>0 }"#;
        let sources = [
            SOURCE.replace(child, &format!("{child}; {sibling}")),
            SOURCE.replace(
                child,
                &format!("var n2=inner; var g2=n2>0; while(g2) {{ {child}; n2=n2-1; g2=n2>0 }}"),
            ),
        ];
        for (index, source) in sources.iter().enumerate() {
            let fixture = Fixture::new(&[("nested-shape-boundary.ko", source)]);
            let owned = fixture.shared();
            assert!(owned.diagnostics().is_empty(), "shape {index}");
            assert!(owned.deferred().is_empty(), "shape {index}");
            assert!(
                owned
                    .owner_observations()
                    .iter()
                    .all(|o| o.is_consistent() && o.nested_normal_body_v1().is_none()),
                "shape {index}"
            );
        }
    }
    #[test]
    fn unsupported_nested_body_keeps_generic_analysis_without_new_permission() {
        for extra in ["break;", "continue;", "val unrelated=\"extra\";"] {
            let source = SOURCE.replace("val x=replace", &format!("{extra} val x=replace"));
            let fixture = Fixture::new(&[("nested-rejected.ko", &source)]);
            let owned = fixture.shared();
            assert!(
                owned
                    .owner_observations()
                    .iter()
                    .all(|o| o.nested_normal_body_v1().is_none())
            );
        }
    }
    #[test]
    fn ci_only_nested_normal_rejects_foreign_analysis_and_mixed_capability() {
        let fixture = Fixture::new(&[("nested-foreign.ko", SOURCE)]);
        let owned = fixture.shared();
        let o = owned
            .owner_observations()
            .iter()
            .find(|o| o.nested_normal_body_v1().is_some())
            .unwrap();
        let mut foreign = o.clone();
        foreign.identity = std::sync::Arc::new(());
        assert!(!foreign.is_consistent());
        assert!(foreign.nested_normal_body_v1().is_none());
        let mut complete = o.clone();
        complete.status = crate::ownership_checking::UnitOwnerObservationStatus::Complete;
        assert!(!complete.is_consistent());
        assert!(complete.nested_normal_body_v1().is_none());
    }
    #[test]
    fn ci_only_nested_body_requires_actual_condition_entry_and_finish_order() {
        let fixture = Fixture::new(&[("nested-events.ko", SOURCE)]);
        let baseline = fixture.shared();
        let control = baseline
            .owner_observations()
            .iter()
            .find_map(|o| o.nested_normal_body_v1())
            .unwrap()
            .control();
        for phase in 1..=4 {
            FAULT.set(Some((control, phase)));
            HITS.set(0);
            let owned = fixture.shared();
            let hits = HITS.get();
            FAULT.set(None);
            assert!(
                hits > 0,
                "phase {phase} must hit actual active child events"
            );
            assert!(
                owned
                    .owner_observations()
                    .iter()
                    .all(|o| o.nested_normal_body_v1().is_none()),
                "phase {phase}"
            );
        }
    }
    #[test]
    fn ci_only_nested_false_exit_requires_unique_actual_cleanup() {
        let fixture = Fixture::new(&[("nested-exit-events.ko", SOURCE)]);
        let baseline = fixture.shared();
        let control = baseline
            .owner_observations()
            .iter()
            .find_map(|o| o.nested_normal_body_v1())
            .unwrap()
            .control();
        for phase in [5, 6] {
            FAULT.set(Some((control, phase)));
            HITS.set(0);
            let owned = fixture.shared();
            let hits = HITS.get();
            FAULT.set(None);
            assert!(hits > 0, "actual child cleanup must be targeted");
            let n = owned
                .owner_observations()
                .iter()
                .find_map(|o| o.nested_normal_body_v1())
                .expect("normal permission remains independent");
            assert!(n.false_exit_partition().is_none());
        }
    }
    #[test]
    fn nested_false_cleanup_keeps_root_needed_by_parent_backedge() {
        let source = SOURCE.replace("println(a); println(b)", "println(b)");
        let fixture = Fixture::new(&[("nested-exit-partial.ko", &source)]);
        let owned = fixture.shared();
        assert!(owned.diagnostics().is_empty());
        let n = owned
            .owner_observations()
            .iter()
            .find_map(|o| o.nested_normal_body_v1())
            .unwrap();
        let (live, drops) = n.false_exit_partition().unwrap();
        // Removing the final read does not retire a: the enclosing loop still
        // needs it for the next child activation. Preserve actual planner liveness.
        assert_eq!(live.len(), 2);
        assert!(drops.is_empty());
    }
}
