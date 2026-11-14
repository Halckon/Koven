//! Capture actual parent traversal boundaries without altering cleanup or permission.
use super::*;
use crate::ownership_checking::UnitOwnerNestedParentBoundaryV1;
pub(in super::super) struct Parent {
    sites: (UnitStatementId, UnitExpressionId, UnitStatementId),
    roots: Vec<UnitSymbolId>,
    stage: u8,
    valid: bool,
    partition: Option<(
        Vec<UnitSymbolId>,
        Vec<crate::ownership_checking::UnitDropFact>,
    )>,
}
impl Parent {
    pub(super) fn new(
        sites: (UnitStatementId, UnitExpressionId, UnitStatementId),
        state: &ValueState,
    ) -> Option<Self> {
        let roots: Vec<_> = boundary_values(state)?.iter().map(|v| v.symbol).collect();
        (roots.len() >= 2).then_some(Self {
            sites,
            roots,
            stage: 0,
            valid: true,
            partition: None,
        })
    }
    fn matches(&self, state: &ValueState) -> bool {
        boundary_values(state)
            .is_some_and(|v| v.iter().map(|v| v.symbol).collect::<Vec<_>>() == self.roots)
    }
    pub(super) fn finish(self, nested: &Nested) -> Option<UnitOwnerNestedParentBoundaryV1> {
        if !self.valid
            || self.stage != 4
            || self.sites.0 != nested.parent
            || self.roots != nested.roots
            || nested.resume_roots.as_ref() != Some(&self.roots)
            || !nested.valid
            || !nested.left
        {
            return None;
        }
        UnitOwnerNestedParentBoundaryV1::seal(self.sites, self.roots, self.partition?)
    }
}
impl ControlTransactions {
    pub(in super::super) fn parent_condition(
        &mut self,
        control: UnitStatementId,
        condition: UnitExpressionId,
        state: &ValueState,
    ) {
        if let Some(p) = self.nested_parent.as_mut().filter(|p| p.sites.0 == control) {
            p.valid &= p.stage == 0 && p.sites.1 == condition && p.matches(state);
            p.stage = 1;
        }
    }
    pub(in super::super) fn parent_body_entry(
        &mut self,
        control: UnitStatementId,
        condition: UnitExpressionId,
        body: UnitStatementId,
        state: &ValueState,
    ) {
        if let Some(p) = self.nested_parent.as_mut().filter(|p| p.sites.0 == control) {
            p.valid &=
                p.stage == 1 && p.sites.1 == condition && p.sites.2 == body && p.matches(state);
            p.stage = 2;
        }
    }
    pub(in super::super) fn parent_backedge(
        &mut self,
        control: UnitStatementId,
        state: &ValueState,
        normal: bool,
    ) {
        if let Some(p) = self.nested_parent.as_mut().filter(|p| p.sites.0 == control) {
            p.valid &= p.stage == 2 && normal && p.matches(state);
            p.stage = 3;
        }
    }
    pub(in super::super) fn parent_false_exit(
        &mut self,
        control: UnitStatementId,
        condition: UnitExpressionId,
        state: &ValueState,
        drops: &[PlannerDropFact],
        possible: bool,
    ) {
        if let Some(p) = self.nested_parent.as_mut().filter(|p| p.sites.0 == control) {
            p.valid &= p.stage == 3 && condition == p.sites.1 && possible;
            p.stage = 4;
            p.partition =
                root_incoming::root_exit::actual_partition(control, &p.roots, state, drops);
            p.valid &= p.partition.is_some();
        }
    }
}
