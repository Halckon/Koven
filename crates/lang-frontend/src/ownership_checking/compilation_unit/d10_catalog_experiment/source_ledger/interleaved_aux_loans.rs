//! Original source, auxiliary and consumer acquisitions must belong to the same actual callable traversal.
use super::*;
use crate::ownership_checking::{UnitLoanAction, UnitLoanActionCoverage};
impl CompilationUnitOwnership {
    pub(crate) fn d10_interleaved_aux_link(
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
                c.interleaved_auxiliary_regions()
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
        if !catalog.owns_interleaved_auxiliary_region(region)
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
        let count = region.uses().len();
        if !matches!(
            (count, region.binding_use_position()),
            (1, 1) | (2, 1) | (2, 2)
        ) || ids.len() != 9 + 2 * count
        {
            return Err(Unsupported);
        }
        let Some(UnitLoanAction::BorrowHandoffCommitted(handoff)) = catalog.action(&ids[2]) else {
            return Err(Unsupported);
        };
        let indices = if count == 1 {
            vec![0, 1, 6, 3]
        } else if region.binding_use_position() == 1 {
            vec![0, 1, 5, 3, 10]
        } else {
            vec![0, 1, 8, 3, 5]
        };
        let acquisitions: Vec<_> = indices
            .into_iter()
            .map(|index| match catalog.action(&ids[index]) {
                Some(UnitLoanAction::Begin(loan)) => Ok(loan),
                _ => Err(Unsupported),
            })
            .collect::<Result<_, _>>()?;
        let source_index = acquisitions[..2]
            .iter()
            .position(|loan| *loan == handoff.source_loan())
            .ok_or(Unsupported)?;
        let auxiliary_index = 1 - source_index;
        if handoff.auxiliary_loan_ends() != std::slice::from_ref(acquisitions[auxiliary_index])
            || catalog
                .distinct_auxiliary_retirements()
                .iter()
                .filter(|r| {
                    r.source_begin() == ids[source_index]
                        && r.auxiliary_begin() == ids[auxiliary_index]
                        && r.handoff() == ids[2]
                        && catalog.owns_distinct_auxiliary_retirement(r)
                        && r.auxiliary_drop().is_none()
                        && r.source_parameter() == source_index
                        && Some(r.source_owner()) == region.source_owner()
                        && Some(r.auxiliary_owner()) == region.auxiliary_owner()
                })
                .count()
                != 1
        {
            return Err(Unsupported);
        }
        if acquisitions[3..]
            .iter()
            .map(|l| l.call())
            .collect::<Vec<_>>()
            != region
                .uses()
                .iter()
                .map(|u| u.loan().call())
                .collect::<Vec<_>>()
        {
            return Err(Unsupported);
        }
        let source_facts: Vec<_> = record.loans.iter().filter(|l| l.call() == call).collect();
        if source_facts != acquisitions[..2] || acquisitions[2].call() != region.consumer_call() {
            return Err(Unsupported);
        }
        let mut ordinals = Vec::new();
        for loan in acquisitions {
            let matches: Vec<_> = record
                .loans
                .iter()
                .enumerate()
                .filter(|(_, f)| *f == loan)
                .collect();
            let [(ordinal, _)] = matches.as_slice() else {
                return Err(Ambiguous);
            };
            if producers.get(*ordinal) != Some(&Some(LoanProducer::Function(callable)))
                || ordinals.contains(ordinal)
            {
                return Err(Unsupported);
            }
            ordinals.push(*ordinal);
        }
        if record
            .loans
            .iter()
            .filter(|l| l.call() == region.consumer_call())
            .count()
            != 1
        {
            return Err(Ambiguous);
        }
        if region.uses().iter().any(|u| {
            record
                .loans
                .iter()
                .filter(|l| l.call() == u.loan().call())
                .count()
                != 1
        }) {
            return Err(Ambiguous);
        }
        Ok(callable)
    }
}
