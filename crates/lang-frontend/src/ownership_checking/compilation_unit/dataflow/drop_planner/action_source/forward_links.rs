//! Validate only the source acquisition actually carried to live wrapper delivery.
use super::Record;
use crate::{
    name_resolution::SourceUnitId,
    ownership_checking::{LoanKind, UnitDropPoint, UnitLoanTarget},
    source::SourceId,
    type_checking::UnitItemId,
};
use std::{collections::BTreeMap, sync::Arc};

pub(super) fn validate(
    records: &[Record],
    identity: &Arc<()>,
    source_unit: SourceUnitId,
    file: SourceId,
) -> BTreeMap<usize, (usize, UnitItemId)> {
    let mut links = BTreeMap::new();
    for (index, record) in records.iter().enumerate() {
        let Record::BorrowForwardHandoff {
            transaction,
            cause: Some(cause),
            completed: Some(completed),
        } = record
        else {
            continue;
        };
        let handle = &cause.acquisition;
        let scope = &cause.scope;
        let source = transaction.source_loan();
        if !Arc::ptr_eq(&handle.identity, identity) || handle.index >= index
            || scope != completed || scope.callable.source_unit() != source_unit
            || scope.body != source.call() || scope.argument != source.argument()
            || transaction.returned_expression() != scope.body
            || transaction.point() != UnitDropPoint::ControlTransfer(scope.body)
            || source.kind() != LoanKind::Shared
            || !matches!(source.target(), UnitLoanTarget::Place(place)
                if place.is_root() && place.root() == scope.root && scope.root.source_unit() == source_unit)
            || [source.call(), source.argument(), scope.body, scope.argument].iter().any(|id| id.source_unit() != source_unit)
            // The selected callee parameter span may legitimately belong to another file.
            || [source.begin_span(), source.end_span()].iter().any(|span| span.source_id() != file)
        {
            continue;
        }
        if matches!(records.get(handle.index), Some(Record::Begin(actual)) if actual == source) {
            links.insert(index, (handle.index, scope.callable));
        }
    }
    links
}

#[cfg(test)]
mod tests;
