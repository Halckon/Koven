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
    branch_entries: &[(crate::ownership_checking::UnitLoanForwardBranch, usize)],
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
            || transaction.point() != UnitDropPoint::ControlTransfer(scope.control)
            || source.kind() != LoanKind::Shared
            || !matches!(source.target(), UnitLoanTarget::Place(place)
                if place.is_root() && place.root() == scope.root && scope.root.source_unit() == source_unit)
            || [source.call(), source.argument(), scope.body, scope.argument, scope.control].iter().any(|id| id.source_unit() != source_unit)
            // The selected callee parameter span may legitimately belong to another file.
            || [source.begin_span(), source.end_span()].iter().any(|span| span.source_id() != file)
        {
            continue;
        }
        let path_valid = match (scope.branch, scope.branch_entry) {
            (None, None) => scope.control == scope.body,
            (Some(branch), Some(entry)) => {
                scope.region.is_none()
                    && branch.callable() == scope.callable
                    && branch.value() == scope.body
                    && branch.control() == scope.control
                    && scope.control != scope.body
                    && [
                        branch.split(),
                        branch.condition(),
                        branch.control(),
                        branch.value(),
                    ]
                    .iter()
                    .all(|id| id.source_unit() == source_unit)
                    && branch_entries.get(entry).is_some_and(|(actual, cursor)| {
                        *actual == branch && *cursor <= handle.index
                    })
                    && branch_entries
                        .iter()
                        .filter(|(actual, _)| *actual == branch)
                        .count()
                        == 1
                    && records
                        .iter()
                        .filter(|record| {
                            matches!(record,
                    Record::BorrowForwardHandoff { cause: Some(other), .. }
                    if other.scope.branch_entry == Some(entry))
                        })
                        .count()
                        == 1
            }
            _ => false,
        };
        if !path_valid {
            continue;
        }
        // Reject every use, not only later uses, when a real acquisition is delivered twice.
        if records.iter().filter(|record| matches!(record,
            Record::BorrowForwardHandoff { cause: Some(other), .. }
            if other.acquisition.index == handle.index && Arc::ptr_eq(&other.acquisition.identity, &handle.identity))).count() != 1 {
            continue;
        }
        if matches!(records.get(handle.index), Some(Record::Begin(actual)) if actual == source) {
            links.insert(index, (handle.index, scope.callable));
        }
    }
    links
}

#[cfg(test)]
mod branch_tests;
#[cfg(test)]
mod tests;
