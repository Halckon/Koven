//! Original branch acquisitions joined to actual traversal provenance, not a new capability.
use super::*;
use crate::ownership_checking::{UnitLoanAction, UnitLoanActionCoverage};
impl CompilationUnitOwnership {
    pub(crate) fn d10_branch_link(
        &self,
        split: UnitExpressionId,
    ) -> Result<UnitItemId, CallableLoanLinkError> {
        use CallableLoanLinkError::{Ambiguous, Unsupported};
        let ledger = self.source_collection_ledger.as_ref().ok_or(Unsupported)?;
        if !ledger.matches_original(self) {
            return Err(Unsupported);
        }
        let candidates: Vec<_> = self
            .loan_action_observations()
            .iter()
            .flat_map(|catalog| {
                catalog
                    .branch_forward_regions()
                    .iter()
                    .filter(move |r| r.split() == split)
                    .map(move |region| (catalog, region))
            })
            .collect();
        let [(catalog, region)] = candidates.as_slice() else {
            return Err(if candidates.is_empty() {
                Unsupported
            } else {
                Ambiguous
            });
        };
        let callable = region.callable();
        if !catalog.owns_branch_forward_region(region)
            || region.coverage() != UnitLoanActionCoverage::Complete
            || region.paths().len() != 2
            || catalog.source() != split.source_unit()
            || callable.source_unit() != split.source_unit()
        {
            return Err(Unsupported);
        }
        let records: Vec<_> = ledger
            .records
            .iter()
            .filter(|r| r.source == split.source_unit())
            .collect();
        let [record] = records.as_slice() else {
            return Err(Ambiguous);
        };
        let producers = record.loan_producers().ok_or(Unsupported)?;
        let mut ordinals = Vec::new();
        for path in region.paths() {
            let ids: Vec<_> = path.action_ids().collect();
            if ids != [path.source_begin(), path.live_handoff()]
                || path.branch().callable() != callable
                || path.branch().split() != split
            {
                return Err(Unsupported);
            }
            let Some(UnitLoanAction::Begin(loan)) = catalog.action(&ids[0]) else {
                return Err(Unsupported);
            };
            let matches: Vec<_> = record
                .loans
                .iter()
                .enumerate()
                .filter(|(_, original)| original.call() == path.branch().value())
                .collect();
            let [(ordinal, original)] = matches.as_slice() else {
                return Err(Ambiguous);
            };
            if *original != loan
                || producers.get(*ordinal) != Some(&Some(LoanProducer::Function(callable)))
                || ordinals.contains(ordinal)
            {
                return Err(Unsupported);
            }
            let Some(UnitLoanAction::BorrowForwardHandoff(handoff)) = catalog.action(&ids[1])
            else {
                return Err(Unsupported);
            };
            if handoff.branch() != Some(path.branch())
                || handoff.callable() != Some(callable)
                || handoff.source_begin().as_ref() != Some(&ids[0])
                || handoff.transaction().source_loan() != loan
            {
                return Err(Unsupported);
            }
            ordinals.push(*ordinal);
        }
        Ok(callable)
    }
}
