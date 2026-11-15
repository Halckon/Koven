//! Separate evidence publication; it never appends legacy action records.
use super::*;
use crate::{
    ownership_checking::compilation_unit::d10_catalog_experiment::{
        EntryBindingEvidence, EntryBindingScalar, EntryBindingUse,
    },
    ownership_checking::{UnitBorrowBindingTransportFact, UnitLoanActionId},
    type_checking::{UnitExpressionId, UnitItemId},
};
pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) struct Retiring<'a> {
    pub callable: UnitItemId,
    pub consumers: &'a [UnitExpressionId],
    pub scalar: Option<&'a EntryBindingScalar>,
    pub transaction: &'a UnitBorrowBindingTransportFact,
    pub source: Option<&'a ActionHandle>,
    pub handoff: Option<&'a ActionHandle>,
    pub returned: &'a [(UnitLoanFact, Option<ActionHandle>)],
    pub ambiguous_use: bool,
    pub end: &'a ActionHandle,
}
impl ActionSource {
    pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) fn entry_binding_retired(
        &mut self,
        retiring: Retiring<'_>,
    ) {
        let Retiring {
            callable,
            consumers,
            scalar,
            transaction,
            source,
            handoff,
            returned,
            ambiguous_use,
            end,
        } = retiring;
        let original = |handle: &ActionHandle| {
            (Arc::ptr_eq(&handle.identity, &self.identity) && handle.index < self.records.len())
                .then(|| UnitLoanActionId {
                    identity: self.identity.clone(),
                    index: handle.index,
                })
        };
        let source_id = source.and_then(|h| {
            matches!(self.records.get(h.index), Some(Record::Begin(loan)) if loan == transaction.source_loan())
                .then(|| original(h)).flatten()
        });
        let handoff_id = handoff.and_then(|h| {
            (source.is_some_and(|s| s.precedes(h)) && matches!(self.records.get(h.index), Some(Record::BorrowHandoffCommitted(f)) if f.binding() == transaction.binding() && f.source_loan() == transaction.source_loan()))
                .then(|| original(h)).flatten()
        });
        let Some(&consumer) = consumers.first() else {
            return;
        };
        let mut previous = handoff;
        let uses:Vec<_>=consumers.iter().enumerate().map(|(index,&consumer)| {
            let actual=returned.get(index).filter(|(loan,_)|loan.call()==consumer);
            let begin=actual.and_then(|(loan,handle)|handle.as_ref().and_then(|h| {
                (previous.is_some_and(|p|p.precedes(h)) && h.precedes(end) && matches!(self.records.get(h.index),Some(Record::Begin(f)) if f==loan)).then(||original(h)).flatten()
            }));
            previous=actual.and_then(|(_,h)|h.as_ref());
            EntryBindingUse {consumer,returned:actual.map(|(f,_)|f.clone()),begin}
        }).collect();
        let ambiguous_use = ambiguous_use
            || returned.len() != consumers.len()
            || uses
                .iter()
                .any(|u| u.returned.is_none() || u.begin.is_none());
        let end_id = (handoff.is_some_and(|h| h.precedes(end)) && matches!(self.records.get(end.index), Some(Record::BorrowBindingEnd(f)) if f.binding() == transaction.binding() && f.point() == transaction.point()))
            .then(|| original(end)).flatten();
        self.entry_binding_evidence.push(EntryBindingEvidence {
            identity: self.identity.clone(),
            callable,
            consumer,
            transaction: transaction.clone(),
            uses,
            scalar: scalar.cloned(),
            source_begin: source_id,
            handoff: handoff_id,
            binding_end: end_id,
            ambiguous_use,
        });
    }
}
