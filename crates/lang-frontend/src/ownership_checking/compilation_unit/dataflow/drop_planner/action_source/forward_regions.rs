//! Actual entry, delivery and completion of one independently scoped live exit.
use super::super::borrow_forward_transport::ForwardScope;
use super::{ActionHandle, ActionSource, Record};
use crate::ownership_checking::{UnitBorrowForwardRegion, UnitLoanActionCoverage};
use crate::type_checking::UnitItemId;
use std::{collections::BTreeMap, sync::Arc};

pub(super) struct Candidate {
    scope: ForwardScope,
    start: usize,
    end: Option<usize>,
    pre_clear: bool,
    post_clear: bool,
    handoff: Option<ActionHandle>,
    ambiguous: bool,
}

impl ActionSource {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn enter_forward_region(
        &mut self,
        mut scope: ForwardScope,
    ) -> ForwardScope {
        if scope.branch.is_some() || scope.branch_entry.is_some() || scope.control != scope.body {
            return scope;
        }
        scope.region = Some(self.forward_regions.len());
        self.forward_regions.push(Candidate {
            scope,
            start: self.records.len(),
            end: None,
            pre_clear: false,
            post_clear: false,
            handoff: None,
            ambiguous: false,
        });
        scope
    }

    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn complete_forward_region(
        &mut self,
        scope: Option<ForwardScope>,
        pre: bool,
        post: bool,
    ) {
        let Some(scope) = scope else {
            return;
        };
        let Some(candidate) = scope.region.and_then(|i| self.forward_regions.get_mut(i)) else {
            return;
        };
        if candidate.scope != scope || candidate.end.is_some() {
            candidate.ambiguous = true;
            return;
        }
        candidate.end = Some(self.records.len());
        candidate.pre_clear = pre;
        candidate.post_clear = post;
    }

    pub(super) fn record_forward_region_handoff(
        &mut self,
        scope: Option<ForwardScope>,
        handle: ActionHandle,
    ) {
        let Some(scope) = scope else {
            return;
        };
        let Some(candidate) = scope.region.and_then(|i| self.forward_regions.get_mut(i)) else {
            return;
        };
        if candidate.scope != scope || candidate.handoff.is_some() || candidate.end.is_some() {
            candidate.ambiguous = true;
            return;
        }
        candidate.handoff = Some(handle);
    }

    pub(super) fn validate_forward_regions(
        &self,
        links: &BTreeMap<usize, (usize, UnitItemId)>,
    ) -> Vec<UnitBorrowForwardRegion> {
        self.forward_regions.iter().map(|candidate| {
            let scope = candidate.scope;
            let end = candidate.end.unwrap_or(candidate.start);
            let handoff = candidate.handoff.as_ref();
            let complete = scope.branch.is_none() && scope.branch_entry.is_none() && scope.control == scope.body
                && !candidate.ambiguous && candidate.pre_clear && candidate.post_clear
                && candidate.end.is_some() && end == candidate.start + 2
                && handoff.is_some_and(|handle| Arc::ptr_eq(&handle.identity, &self.identity)
                    && handle.index == candidate.start + 1
                    && links.get(&handle.index) == Some(&(candidate.start, scope.callable)))
                && matches!(self.records.get(candidate.start..end),
                    Some([Record::Begin(_), Record::BorrowForwardHandoff { cause: Some(cause), completed: Some(completed), .. }])
                    if cause.scope == scope && *completed == scope
                    && cause.acquisition.index == candidate.start
                    && Arc::ptr_eq(&cause.acquisition.identity, &self.identity));
            UnitBorrowForwardRegion {
                identity: self.identity.clone(), callable: scope.callable, parameter: scope.root,
                body: scope.body,
                coverage: if complete { UnitLoanActionCoverage::Complete } else { UnitLoanActionCoverage::Partial },
                records: candidate.start..end,
                live_handoff: complete.then_some(candidate.start + 1),
            }
        }).collect()
    }
}

#[cfg(test)]
mod tests;
