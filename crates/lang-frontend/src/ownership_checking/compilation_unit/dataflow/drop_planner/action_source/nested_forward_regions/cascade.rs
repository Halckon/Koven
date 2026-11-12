//! Two actual merges with an explicit inner-resume to outer-token causal chain.
use super::*;
use crate::ownership_checking::{
    UnitBorrowNestedForwardCascadeContinuation, UnitBorrowNestedForwardContinuation,
    UnitLoanNestedForwardEdge,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CascadeProgress {
    Empty,
    InnerPending(NestedForwardState),
    InnerResumed(NestedForwardState),
    OuterPending(NestedForwardState),
    OuterResumed(NestedForwardState),
}
impl ActionSource {
    pub(in super::super::super) fn nested_cascade_continuation(
        &mut self,
        id: usize,
        split: usize,
        successor: Option<NestedForwardState>,
        then_terminated: bool,
        else_terminated: bool,
        clear: bool,
    ) -> Option<ContinuationToken> {
        let c = self.nested_regions.get_mut(id)?;
        let expected_stage = match split {
            1 => 12,
            0 => 14,
            _ => {
                c.valid = false;
                return None;
            }
        };
        let matching = successor.is_some_and(|state| {
            state.plan == c.plan
                && state.coverage == Some(id)
                && state.leaf == Some(2)
                && state.ready_split.is_none()
                && state.path == [c.entries[0], c.entries[2]]
                && c.entries[0].is_some()
                && c.entries[2].is_some()
        });
        // The outer token can only consume the exact state returned by the inner merge.
        let predecessor = match (split, c.cascade, successor) {
            (1, CascadeProgress::Empty, _) => true,
            (0, CascadeProgress::InnerResumed(previous), Some(next)) => previous == next,
            _ => false,
        };
        let good = c.cascade()
            && then_terminated
            && !else_terminated
            && matching
            && predecessor
            && !c.inner_closed
            && !c.inner_consumed
            && !c.resumed
            && c.continuation.is_none();
        if !c.advance(expected_stage, 4, self.records.len(), clear && good) {
            return None;
        }
        let state = successor?;
        c.cascade = if split == 1 {
            CascadeProgress::InnerPending(state)
        } else {
            CascadeProgress::OuterPending(state)
        };
        Some(ContinuationToken {
            identity: self.identity.clone(),
            candidate: id,
            entry: c.entries[if split == 1 { 2 } else { 0 }]?,
            cursor: self.records.len(),
            state,
            cascade_split: Some(split),
        })
    }
    pub(super) fn resume_nested_cascade(
        &mut self,
        token: ContinuationToken,
        clear: bool,
    ) -> Option<NestedForwardState> {
        let c = self.nested_regions.get_mut(token.candidate)?;
        let Some(split) = token.cascade_split else {
            c.valid = false;
            return None;
        };
        let (stage, pending, entry) = match split {
            1 => (13, CascadeProgress::InnerPending(token.state), c.entries[2]),
            0 => (15, CascadeProgress::OuterPending(token.state), c.entries[0]),
            _ => {
                c.valid = false;
                return None;
            }
        };
        let good = c.cascade()
            && Arc::ptr_eq(&token.identity, &self.identity)
            && token.cursor == self.records.len()
            && c.cascade == pending
            && token.state.plan == c.plan
            && token.state.coverage == Some(token.candidate)
            && token.state.leaf == Some(2)
            && token.state.ready_split.is_none()
            && token.state.path == [c.entries[0], c.entries[2]]
            && entry == Some(token.entry)
            && c.entries[0].is_some()
            && c.entries[2].is_some();
        if !c.advance(stage, 4, self.records.len(), clear && good) {
            return None;
        }
        c.cascade = if split == 1 {
            CascadeProgress::InnerResumed(token.state)
        } else {
            CascadeProgress::OuterResumed(token.state)
        };
        Some(token.state)
    }
    pub(super) fn valid_cascade_completion(&self, id: usize, c: &Candidate) -> bool {
        if !c.cascade() {
            return c.cascade == CascadeProgress::Empty;
        }
        let CascadeProgress::OuterResumed(state) = c.cascade else {
            return false;
        };
        state.plan == c.plan
            && state.coverage == Some(id)
            && state.leaf == Some(2)
            && state.ready_split.is_none()
            && state.path == [c.entries[0], c.entries[2]]
            && [(0, 0, 2, None), (2, 1, 4, c.entries[0])].iter().all(
                |&(role, split, offset, parent)| {
                    c.entries[role]
                        .and_then(|i| self.nested_entries.get(i))
                        .is_some_and(|entry| {
                            entry.callable == c.plan.callable
                                && entry.topology == c.plan.topology
                                && entry.flag_layout == c.plan.flag_layout
                                && entry.split == c.plan.splits()[split].split
                                && entry.condition == c.plan.splits()[split].condition
                                && !entry.then_arm
                                && entry.parent == parent
                                && entry.cursor == c.start + offset
                        })
                },
            )
    }
    pub(super) fn cascade_certificate(
        &self,
        c: &Candidate,
    ) -> Option<UnitBorrowNestedForwardCascadeContinuation> {
        let step = |role: usize| {
            let entry = self.nested_entries.get(c.entries[role]?)?;
            Some(UnitBorrowNestedForwardContinuation {
                entry: UnitLoanNestedForwardEdge {
                    split: entry.split,
                    condition: entry.condition,
                    then_arm: entry.then_arm,
                },
                split: entry.split,
                condition: entry.condition,
                tail_control: c.plan.leaves()[2].control,
            })
        };
        Some(UnitBorrowNestedForwardCascadeContinuation {
            inner: step(2)?,
            outer: step(0)?,
        })
    }
}
