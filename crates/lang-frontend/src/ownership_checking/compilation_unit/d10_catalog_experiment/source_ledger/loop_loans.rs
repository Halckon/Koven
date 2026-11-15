//! Partial loop traversal provenance only; not a fixed-point or cleanup authority.
use super::*;
use crate::ownership_checking::{UnitLoanAction, UnitLoanActionCoverage};
impl CompilationUnitOwnership {
    pub(crate) fn d10_loop_link(
        &self,
        statement: crate::type_checking::UnitStatementId,
    ) -> Result<UnitItemId, CallableLoanLinkError> {
        use CallableLoanLinkError::{Ambiguous, Unsupported};
        let ledger = self.source_collection_ledger.as_ref().ok_or(Unsupported)?;
        if !ledger.matches_original(self) {
            return Err(Unsupported);
        }
        let candidates: Vec<_> = self
            .loan_action_observations()
            .iter()
            .flat_map(|c| {
                c.loop_invariant_lineages()
                    .iter()
                    .filter(move |l| l.statement() == statement)
                    .map(move |l| (c, l))
            })
            .collect();
        let [(catalog, lineage)] = candidates.as_slice() else {
            return Err(Ambiguous);
        };
        if !catalog.owns_loop_invariant_lineage(lineage)
            || lineage.coverage() != UnitLoanActionCoverage::Partial
            || catalog.source() != statement.source_unit()
        {
            return Err(Unsupported);
        }
        let records: Vec<_> = ledger
            .records
            .iter()
            .filter(|r| r.source == statement.source_unit())
            .collect();
        let [record] = records.as_slice() else {
            return Err(Ambiguous);
        };
        let producers = record.loan_producers().ok_or(Unsupported)?;
        let mut callable = None;
        let mut ordinals = Vec::new();
        for id in
            std::iter::once(lineage.source_begin()).chain(lineage.uses().iter().map(|u| u.begin()))
        {
            let Some(UnitLoanAction::Begin(loan)) = catalog.action(&id) else {
                return Err(Unsupported);
            };
            let matches: Vec<_> = record
                .loans
                .iter()
                .enumerate()
                .filter(|(_, f)| f.call() == loan.call())
                .collect();
            let [(ordinal, original)] = matches.as_slice() else {
                return Err(Ambiguous);
            };
            let Some(Some(LoanProducer::Function(item))) = producers.get(*ordinal) else {
                return Err(Unsupported);
            };
            if *original != loan
                || ordinals.contains(ordinal)
                || item.source_unit() != statement.source_unit()
                || callable.is_some_and(|c| c != *item)
            {
                return Err(Unsupported);
            }
            callable = Some(*item);
            ordinals.push(*ordinal);
        }
        callable.ok_or(Unsupported)
    }
}
