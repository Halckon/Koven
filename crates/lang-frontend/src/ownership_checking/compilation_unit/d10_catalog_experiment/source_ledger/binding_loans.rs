//! Both original binding acquisitions must belong to the same actual callable traversal.
use super::*;
use crate::ownership_checking::{UnitLoanAction, UnitLoanActionCoverage};
impl CompilationUnitOwnership {
    pub(crate) fn d10_binding_link(
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
                c.binding_regions()
                    .iter()
                    .filter(move |r| r.source_call() == call)
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
        if !catalog.owns_binding_region(region)
            || region.coverage() != UnitLoanActionCoverage::Complete
            || catalog.source() != call.source_unit()
            || callable.source_unit() != call.source_unit()
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
        let ids: Vec<_> = region.action_ids().collect();
        if ids.len() != 7 {
            return Err(Unsupported);
        }
        let mut ordinals = Vec::new();
        for (index, expected) in [(0, call), (2, region.consumer_call())] {
            let Some(UnitLoanAction::Begin(loan)) = catalog.action(&ids[index]) else {
                return Err(Unsupported);
            };
            if loan.call() != expected {
                return Err(Unsupported);
            }
            let matches: Vec<_> = record
                .loans
                .iter()
                .enumerate()
                .filter(|(_, f)| f.call() == expected)
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
            ordinals.push(*ordinal);
        }
        Ok(callable)
    }
}
