//! Separate evidence publication; it never appends legacy action records.
use super::*;
use crate::{
    ownership_checking::compilation_unit::d10_catalog_experiment::EntryBindingEvidence,
    ownership_checking::{UnitBorrowBindingTransportFact, UnitLoanActionId},
    type_checking::{UnitExpressionId, UnitItemId},
};
pub(in crate::ownership_checking::compilation_unit::dataflow::drop_planner) struct Retiring<'a> {
    pub callable: UnitItemId,
    pub consumer: UnitExpressionId,
    pub transaction: &'a UnitBorrowBindingTransportFact,
    pub source: Option<&'a ActionHandle>,
    pub handoff: Option<&'a ActionHandle>,
    pub returned: Option<&'a (UnitLoanFact, Option<ActionHandle>)>,
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
            consumer,
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
        let use_id = returned.and_then(|(loan, handle)| {
            handle.as_ref().and_then(|h| {
                (handoff.is_some_and(|p| p.precedes(h))
                    && h.precedes(end)
                    && matches!(self.records.get(h.index), Some(Record::Begin(f)) if f == loan))
                .then(|| original(h))
                .flatten()
            })
        });
        let end_id = (handoff.is_some_and(|h| h.precedes(end)) && matches!(self.records.get(end.index), Some(Record::BorrowBindingEnd(f)) if f.binding() == transaction.binding() && f.point() == transaction.point()))
            .then(|| original(end)).flatten();
        self.entry_binding_evidence.push(EntryBindingEvidence {
            identity: self.identity.clone(),
            callable,
            consumer,
            transaction: transaction.clone(),
            returned: returned.map(|(loan, _)| loan.clone()),
            source_begin: source_id,
            handoff: handoff_id,
            use_begin: use_id,
            binding_end: end_id,
            ambiguous_use,
        });
    }
}
