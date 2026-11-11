//! Positive finite closure protocol, independent of the old Partial ancestry product.
use super::super::nested_forward_scope::{NestedForwardPlan, NestedForwardState};
use super::{ActionSource, Record};
use crate::{
    ownership_checking::{
        UnitBorrowNestedForwardRegion, UnitBorrowNestedForwardTopology, UnitLoanActionCoverage,
        UnitLoanNestedForwardLineage,
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
    entries: [Option<usize>; 6],
    inner_closed: bool,
    inner_consumed: bool,
    right_closed: bool,
    right_consumed: bool,
    continuation: Option<NestedForwardState>,
    resumed: bool,
    closed: Option<usize>,
}
#[derive(Clone, Debug)]
pub(in super::super) struct ContinuationToken {
    identity: Arc<()>,
    candidate: usize,
    entry: usize,
    cursor: usize,
    state: NestedForwardState,
}
impl Candidate {
    fn tail(&self) -> bool {
        self.plan.topology == UnitBorrowNestedForwardTopology::NestedThenTail
    }

    fn balanced(&self) -> bool {
        self.plan.topology == UnitBorrowNestedForwardTopology::BalancedBoth
    }
    fn action_count(&self) -> usize {
        self.plan.leaves().len() * 2
    }
    fn close_stage(&self) -> usize {
        if self.balanced() {
            24
        } else if self.tail() {
            18
        } else {
            17
        }
    }
    fn tokens_consumed(&self) -> bool {
        self.inner_closed
            && self.inner_consumed
            && (!self.balanced() || (self.right_closed && self.right_consumed))
    }
    fn entry_role(&self, split: usize, then: bool) -> Option<usize> {
        if self.balanced() {
            match (split, then) {
                (0, true) => Some(0),
                (0, false) => Some(3),
                (1, true) => Some(1),
                (1, false) => Some(2),
                (2, true) => Some(4),
                (2, false) => Some(5),
                _ => None,
            }
        } else {
            match split {
                0 => Some(if then == self.plan.topology.nested_then() {
                    0
                } else {
                    3
                }),
                1 => Some(if then { 1 } else { 2 }),
                _ => None,
            }
        }
    }

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
            valid: self.enabled && clear && unique && plan.topology_matches_tree(),
            entries: [None; 6],
            inner_closed: false,
            inner_consumed: false,
            right_closed: false,
            right_consumed: false,
            continuation: None,
            resumed: false,
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
        let (stage, offset) = if c.balanced() {
            match (split, after) {
                (0, false) => (0, 0),
                (0, true) => (1, 0),
                (1, false) => (3, 0),
                (1, true) => (4, 0),
                (2, false) => (13, 4),
                (2, true) => (14, 4),
                _ => {
                    c.valid = false;
                    return;
                }
            }
        } else {
            match (c.plan.topology.nested_then(), split, after) {
                (_, 0, false) => (0, 0),
                (_, 0, true) => (1, 0),
                (true, 1, false) => (3, 0),
                (true, 1, true) => (4, 0),
                (false, 1, false) => (6, 2),
                (false, 1, true) => (7, 2),
                _ => {
                    c.valid = false;
                    return;
                }
            }
        };
        c.advance(stage, offset, self.records.len(), clear);
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
        let (role, stage, offset) = if c.balanced() {
            match (split, then_arm) {
                (0, true) => (0, 2, 0),
                (1, true) => (1, 5, 0),
                (1, false) => (2, 8, 2),
                (0, false) => (3, 12, 4),
                (2, true) => (4, 15, 4),
                (2, false) => (5, 18, 6),
                _ => {
                    c.valid = false;
                    return;
                }
            }
        } else {
            match (c.plan.topology.nested_then(), split, then_arm) {
                (true, 0, true) => (0, 2, 0),
                (true, 1, true) => (1, 5, 0),
                (true, 1, false) => (2, 8, 2),
                (true, 0, false) => (3, 12, 4),
                (false, 0, true) => (3, 2, 0),
                (false, 0, false) => (0, 5, 2),
                (false, 1, true) => (1, 8, 2),
                (false, 1, false) => (2, 11, 4),
                _ => {
                    c.valid = false;
                    return;
                }
            }
        };
        let expected_parent = match split {
            1 => c.entries[0],
            2 => c.entries[3],
            _ => None,
        };
        let Some(wanted) = c.plan.splits().get(split).copied() else {
            c.valid = false;
            return;
        };
        let actual = self.nested_entries.get(entry);
        let good = actual.is_some_and(|e| {
            e.callable == c.plan.callable
                && e.topology == c.plan.topology
                && e.flag_layout == c.plan.flag_layout
                && e.split == wanted.split
                && e.condition == wanted.condition
                && e.then_arm == then_arm
                && e.parent == expected_parent
                && e.cursor == self.records.len()
        }) && (split == 0 || expected_parent.is_some())
            && !c.entries.contains(&Some(entry))
            && (!(c.balanced() || c.plan.topology.nested_then())
                || role != 3
                || (c.inner_closed && !c.inner_consumed));
        if c.advance(stage, offset, self.records.len(), clear && good) {
            c.entries[role] = Some(entry);
            if role == 3 && !c.tail() && (c.balanced() || c.plan.topology.nested_then()) {
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
        let stage = if c.tail() {
            match leaf {
                0 => 6,
                1 => 9,
                2 => 15,
                _ => {
                    c.valid = false;
                    return;
                }
            }
        } else if c.balanced() {
            match leaf {
                0 => 6,
                1 => 9,
                2 => 16,
                3 => 19,
                _ => {
                    c.valid = false;
                    return;
                }
            }
        } else {
            match (c.plan.topology.nested_then(), leaf) {
                (true, 0) => 6,
                (_, 1) => 9,
                (true, 2) => 13,
                (false, 0) => 3,
                (false, 2) => 12,
                _ => {
                    c.valid = false;
                    return;
                }
            }
        } + usize::from(post);
        let offset = leaf * 2 + 1 + usize::from(post);
        let Some(wanted) = c.plan.leaves().get(leaf).copied() else {
            c.valid = false;
            return;
        };
        let begin = c.start + leaf * 2;
        let source = match self.records.get(begin) {
            Some(Record::Begin(source)) => Some(source),
            _ => None,
        };
        let good = (!c.tail() || leaf != 2 || (c.resumed && c.continuation.is_some()))
            && source.is_some_and(|s| s.call() == wanted.value && s.argument() == wanted.argument)
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
        if c.tail() && split != 1 {
            c.valid = false;
            return;
        }
        let (stage, offset) = if c.balanced() {
            match split {
                1 => (11, 4),
                2 => (21, 8),
                0 => (22, 8),
                _ => {
                    c.valid = false;
                    return;
                }
            }
        } else {
            match (c.plan.topology.nested_then(), split) {
                (true, 1) => (11, 4),
                (false, 1) => (14, 6),
                (_, 0) => (15, 6),
                _ => {
                    c.valid = false;
                    return;
                }
            }
        };
        let consumed_correctly = if c.plan.topology.nested_then() {
            c.inner_consumed
        } else {
            !c.inner_consumed
        };
        let closure = if c.balanced() {
            match split {
                1 => !c.inner_closed && !c.inner_consumed,
                2 => c.inner_closed && c.inner_consumed && !c.right_closed && !c.right_consumed,
                0 => c.inner_closed && c.inner_consumed && c.right_closed && !c.right_consumed,
                _ => false,
            }
        } else {
            split == 1 || (c.inner_closed && consumed_correctly)
        };
        let good = both_terminated && no_live && closure;
        if c.advance(stage, offset, self.records.len(), clear && good) {
            if split == 1 {
                c.inner_closed = true;
            } else if c.balanced() && split == 2 {
                c.right_closed = true;
            } else if c.balanced() {
                c.right_consumed = true;
            } else if !c.plan.topology.nested_then() {
                c.inner_consumed = true;
            }
        }
    }
    pub(in super::super) fn nested_region_continuation(
        &mut self,
        id: usize,
        successor: Option<NestedForwardState>,
        then_terminated: bool,
        else_terminated: bool,
        clear: bool,
    ) -> Option<ContinuationToken> {
        let c = self.nested_regions.get_mut(id)?;
        let matching = successor.is_some_and(|state| {
            state.plan == c.plan
                && state.coverage == Some(id)
                && state.leaf == Some(2)
                && state.ready_split.is_none()
                && state.path == [c.entries[3], None]
                && c.entries[3].is_some()
        });
        let good = c.tail()
            && then_terminated
            && !else_terminated
            && matching
            && c.inner_closed
            && !c.inner_consumed
            && c.continuation.is_none()
            && !c.resumed;
        if !c.advance(13, 4, self.records.len(), clear && good) {
            return None;
        }
        let state = successor?;
        c.inner_consumed = true;
        c.continuation = Some(state);
        Some(ContinuationToken {
            identity: self.identity.clone(),
            candidate: id,
            entry: c.entries[3]?,
            cursor: self.records.len(),
            state,
        })
    }
    pub(in super::super) fn nested_continuation_base(
        &self,
        token: &ContinuationToken,
    ) -> Option<(usize, usize)> {
        Arc::ptr_eq(&token.identity, &self.identity).then_some(())?;
        self.nested_regions.get(token.candidate).map(|c| c.base)
    }
    pub(in super::super) fn resume_nested_continuation(
        &mut self,
        token: ContinuationToken,
        clear: bool,
    ) -> Option<NestedForwardState> {
        let c = self.nested_regions.get_mut(token.candidate)?;
        let good = c.tail()
            && Arc::ptr_eq(&token.identity, &self.identity)
            && token.cursor == self.records.len()
            && c.continuation == Some(token.state)
            && token.state.coverage == Some(token.candidate)
            && token.state.plan == c.plan
            && c.entries[3] == Some(token.entry)
            && token.state.path == [Some(token.entry), None]
            && c.inner_closed
            && c.inner_consumed
            && !c.resumed;
        if !c.advance(14, 4, self.records.len(), clear && good) {
            return None;
        }
        c.resumed = true;
        Some(token.state)
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
            c.close_stage() - 1,
            c.action_count(),
            self.records.len(),
            clear && terminated && c.tokens_consumed() && (!c.tail() || c.resumed),
        );
    }
    pub(in super::super) fn close_nested_callable(&mut self, callable: UnitItemId) {
        for c in self
            .nested_regions
            .iter_mut()
            .filter(|c| c.plan.callable == callable)
        {
            if c.advance(c.close_stage(), c.action_count(), self.records.len(), true) {
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
                    && c.plan.topology_matches_tree()
                    && c.stage == c.close_stage()+1
                    && c.closed == c.start.checked_add(c.action_count())
                    && c.tokens_consumed()
                    && if c.tail() {
                        c.resumed && c.continuation.is_some_and(|state| state.plan == c.plan
                            && state.coverage == Some(id) && state.leaf == Some(2)
                            && state.ready_split.is_none() && state.path == [c.entries[3], None])
                            && c.entries[3].and_then(|entry| self.nested_entries.get(entry)).is_some_and(|entry|
                                entry.callable == c.plan.callable && entry.topology == c.plan.topology
                                && entry.flag_layout == c.plan.flag_layout && entry.split == c.plan.splits()[0].split
                                && entry.condition == c.plan.splits()[0].condition && !entry.then_arm
                                && entry.parent.is_none() && entry.cursor == c.start + 4)
                    } else { !c.resumed && c.continuation.is_none() }
                    && self
                        .nested_regions
                        .iter()
                        .filter(|other| other.plan.callable == c.plan.callable)
                        .count()
                        == 1
                    && self.records.get(c.start..c.start.saturating_add(c.action_count())).is_some_and(|records| {
                    records.chunks_exact(2).enumerate().all(|(leaf, pair)| {
                        matches!(pair, [Record::Begin(begin), Record::BorrowForwardHandoff { transaction, .. }]
                            if transaction.source_loan() == begin
                                && transaction.returned_expression() == c.plan.leaves()[leaf].value
                                && transaction.point() == crate::ownership_checking::UnitDropPoint::ControlTransfer(c.plan.leaves()[leaf].control))
                    })
                })
                && paths.len() == c.plan.leaves().len()
                    && paths.iter().enumerate().all(|(leaf, l)| {
                        let expected = c.plan.leaves()[leaf];
                        let expected_entries = expected.path.map(|edge| edge.map(|(split,then)| {
                            let role = c.entry_role(split, then);
                            role.and_then(|role| c.entries[role])
                        })).map(Option::flatten);
                        Arc::ptr_eq(&l.identity, &self.identity)
                            && l.topology() == c.plan.topology
                            && l.flag_layout() == c.plan.flag_layout
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
                let continuation = if good && c.tail() {
                    c.entries[3].and_then(|entry| self.nested_entries.get(entry)).map(|entry| {
                        crate::ownership_checking::UnitBorrowNestedForwardContinuation {
                            split: entry.split, condition: entry.condition,
                            tail_control: c.plan.leaves()[2].control,
                            entry: crate::ownership_checking::UnitLoanNestedForwardEdge {
                                split: entry.split, condition: entry.condition, then_arm: entry.then_arm,
                            },
                        }
                    })
                } else { None };
                UnitBorrowNestedForwardRegion {
                    continuation,
                    flag_layout: c.plan.flag_layout,
                    topology: c.plan.topology,
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
