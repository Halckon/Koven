//! Ordinary subset preselection, only for a Unit with one exact physical source.
use lang_frontend::{
    name_resolution::{SourceUnitId, ValidatedCompilationUnitNames},
    ownership_checking::UnitDropFact,
    source::{SourceId, Span},
};
use std::cell::OnceCell;

#[derive(Default)]
pub(in crate::ssa) struct SubsetDropIndex(OnceCell<Option<Vec<(usize, usize)>>>);

impl SubsetDropIndex {
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
            .0
            .get_or_init(|| {
                build(
                    source.source_id(),
                    facts.iter().map(|fact| fact.value_origin()),
                )
            })
            .as_ref()?;
        Some(select(sorted, scope))
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
