//! Independent validation of actual transport handles, never descriptor-based lookup.
use super::Record;
use crate::{
    name_resolution::SourceUnitId,
    ownership_checking::{LoanKind, UnitLoanTarget},
    source::SourceId,
};
use std::{collections::BTreeMap, sync::Arc};

pub(super) fn validate(
    records: &[Record],
    identity: &Arc<()>,
    source_unit: SourceUnitId,
    file: SourceId,
) -> BTreeMap<usize, usize> {
    let mut links = BTreeMap::new();
    for (index, record) in records.iter().enumerate() {
        let Record::BorrowConsumerRetired {
            transfer: Some(handle),
            transaction,
            ..
        } = record
        else {
            continue;
        };
        if handle.index >= index || !Arc::ptr_eq(&handle.identity, identity) {
            continue;
        }
        let Some(Record::BorrowConsumerTransfer {
            source,
            consumer,
            argument,
            ..
        }) = records.get(handle.index)
        else {
            continue;
        };
        let [ended_consumer, ended_source] = transaction.ended_loans();
        let valid = source == ended_source
            && *consumer == ended_consumer.call() && *argument == ended_consumer.argument()
            && source.call() != *consumer && source.kind() == LoanKind::Shared && ended_consumer.kind() == LoanKind::Shared
            && source.target() == ended_consumer.target()
            && matches!(source.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root().source_unit() == source_unit)
            && [source.call(), source.argument(), *consumer, *argument].iter().all(|id| id.source_unit() == source_unit)
            // Callee parameter spans intentionally remain free to refer to other files.
            && [source.begin_span(), source.end_span(), ended_consumer.begin_span(), ended_consumer.end_span()].iter().all(|span| span.source_id() == file);
        if valid {
            links.insert(index, handle.index);
        }
    }
    links
}

/// Follow only carried handles; never discover acquisitions by searching token facts.
pub(super) fn acquisitions(
    records: &[Record],
    identity: &Arc<()>,
    links: &BTreeMap<usize, usize>,
) -> BTreeMap<usize, (usize, usize)> {
    let mut proven = BTreeMap::new();
    for (&retired, &transfer) in links {
        let Record::BorrowConsumerTransfer {
            acquisition: Some(source),
            ..
        } = &records[transfer]
        else {
            continue;
        };
        let Record::BorrowConsumerRetired {
            acquisition: Some(consumer),
            transaction,
            ..
        } = &records[retired]
        else {
            continue;
        };
        if !(Arc::ptr_eq(&source.identity, identity)
            && Arc::ptr_eq(&consumer.identity, identity)
            && source.index < transfer
            && transfer < consumer.index
            && consumer.index < retired)
        {
            continue;
        }
        let [ended_consumer, ended_source] = transaction.ended_loans();
        if matches!(records.get(source.index), Some(Record::Begin(actual)) if actual == ended_source)
            && matches!(records.get(consumer.index), Some(Record::Begin(actual)) if actual == ended_consumer)
        {
            proven.insert(retired, (source.index, consumer.index));
        }
    }
    proven
}

#[cfg(test)]
mod tests;
