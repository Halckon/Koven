//! Ordinary subset preselection by checked physical source and callable scope.
use lang_frontend::{
    name_resolution::{SourceUnitId, ValidatedCompilationUnitNames},
    ownership_checking::{UnitDropFact, UnitDropPoint},
    source::{SourceId, Span},
};
use std::cell::OnceCell;

type ScopeRows = Vec<(usize, usize)>;

#[derive(Default)]
pub(in crate::ssa) struct SubsetDropIndex(OnceCell<Option<Vec<ScopeRows>>>);

impl SubsetDropIndex {
    pub(super) fn candidates(
        &self,
        names: &ValidatedCompilationUnitNames,
        unit: SourceUnitId,
        scope: Span,
        facts: &[UnitDropFact],
    ) -> Option<Vec<usize>> {
        let roster = names.names().index().source_units();
        let source = roster.get(unit.index())?;
        if source.id() != unit || source.source_id() != scope.source_id() {
            return None;
        }
        let sorted = self
            .0
            .get_or_init(|| {
                let physical = roster
                    .iter()
                    .map(|source| source.source_id())
                    .collect::<Vec<_>>();
                build(
                    &physical,
                    facts
                        .iter()
                        .map(|fact| (point_source(fact.point()).index(), fact.value_origin())),
                )
            })
            .as_ref()?;
        Some(select(sorted.get(unit.index())?, scope))
    }
}

fn point_source(point: UnitDropPoint) -> SourceUnitId {
    match point {
        UnitDropPoint::AfterExpression(id)
        | UnitDropPoint::AfterBinaryOperands(id)
        | UnitDropPoint::CallReturn(id)
        | UnitDropPoint::ControlTransfer(id)
        | UnitDropPoint::LambdaEntry(id)
        | UnitDropPoint::BeforeReplacement(id)
        | UnitDropPoint::AfterReplacement(id) => id.source_unit(),
        UnitDropPoint::WhenComparisonCompleted { control, .. }
        | UnitDropPoint::WhenAlternativeNoMatch { control, .. }
        | UnitDropPoint::WhenAlternativeMatch { control, .. }
        | UnitDropPoint::BranchEntry { control, .. }
        | UnitDropPoint::BranchExit { control, .. } => control.source_unit(),
        UnitDropPoint::AfterStatement(id) | UnitDropPoint::LoopExit(id) => id.source_unit(),
        UnitDropPoint::FunctionEntry(id) => id.source_unit(),
    }
}

fn build(
    sources: &[SourceId],
    origins: impl Iterator<Item = (usize, Span)>,
) -> Option<Vec<ScopeRows>> {
    let mut bins = vec![Vec::new(); sources.len()];
    for (ordinal, (unit, origin)) in origins.enumerate() {
        #[cfg(test)]
        super::receiver_receipt::work_count::visit(|c| c.subset_drop_index_rows += 1);
        // Validate the complete physical id (including its SourceMap owner).
        // Unexpected origins require the old whole-product predicate.
        if sources.get(unit) != Some(&origin.source_id()) {
            return None;
        }
        bins[unit].push((origin.start(), ordinal));
    }
    for bin in &mut bins {
        bin.sort_unstable();
    }
    Some(bins)
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
    let second = sources.add_source("other.ko", "abcdefghij").unwrap();
    let mut other = SourceMap::new();
    let foreign = other.add_source("same.ko", "abcdefghij").unwrap();
    let at = |i, j| sources.span(source, i, j).unwrap();
    let origins = [at(6, 7), at(1, 2), at(3, 9), at(3, 9), at(8, 8), at(7, 7)];
    let after = sources.span(second, 3, 6).unwrap();
    let before = sources.span(second, 1, 4).unwrap();
    let bins = build(
        &[source, second],
        origins
            .into_iter()
            .map(|span| (0, span))
            .chain([(1, after), (1, before)]),
    )
    .unwrap();
    // Exact end/containment remains the original predicate's job.
    assert_eq!(select(&bins[0], at(3, 7)), [0, 2, 3, 5]);
    assert_eq!(select(&bins[1], after), [6]);
    assert!(
        build(
            &[source, second],
            [(0, at(3, 4)), (1, other.span(foreign, 3, 4).unwrap())].into_iter()
        )
        .is_none()
    );
    assert!(build(&[source, second], [(1, at(3, 4))].into_iter()).is_none());
}
