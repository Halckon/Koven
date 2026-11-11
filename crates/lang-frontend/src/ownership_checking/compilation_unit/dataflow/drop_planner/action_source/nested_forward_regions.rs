//! Positive finite closure protocol, independent of the old Partial ancestry product.
use super::super::nested_forward_scope::NestedForwardPlan;
use super::{ActionSource, Record};
use crate::{
    ownership_checking::{
        UnitBorrowNestedForwardRegion, UnitLoanActionCoverage, UnitLoanNestedForwardLineage,
    },
    type_checking::UnitItemId,
};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(super) struct Candidate {
    plan: NestedForwardPlan,
    base: (usize, usize),
    start: usize,
    stage: usize,
    valid: bool,
    entries: [Option<usize>; 4],
    inner_closed: bool,
    inner_consumed: bool,
    closed: Option<usize>,
}
impl Candidate {
    fn advance(&mut self, stage: usize, offset: usize, cursor: usize, clear: bool) -> bool {
        self.valid &= clear
            && self.stage == stage
            && self.start.checked_add(offset) == Some(cursor)
            && self.closed.is_none();
        if self.valid {
            self.stage += 1;
        }
        self.valid
    }
}
impl ActionSource {
    pub(in super::super) fn begin_nested_region(
        &mut self,
        plan: NestedForwardPlan,
        base: (usize, usize),
        clear: bool,
    ) -> usize {
        let id = self.nested_regions.len();
        let unique = !self
            .nested_regions
            .iter()
            .any(|c| c.plan.callable == plan.callable);
        self.nested_regions.push(Candidate {
            plan,
            base,
            start: self.records.len(),
            stage: 0,
            valid: self.enabled && clear && unique,
            entries: [None; 4],
            inner_closed: false,
            inner_consumed: false,
            closed: None,
        });
        id
    }
    pub(in super::super) fn nested_region_base(&self, id: usize) -> Option<(usize, usize)> {
        self.nested_regions.get(id).map(|c| c.base)
    }
    pub(in super::super) fn nested_region_condition(
        &mut self,
        id: usize,
        split: usize,
        after: bool,
        clear: bool,
    ) {
        let Some(c) = self.nested_regions.get_mut(id) else {
            return;
        };
        let stage = match (split, after) {
            (0, false) => 0,
            (0, true) => 1,
            (1, false) => 3,
            (1, true) => 4,
            _ => {
                c.valid = false;
                return;
            }
        };
        c.advance(stage, 0, self.records.len(), clear);
    }
    pub(in super::super) fn nested_region_arm(
        &mut self,
        id: usize,
        split: usize,
        then_arm: bool,
        entry: usize,
        clear: bool,
    ) {
        let Some(c) = self.nested_regions.get_mut(id) else {
            return;
        };
        let (role, stage, offset) = match (split, then_arm) {
            (0, true) => (0, 2, 0),
            (1, true) => (1, 5, 0),
            (1, false) => (2, 8, 2),
            (0, false) => (3, 12, 4),
            _ => {
                c.valid = false;
                return;
            }
        };
        let expected_parent = if split == 1 { c.entries[0] } else { None };
        let actual = self.nested_entries.get(entry);
        let good = actual.is_some_and(|e| {
            e.callable == c.plan.callable
                && e.split == c.plan.splits[split].split
                && e.condition == c.plan.splits[split].condition
                && e.then_arm == then_arm
                && e.parent == expected_parent
                && e.cursor == self.records.len()
        }) && (split == 0 || expected_parent.is_some())
            && !c.entries.contains(&Some(entry))
            && (role != 3 || (c.inner_closed && !c.inner_consumed));
        if c.advance(stage, offset, self.records.len(), clear && good) {
            c.entries[role] = Some(entry);
            if role == 3 {
                c.inner_consumed = true;
            }
        }
    }
    pub(in super::super) fn nested_region_return(
        &mut self,
        id: usize,
        leaf: usize,
        post: bool,
        clear: bool,
    ) {
        let Some(c) = self.nested_regions.get_mut(id) else {
            return;
        };
        let stage = match leaf {
            0 => 6,
            1 => 9,
            2 => 13,
            _ => {
                c.valid = false;
                return;
            }
        } + usize::from(post);
        let offset = leaf * 2 + 1 + usize::from(post);
        let wanted = c.plan.leaves[leaf];
        let begin = c.start + leaf * 2;
        let source = match self.records.get(begin) {
            Some(Record::Begin(source)) => Some(source),
            _ => None,
        };
        let good = source
            .is_some_and(|s| s.call() == wanted.value && s.argument() == wanted.argument)
            && (!post
                || self
                    .nested_handoffs
                    .iter()
                    .filter(|(index, cause)| {
                        *index == begin + 1
                            && cause.state.coverage == Some(id)
                            && cause.state.plan == c.plan
                            && cause.state.leaf == Some(leaf)
                            && cause.acquisition.index == begin
                            && Arc::ptr_eq(&cause.acquisition.identity, &self.identity)
                    })
                    .count()
                    == 1);
        c.advance(stage, offset, self.records.len(), clear && good);
    }
    pub(in super::super) fn nested_region_join(
        &mut self,
        id: usize,
        split: usize,
        both_terminated: bool,
        no_live: bool,
        clear: bool,
    ) {
        let Some(c) = self.nested_regions.get_mut(id) else {
            return;
        };
        let (stage, offset) = match split {
            1 => (11, 4),
            0 => (15, 6),
            _ => {
                c.valid = false;
                return;
            }
        };
        let good =
            both_terminated && no_live && (split == 1 || (c.inner_closed && c.inner_consumed));
        if c.advance(stage, offset, self.records.len(), clear && good) && split == 1 {
            c.inner_closed = true;
        }
    }
    pub(in super::super) fn seal_nested_region(
        &mut self,
        id: usize,
        terminated: bool,
        clear: bool,
    ) {
        let Some(c) = self.nested_regions.get_mut(id) else {
            return;
        };
        c.advance(
            16,
            6,
            self.records.len(),
            clear && terminated && c.inner_closed && c.inner_consumed,
        );
    }
    pub(in super::super) fn close_nested_callable(&mut self, callable: UnitItemId) {
        for c in self
            .nested_regions
            .iter_mut()
            .filter(|c| c.plan.callable == callable)
        {
            if c.advance(17, 6, self.records.len(), true) {
                c.closed = Some(self.records.len());
            }
        }
    }
    pub(super) fn finish_nested_regions(
        &self,
        lineages: &[UnitLoanNestedForwardLineage],
    ) -> Vec<UnitBorrowNestedForwardRegion> {
        #[cfg(test)]
        tests::record(self, lineages);
        self.nested_regions
            .iter()
            .enumerate()
            .map(|(id, c)| {
                let paths = lineages
                    .iter()
                    .filter(|l| l.callable() == c.plan.callable)
                    .cloned()
                    .collect::<Vec<_>>();
                let good = c.valid
                    && c.stage == 18
                    && c.closed == c.start.checked_add(6)
                    && c.inner_closed
                    && c.inner_consumed
                    && self
                        .nested_regions
                        .iter()
                        .filter(|other| other.plan.callable == c.plan.callable)
                        .count()
                        == 1
                    && self.records.get(c.start..c.start.saturating_add(6)).is_some_and(|records| {
                    records.chunks_exact(2).enumerate().all(|(leaf, pair)| {
                        matches!(pair, [Record::Begin(begin), Record::BorrowForwardHandoff { transaction, .. }]
                            if transaction.source_loan() == begin
                                && transaction.returned_expression() == c.plan.leaves[leaf].value
                                && transaction.point() == crate::ownership_checking::UnitDropPoint::ControlTransfer(c.plan.leaves[leaf].control))
                    })
                })
                && paths.len() == 3
                    && paths.iter().enumerate().all(|(leaf, l)| {
                        let expected = c.plan.leaves[leaf];
                        let expected_entries = match leaf {
                            0 => [c.entries[0], c.entries[1]],
                            1 => [c.entries[0], c.entries[2]],
                            _ => [c.entries[3], None],
                        };
                        Arc::ptr_eq(&l.identity, &self.identity)
                            && l.source_parameter() == c.plan.root
                            && l.control() == expected.control
                            && l.value() == expected.value
                            && l.begin == c.start + leaf * 2
                            && l.handoff == c.start + leaf * 2 + 1
                            && self
                                .nested_handoffs
                                .iter()
                                .filter(|(index, cause)| {
                                    *index == l.handoff
                                        && cause.state.coverage == Some(id)
                                        && cause.state.plan == c.plan
                                        && cause.state.path == expected_entries
                                })
                                .count()
                                == 1
                    });
                UnitBorrowNestedForwardRegion {
                    identity: self.identity.clone(),
                    callable: c.plan.callable,
                    root: c.plan.root,
                    coverage: if good {
                        UnitLoanActionCoverage::Complete
                    } else {
                        UnitLoanActionCoverage::Partial
                    },
                    paths: if good { paths } else { Vec::new() },
                }
            })
            .collect()
    }
}
#[cfg(test)]
mod tests;
