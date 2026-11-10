//! Actual parent/child records and same-catalog validation of their carried references.
use super::{ActionHandle, ActionSource, Record};
use crate::{
    name_resolution::{SourceUnitId, UnitSymbolId},
    ownership_checking::{
        BorrowBindingFact, BorrowBindingStorage, LoanKind, UnitDropPoint, UnitLoanAction,
        UnitLoanBindingChildCommit, UnitLoanBindingChildRetirement, UnitLoanBindingChildUseReturn,
        UnitLoanFact, UnitLoanTarget,
    },
    source::SourceId,
    type_checking::UnitExpressionId,
};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug)]
pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) struct Parent {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) binding:
        UnitSymbolId,
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) source:
        UnitLoanFact,
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) source_begin:
        Option<ActionHandle>,
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) handoff:
        Option<ActionHandle>,
}

pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) struct Commit {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fact:
        BorrowBindingFact<UnitExpressionId, UnitSymbolId, UnitLoanTarget>,
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) initializer:
        UnitExpressionId,
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) raw_parent:
        UnitSymbolId,
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) parent: Parent,
}

pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) struct UseReturn {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) binding:
        UnitSymbolId,
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) parent: Parent,
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) commit:
        ActionHandle,
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) loan: UnitLoanFact,
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) begin:
        Option<ActionHandle>,
}

pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) struct Retirement {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) binding:
        UnitSymbolId,
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) parent: Parent,
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) commit:
        ActionHandle,
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) use_return:
        Option<ActionHandle>,
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) binding_end:
        ActionHandle,
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) point:
        UnitDropPoint,
}

impl ActionSource {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn commit_binding_child(
        &mut self,
        fact: Commit,
    ) -> Option<ActionHandle> {
        if !self.enabled {
            return None;
        }
        let handle = self.next_handle();
        self.records.push(Record::BorrowBindingChildCommitted(fact));
        Some(handle)
    }

    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn return_binding_child_use(
        &mut self,
        fact: UseReturn,
    ) -> Option<ActionHandle> {
        if !self.enabled {
            return None;
        }
        let handle = self.next_handle();
        self.records
            .push(Record::BorrowBindingChildUseReturned(fact));
        Some(handle)
    }

    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn retire_binding_child(
        &mut self,
        fact: Retirement,
    ) -> Option<ActionHandle> {
        if !self.enabled {
            return None;
        }
        let handle = self.next_handle();
        self.records.push(Record::BorrowBindingChildRetired(fact));
        Some(handle)
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct Links {
    pub(super) source_begin: Option<usize>,
    pub(super) parent_handoff: Option<usize>,
    pub(super) commit: Option<usize>,
    pub(super) begin: Option<usize>,
    pub(super) use_return: Option<usize>,
    pub(super) binding_end: Option<usize>,
}

impl Commit {
    pub(super) fn into_action(self, identity: Arc<()>, links: Links) -> UnitLoanAction {
        UnitLoanAction::BorrowBindingChildCommitted(UnitLoanBindingChildCommit {
            identity,
            binding: self.fact.binding(),
            parent: self.parent.binding,
            initializer: self.initializer,
            source_begin: links.source_begin,
            parent_handoff: links.parent_handoff,
        })
    }
}
impl UseReturn {
    pub(super) fn into_action(self, identity: Arc<()>, links: Links) -> UnitLoanAction {
        UnitLoanAction::BorrowBindingChildUseReturned(UnitLoanBindingChildUseReturn {
            identity,
            binding: self.binding,
            parent: self.parent.binding,
            loan: self.loan,
            commit: links.commit,
            begin: links.begin,
            source_begin: links.source_begin,
            parent_handoff: links.parent_handoff,
        })
    }
}
impl Retirement {
    pub(super) fn into_action(self, identity: Arc<()>, links: Links) -> UnitLoanAction {
        UnitLoanAction::BorrowBindingChildRetired(UnitLoanBindingChildRetirement {
            identity,
            binding: self.binding,
            parent: self.parent.binding,
            point: self.point,
            commit: links.commit,
            use_return: links.use_return,
            binding_end: links.binding_end,
            source_begin: links.source_begin,
            parent_handoff: links.parent_handoff,
        })
    }
}

struct Catalog<'a> {
    records: &'a [Record],
    identity: &'a Arc<()>,
    source: SourceUnitId,
    file: SourceId,
}
impl Catalog<'_> {
    fn earlier(&self, handle: &ActionHandle, before: usize) -> Option<usize> {
        (Arc::ptr_eq(&handle.identity, self.identity) && handle.index < before)
            .then_some(handle.index)
    }

    fn local_shared(&self, loan: &UnitLoanFact) -> bool {
        loan.kind() == LoanKind::Shared
            && loan.call().source_unit() == self.source
            && loan.argument().source_unit() == self.source
            && loan.begin_span().source_id() == self.file
            && loan.end_span().source_id() == self.file
            && matches!(loan.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root().source_unit() == self.source)
    }

    fn parent(&self, parent: &Parent, before: usize) -> Option<Links> {
        let handoff = self.earlier(parent.handoff.as_ref()?, before)?;
        let begin = self.earlier(parent.source_begin.as_ref()?, handoff)?;
        if parent.binding.source_unit() != self.source
            || !self.local_shared(&parent.source)
            || !matches!(self.records.get(begin), Some(Record::Begin(loan)) if *loan == parent.source)
            || !matches!(self.records.get(handoff), Some(Record::BorrowHandoffCommitted(fact)) if fact.binding() == parent.binding && *fact.source_loan() == parent.source)
            || matches!(parent.source.target(), UnitLoanTarget::Place(place) if place.root() == parent.binding)
        {
            return None;
        }
        Some(Links {
            source_begin: Some(begin),
            parent_handoff: Some(handoff),
            ..Links::default()
        })
    }

    fn commit(&self, fact: &Commit, index: usize) -> Option<Links> {
        let links = self.parent(&fact.parent, index)?;
        (fact.raw_parent == fact.parent.binding
            && fact.fact.parent() == Some(fact.parent.binding)
            && fact.fact.binding() != fact.parent.binding
            && fact.fact.binding().source_unit() == self.source
            && fact.fact.initializer() == fact.initializer
            && fact.initializer.source_unit() == self.source
            && fact.fact.marker_span().source_id() == self.file
            && fact.fact.source_loan().is_none()
            && fact.fact.storage() == BorrowBindingStorage::BorrowedStorage
            && fact.fact.origin() == fact.parent.source.target()
            && !matches!(fact.fact.origin(), UnitLoanTarget::Place(place) if place.root() == fact.fact.binding()))
            .then_some(links)
    }

    fn carried_commit(
        &self,
        handle: &ActionHandle,
        binding: UnitSymbolId,
        parent: &Parent,
        before: usize,
        prior: &BTreeMap<usize, Links>,
    ) -> Option<Links> {
        let index = self.earlier(handle, before)?;
        let Record::BorrowBindingChildCommitted(committed) = self.records.get(index)? else {
            return None;
        };
        let links = *prior.get(&index)?;
        if committed.fact.binding() != binding || !same_parent(parent, &committed.parent) {
            return None;
        }
        // Revalidate current carried source handles as well as the original commit.
        let actual = self.parent(parent, index)?;
        if actual.source_begin != links.source_begin
            || actual.parent_handoff != links.parent_handoff
        {
            return None;
        }
        Some(Links {
            commit: Some(index),
            ..links
        })
    }

    fn use_return(
        &self,
        fact: &UseReturn,
        index: usize,
        prior: &BTreeMap<usize, Links>,
    ) -> Option<Links> {
        let begin = self.earlier(fact.begin.as_ref()?, index)?;
        let links = self.carried_commit(&fact.commit, fact.binding, &fact.parent, begin, prior)?;
        (self.local_shared(&fact.loan)
            && fact.loan.call() != fact.parent.source.call()
            && fact.loan.target() == fact.parent.source.target()
            && matches!(self.records.get(begin), Some(Record::Begin(loan)) if *loan == fact.loan))
        .then_some(Links {
            begin: Some(begin),
            ..links
        })
    }

    fn retirement(
        &self,
        fact: &Retirement,
        index: usize,
        prior: &BTreeMap<usize, Links>,
    ) -> Option<Links> {
        let end = self.earlier(&fact.binding_end, index)?;
        if end.checked_add(1) != Some(index)
            || !matches!(self.records.get(end), Some(Record::BorrowBindingEnd(actual)) if actual.binding() == fact.binding && actual.point() == fact.point)
        {
            return None;
        }
        let links = self.carried_commit(&fact.commit, fact.binding, &fact.parent, end, prior)?;
        let use_return = fact
            .use_return
            .as_ref()
            .and_then(|handle| self.earlier(handle, end))
            .filter(|&returned| {
                let Some(Record::BorrowBindingChildUseReturned(actual)) =
                    self.records.get(returned)
                else {
                    return false;
                };
                actual.binding == fact.binding
                    && actual.commit.same_record(&fact.commit)
                    && same_parent(&actual.parent, &fact.parent)
                    && prior
                        .get(&returned)
                        .is_some_and(|used| used.begin.is_some() && used.commit == links.commit)
            });
        Some(Links {
            use_return,
            binding_end: Some(end),
            ..links
        })
    }
}

fn same_parent(left: &Parent, right: &Parent) -> bool {
    let same = |a: &Option<ActionHandle>, b: &Option<ActionHandle>| {
        a.as_ref()
            .zip(b.as_ref())
            .is_some_and(|(a, b)| a.same_record(b))
    };
    left.binding == right.binding
        && left.source == right.source
        && same(&left.source_begin, &right.source_begin)
        && same(&left.handoff, &right.handoff)
}

pub(super) fn validate(
    records: &[Record],
    identity: &Arc<()>,
    source: SourceUnitId,
    file: SourceId,
) -> BTreeMap<usize, Links> {
    let catalog = Catalog {
        records,
        identity,
        source,
        file,
    };
    let mut links = BTreeMap::new();
    for (index, record) in records.iter().enumerate() {
        let checked = match record {
            Record::BorrowBindingChildCommitted(fact) => catalog.commit(fact, index),
            Record::BorrowBindingChildUseReturned(fact) => catalog.use_return(fact, index, &links),
            Record::BorrowBindingChildRetired(fact) => catalog.retirement(fact, index, &links),
            _ => None,
        };
        if let Some(checked) = checked {
            links.insert(index, checked);
        }
    }
    links
}

#[cfg(test)]
mod tests;
