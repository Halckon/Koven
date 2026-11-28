//! Shared cleanup lookups for ordinary subset admission.
use lang_frontend::{
    name_resolution::{SourceUnitId, ValidatedCompilationUnitNames},
    ownership_checking::{
        UnitDirectCallLoanEndFact, UnitDropFact, UnitDropPoint, UnitDropTarget, UnitLoanFact,
    },
    source::{SourceId, Span},
    type_checking::UnitExpressionId,
};
use std::{cell::OnceCell, collections::BTreeMap};

type UnwindKey = (UnitExpressionId, UnitExpressionId, UnitExpressionId);
type UnwindIndex = BTreeMap<UnwindKey, Vec<(usize, usize)>>;
type TemporaryUnwindIndex = BTreeMap<(UnitExpressionId, UnitExpressionId), Vec<Span>>;

#[derive(Default)]
pub(in crate::ssa) struct SubsetDropIndex {
    drop_origins: OnceCell<Option<Vec<(usize, usize)>>>,
    direct_unwinds: OnceCell<UnwindIndex>,
    temporary_unwinds: OnceCell<TemporaryUnwindIndex>,
}

impl SubsetDropIndex {
    /// Preselect ordinary drops only for a Unit with one exact physical source.
    pub(super) fn candidates(
        &self,
        names: &ValidatedCompilationUnitNames,
        unit: SourceUnitId,
        scope: Span,
        facts: &[UnitDropFact],
    ) -> Option<Vec<usize>> {
        let [source] = names.names().index().source_units() else {
            return None;
        };
        if source.id() != unit || source.source_id() != scope.source_id() {
            return None;
        }
        let sorted = self
            .drop_origins
            .get_or_init(|| {
                build(
                    source.source_id(),
                    facts.iter().map(|fact| fact.value_origin()),
                )
            })
            .as_ref()?;
        Some(select(sorted, scope))
    }

    /// Use the source-qualified edge and operand identity before comparing the full loan.
    pub(super) fn ends_before_drops(
        index: Option<&Self>,
        facts: [&[UnitDirectCallLoanEndFact]; 2],
        transfer: UnitExpressionId,
        loan: &UnitLoanFact,
    ) -> bool {
        let matches = |fact: &UnitDirectCallLoanEndFact| {
            fact.point() == UnitDropPoint::ControlTransfer(transfer)
                && fact.loan() == loan
                && fact.preceding_drops() == 0
        };
        let Some(index) = index else {
            return facts.into_iter().flatten().any(matches);
        };
        let rows = index.direct_unwinds.get_or_init(|| {
            let mut rows = UnwindIndex::new();
            for (table, facts) in facts.iter().enumerate() {
                for (index, end) in facts.iter().enumerate() {
                    if let UnitDropPoint::ControlTransfer(transfer) = end.point()
                        && end.preceding_drops() == 0
                    {
                        rows.entry((transfer, end.loan().call(), end.loan().argument()))
                            .or_default()
                            .push((table, index));
                    }
                }
            }
            rows
        });
        rows.get(&(transfer, loan.call(), loan.argument()))
            .is_some_and(|indices| {
                indices
                    .iter()
                    .any(|&(table, index)| matches(&facts[table][index]))
            })
    }

    /// Find the exact unconditional drop for a completed temporary on this edge.
    pub(super) fn unconditional_temporary_drop(
        index: Option<&Self>,
        facts: &[UnitDropFact],
        transfer: UnitExpressionId,
        owner: UnitExpressionId,
        origin: Span,
    ) -> bool {
        let key = (transfer, owner);
        let eligible = |fact: &UnitDropFact| {
            let (UnitDropPoint::ControlTransfer(transfer), UnitDropTarget::Temporary(owner)) =
                (fact.point(), fact.target())
            else {
                return None;
            };
            (!fact.is_conditional_temporary()).then_some((transfer, owner))
        };
        let Some(index) = index else {
            return facts
                .iter()
                .any(|fact| eligible(fact) == Some(key) && fact.value_origin() == origin);
        };
        let rows = index.temporary_unwinds.get_or_init(|| {
            let mut rows = TemporaryUnwindIndex::new();
            for fact in facts {
                if let Some(key) = eligible(fact) {
                    rows.entry(key).or_default().push(fact.value_origin());
                }
            }
            rows
        });
        rows.get(&key)
            .is_some_and(|origins| origins.contains(&origin))
    }
}

fn build(source: SourceId, origins: impl Iterator<Item = Span>) -> Option<Vec<(usize, usize)>> {
    let mut sorted = Vec::new();
    for (ordinal, origin) in origins.enumerate() {
        #[cfg(test)]
        super::receiver_receipt::work_count::visit(|c| c.subset_drop_index_rows += 1);
        // Unexpected origins do not justify skipping anything. Retain the old scan.
        if origin.source_id() != source {
            return None;
        }
        sorted.push((origin.start(), ordinal));
    }
    sorted.sort_unstable();
    Some(sorted)
}

fn select(sorted: &[(usize, usize)], scope: Span) -> Vec<usize> {
    let start = sorted.partition_point(|row| row.0 < scope.start());
    let end = sorted.partition_point(|row| row.0 <= scope.end());
    let mut matching: Vec<_> = sorted[start..end].iter().map(|row| row.1).collect();
    // Preserve exact observation/short-circuit order even when origins are unsorted.
    matching.sort_unstable();
    matching
}

#[test]
fn subset_drop_candidates_keep_ordinals_and_refuse_foreign_physical_origin() {
    use lang_frontend::source::SourceMap;
    let mut sources = SourceMap::new();
    let source = sources.add_source("same.ko", "abcdefghij").unwrap();
    let mut other = SourceMap::new();
    let foreign = other.add_source("same.ko", "abcdefghij").unwrap();
    let at = |i, j| sources.span(source, i, j).unwrap();
    let origins = [at(6, 7), at(1, 2), at(3, 9), at(3, 9), at(8, 8), at(7, 7)];
    assert_eq!(
        select(&build(source, origins.into_iter()).unwrap(), at(3, 7)),
        // Exact end/containment is checked by the original predicate, not at this stage.
        [0, 2, 3, 5]
    );
    assert!(
        build(
            source,
            [at(3, 4), other.span(foreign, 3, 4).unwrap()].into_iter()
        )
        .is_none()
    );
}
