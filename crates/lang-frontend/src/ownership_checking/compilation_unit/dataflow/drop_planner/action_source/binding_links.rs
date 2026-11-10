//! Validate only carried references to actual records; never search equal descriptors.
use super::{ActionHandle, Record};
use crate::{
    name_resolution::{SourceUnitId, UnitSymbolId},
    ownership_checking::{LoanKind, UnitBorrowBindingTransportFact, UnitLoanFact, UnitLoanTarget},
    source::SourceId,
};
use std::{collections::BTreeMap, sync::Arc};

pub(super) struct UseReturn {
    pub(super) binding: UnitSymbolId,
    pub(super) loan: UnitLoanFact,
    pub(super) source_begin: Option<ActionHandle>,
    pub(super) handoff: Option<ActionHandle>,
    pub(super) begin: Option<ActionHandle>,
}

pub(super) struct SourceRetirement {
    pub(super) transaction: UnitBorrowBindingTransportFact,
    pub(super) source_begin: Option<ActionHandle>,
    pub(super) handoff: Option<ActionHandle>,
    pub(super) use_return: Option<ActionHandle>,
    pub(super) binding_end: ActionHandle,
}

#[derive(Clone, Copy, Default)]
pub(super) struct Links {
    pub(super) source_begin: Option<usize>,
    pub(super) handoff: Option<usize>,
    pub(super) begin: Option<usize>,
    pub(super) use_return: Option<usize>,
    pub(super) binding_end: Option<usize>,
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

    fn source_handoff(
        &self,
        binding: UnitSymbolId,
        source_begin: Option<&ActionHandle>,
        handoff: Option<&ActionHandle>,
        before: usize,
    ) -> Option<(usize, usize, &UnitLoanFact)> {
        let handoff = self.earlier(handoff?, before)?;
        let source_begin = self.earlier(source_begin?, handoff)?;
        let Record::BorrowHandoffCommitted(commit) = self.records.get(handoff)? else {
            return None;
        };
        let Record::Begin(source) = self.records.get(source_begin)? else {
            return None;
        };
        (binding.source_unit() == self.source
            && commit.binding() == binding
            && commit.source_loan() == source
            && self.local_shared(source))
        .then_some((source_begin, handoff, source))
    }

    fn use_return(&self, fact: &UseReturn, index: usize) -> Option<Links> {
        let begin = self.earlier(fact.begin.as_ref()?, index)?;
        let (source_begin, handoff, source) = self.source_handoff(
            fact.binding,
            fact.source_begin.as_ref(),
            fact.handoff.as_ref(),
            begin,
        )?;
        (matches!(self.records.get(begin), Some(Record::Begin(loan)) if *loan == fact.loan)
            && self.local_shared(&fact.loan)
            && fact.loan.target() == source.target()
            && fact.loan.call() != source.call())
        .then_some(Links {
            source_begin: Some(source_begin),
            handoff: Some(handoff),
            begin: Some(begin),
            ..Links::default()
        })
    }

    fn retirement(
        &self,
        fact: &SourceRetirement,
        index: usize,
        prior: &BTreeMap<usize, Links>,
    ) -> Option<Links> {
        let end = self.earlier(&fact.binding_end, index)?;
        // The actual removal site publishes these two observations consecutively.
        if end.checked_add(1) != Some(index) {
            return None;
        }
        let Record::BorrowBindingEnd(binding_end) = self.records.get(end)? else {
            return None;
        };
        if binding_end.binding() != fact.transaction.binding()
            || binding_end.point() != fact.transaction.point()
        {
            return None;
        }
        let (source_begin, handoff, source) = self.source_handoff(
            fact.transaction.binding(),
            fact.source_begin.as_ref(),
            fact.handoff.as_ref(),
            end,
        )?;
        if source != fact.transaction.source_loan() {
            return None;
        }
        let use_return = fact.use_return.as_ref()
            .and_then(|handle| self.earlier(handle, end))
            .filter(|&use_index| {
                matches!(self.records.get(use_index), Some(Record::BorrowBindingUseReturned(returned)) if returned.binding == fact.transaction.binding())
                    && prior.get(&use_index).is_some_and(|links| {
                        links.source_begin == Some(source_begin) && links.handoff == Some(handoff)
                            && links.begin.is_some()
                    })
            });
        Some(Links {
            source_begin: Some(source_begin),
            handoff: Some(handoff),
            use_return,
            binding_end: Some(end),
            ..Links::default()
        })
    }
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
            Record::BorrowBindingUseReturned(fact) => catalog.use_return(fact, index),
            Record::BorrowBindingSourceRetired(fact) => catalog.retirement(fact, index, &links),
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
