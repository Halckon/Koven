//! Original live-forward region linked to the actual callable loan producer.
use super::*;
use crate::ownership_checking::{UnitLoanAction, UnitLoanActionCoverage};
impl CompilationUnitOwnership {
    pub(crate) fn d10_forward_link(
        &self,
        call: UnitExpressionId,
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
                c.forward_regions()
                    .iter()
                    .filter(move |r| r.body() == call)
                    .map(move |r| (c, r))
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
        if !catalog.owns_forward_region(region)
            || region.coverage() != UnitLoanActionCoverage::Complete
            || catalog.source() != call.source_unit()
            || callable.source_unit() != call.source_unit()
        {
            return Err(Unsupported);
        }
        let ids: Vec<_> = region.action_ids().collect();
        let [begin, end] = ids.as_slice() else {
            return Err(Unsupported);
        };
        let Some(UnitLoanAction::Begin(loan)) = catalog.action(begin) else {
            return Err(Unsupported);
        };
        let Some(UnitLoanAction::BorrowForwardHandoff(handoff)) = catalog.action(end) else {
            return Err(Unsupported);
        };
        if loan.call() != call
            || handoff.branch().is_some()
            || handoff.callable() != Some(callable)
            || handoff.source_begin().as_ref() != Some(begin)
            || handoff.transaction().source_loan() != loan
            || handoff.transaction().returned_expression() != call
        {
            return Err(Unsupported);
        }
        let records: Vec<_> = ledger
            .records
            .iter()
            .filter(|r| r.source == call.source_unit())
            .collect();
        let [record] = records.as_slice() else {
            return Err(Ambiguous);
        };
        let producers = record.loan_producers().ok_or(Unsupported)?;
        let originals: Vec<_> = record
            .loans
            .iter()
            .enumerate()
            .filter(|(_, f)| f.call() == call)
            .collect();
        let [(index, original)] = originals.as_slice() else {
            return Err(Ambiguous);
        };
        if *original != loan
            || producers.get(*index) != Some(&Some(LoanProducer::Function(callable)))
        {
            return Err(Unsupported);
        }
        Ok(callable)
    }
}
