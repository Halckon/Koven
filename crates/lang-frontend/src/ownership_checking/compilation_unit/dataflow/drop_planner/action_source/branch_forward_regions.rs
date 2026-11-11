//! Three-way proof: declared exits, real control transitions, and unfiltered action intervals.
use super::super::borrow_forward_transport::ForwardScope;
use super::{ActionSource, Record};
use crate::ownership_checking::{
    UnitBorrowBranchForwardPath, UnitBorrowBranchForwardRegion, UnitLoanActionCoverage,
};
use crate::type_checking::UnitItemId;
use std::{collections::BTreeMap, sync::Arc};

pub(super) struct Candidate {
    expected: [ForwardScope; 2],
    base: (usize, usize),
    start: usize,
    condition: Option<(usize, bool)>,
    split: Option<usize>,
    arms: [Option<Arm>; 2],
    continuation: bool,
    end: Option<usize>,
    closed: Option<usize>,
    invalid: bool,
}

#[cfg(test)]
mod tests;
struct Arm {
    scope: ForwardScope,
    start: usize,
    pre: Option<usize>,
    end: Option<usize>,
}

impl ActionSource {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn begin_branch_region(
        &mut self,
        expected: [ForwardScope; 2],
        base: (usize, usize),
        clear: bool,
    ) -> usize {
        let id = self.branch_forward_regions.len();
        self.branch_forward_regions.push(Candidate {
            expected,
            base,
            start: self.records.len(),
            condition: None,
            split: None,
            arms: [None, None],
            continuation: false,
            end: None,
            closed: None,
            invalid: !clear,
        });
        id
    }
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn branch_base(
        &self,
        id: usize,
    ) -> Option<(usize, usize)> {
        self.branch_forward_regions.get(id).map(|c| c.base)
    }
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn branch_condition(
        &mut self,
        id: usize,
        after: bool,
        clear: bool,
    ) {
        let Some(c) = self.branch_forward_regions.get_mut(id) else {
            return;
        };
        if after {
            c.invalid |= c.end.is_some()
                || c.split.is_some()
                || !clear
                || c.condition != Some((self.records.len(), true));
            c.split = Some(self.records.len());
        } else {
            c.invalid |=
                c.end.is_some() || c.condition.is_some() || !clear || self.records.len() != c.start;
            c.condition = Some((self.records.len(), clear));
        }
    }
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn branch_arm(
        &mut self,
        id: usize,
        scope: ForwardScope,
        clear: bool,
    ) {
        let Some(c) = self.branch_forward_regions.get_mut(id) else {
            return;
        };
        let Some(branch) = scope.branch else {
            c.invalid = true;
            return;
        };
        let arm = usize::from(!branch.then_arm());
        let mut expected = c.expected[arm];
        expected.branch_entry = scope.branch_entry;
        c.invalid |= c.end.is_some()
            || c.split.is_none()
            || c.arms[arm].is_some()
            || !clear
            || scope != expected
            || scope.branch_entry.is_none();
        c.arms[arm] = Some(Arm {
            scope,
            start: self.records.len(),
            pre: None,
            end: None,
        });
    }
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn branch_region_for_scope(
        &self,
        scope: ForwardScope,
    ) -> Option<usize> {
        let mut found = self
            .branch_forward_regions
            .iter()
            .enumerate()
            .filter(|(_, c)| c.arms.iter().flatten().any(|a| a.scope == scope));
        let id = found.next()?.0;
        found.next().is_none().then_some(id)
    }
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn branch_return(
        &mut self,
        id: usize,
        scope: ForwardScope,
        post: bool,
        clear: bool,
    ) {
        let Some(c) = self.branch_forward_regions.get_mut(id) else {
            return;
        };
        let Some(arm) = c.arms.iter_mut().flatten().find(|a| a.scope == scope) else {
            c.invalid = true;
            return;
        };
        c.invalid |= c.end.is_some() || !clear;
        if post {
            c.invalid |= arm.end.is_some()
                || arm.pre != Some(arm.start + 1)
                || self.records.len() != arm.start + 2;
            arm.end = Some(self.records.len());
        } else {
            c.invalid |=
                arm.pre.is_some() || arm.end.is_some() || self.records.len() != arm.start + 1;
            arm.pre = Some(self.records.len());
        }
    }
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn branch_continuation(
        &mut self,
        id: usize,
        then_terminated: bool,
        scopes: &[Option<ForwardScope>],
        clear: bool,
    ) {
        let Some(c) = self.branch_forward_regions.get_mut(id) else {
            return;
        };
        let valid = c.arms[0].as_ref().is_some_and(|a| a.end.is_some())
            && c.arms[1]
                .as_ref()
                .is_some_and(|a| scopes == [Some(a.scope)]);
        c.invalid |= c.end.is_some() || c.continuation || !then_terminated || !valid || !clear;
        c.continuation = true;
    }
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn seal_branch_region(
        &mut self,
        id: usize,
        terminated: bool,
        clear: bool,
    ) {
        let Some(c) = self.branch_forward_regions.get_mut(id) else {
            return;
        };
        c.invalid |= c.end.is_some() || !terminated || !clear;
        c.end = Some(self.records.len());
    }
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn close_branch_callable(
        &mut self,
        callable: UnitItemId,
    ) {
        for c in self
            .branch_forward_regions
            .iter_mut()
            .filter(|c| c.expected[0].callable == callable)
        {
            c.invalid |= c.closed.is_some() || c.end != Some(self.records.len());
            c.closed = Some(self.records.len());
        }
    }
    pub(super) fn validate_branch_regions(
        &self,
        links: &BTreeMap<usize, (usize, UnitItemId)>,
    ) -> Vec<UnitBorrowBranchForwardRegion> {
        self.branch_forward_regions.iter().filter_map(|c| {
            let branch = c.expected[0].branch?;
            let left = c.expected[0];
            let right = c.expected[1];
            let declaration = right.branch.is_some_and(|other| {
                branch.then_arm() && !other.then_arm()
                    && branch.callable() == left.callable && other.callable() == right.callable
                    && left.callable == right.callable && left.root == right.root
                    && branch.split() == other.split() && branch.condition() == other.condition()
                    && branch.control() == left.control && other.control() == right.control
                    && branch.value() == left.body && other.value() == right.body
                    && left.control != right.control && left.body != right.body
                    && left.control != left.body && right.control != right.body
                    && left.region.is_none() && right.region.is_none()
                    && left.branch_entry.is_none() && right.branch_entry.is_none()
                    && self.branch_forward_regions.iter().filter(|other| {
                        other.expected[0].callable == left.callable
                            && other.expected[0].branch.is_some_and(|b| b.split() == branch.split())
                    }).count() == 1
            });
            let complete = declaration && !c.invalid && c.continuation && c.end.is_some() && c.closed == c.end
                && c.condition == Some((c.start, true)) && c.split == Some(c.start)
                && c.arms[0].as_ref().is_some_and(|a| a.start == c.start)
                && c.arms[0].as_ref().zip(c.arms[1].as_ref()).is_some_and(|(a,b)| a.end == Some(b.start) && b.end == c.end)
                && c.arms.iter().enumerate().all(|(i, arm)| arm.as_ref().is_some_and(|a| {
                    let mut expected = c.expected[i]; expected.branch_entry = a.scope.branch_entry;
                    a.scope == expected && a.scope.region.is_none()
                        && a.pre == Some(a.start + 1) && a.end == Some(a.start + 2)
                        && links.get(&(a.start + 1)) == Some(&(a.start, a.scope.callable))
                        && matches!(self.records.get(a.start..a.start+2), Some([Record::Begin(_), Record::BorrowForwardHandoff { cause: Some(cause), completed: Some(done), .. }])
                            if cause.scope == a.scope && *done == a.scope && cause.acquisition.index == a.start && Arc::ptr_eq(&cause.acquisition.identity, &self.identity))
                }));
            let scope = c.expected[0];
            Some(UnitBorrowBranchForwardRegion {
                identity: self.identity.clone(), callable: scope.callable, parameter: scope.root,
                split: branch.split(), condition: branch.condition(),
                coverage: if complete { UnitLoanActionCoverage::Complete } else { UnitLoanActionCoverage::Partial },
                paths: if complete { c.arms.iter().flatten().filter_map(|a| Some(UnitBorrowBranchForwardPath {
                    identity: self.identity.clone(), branch: a.scope.branch?, records: a.start..a.start+2,
                })).collect() } else { Vec::new() },
            })
        }).collect()
    }
}
