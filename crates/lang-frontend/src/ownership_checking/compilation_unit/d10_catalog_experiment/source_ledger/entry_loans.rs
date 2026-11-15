//! Actual entry acquisition attribution only. No binding lifecycle certificate is minted.
use super::*;
impl CompilationUnitOwnership {
    pub(crate) fn d10_entry_borrow_link(&self, call: UnitExpressionId) -> Option<UnitItemId> {
        let ledger = self.source_collection_ledger.as_ref()?;
        if !ledger.matches_original(self) {
            return None;
        }
        let records: Vec<_> = ledger
            .records
            .iter()
            .filter(|r| r.source == call.source_unit())
            .collect();
        let [record] = records.as_slice() else {
            return None;
        };
        let handoffs: Vec<_> = self
            .borrow_call_handoffs()
            .iter()
            .filter(|h| h.source_loan().call() == call)
            .collect();
        let [handoff] = handoffs.as_slice() else {
            return None;
        };
        let acquisitions: Vec<_> = record
            .loans
            .iter()
            .enumerate()
            .filter(|(_, l)| l.call() == call)
            .collect();
        if acquisitions.len() != 2
            || acquisitions
                .iter()
                .filter(|(_, l)| *l == handoff.source_loan())
                .count()
                != 1
        {
            return None;
        }
        let producers = record.loan_producers()?;
        let mut callable = None;
        for (ordinal, loan) in acquisitions {
            if record.loans.iter().filter(|other| *other == loan).count() != 1 {
                return None;
            }
            let Some(LoanProducer::Function(item)) = producers.get(ordinal)? else {
                return None;
            };
            if callable.is_some_and(|previous| previous != *item) {
                return None;
            }
            callable = Some(*item);
        }
        callable
    }
}
