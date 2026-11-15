//! Core no-carry identity and original call acquisitions, not a new catalog completeness claim.
use super::*;
impl CompilationUnitOwnership {
    pub(crate) fn d10_fresh_loop_link(
        &self,
        statement: crate::type_checking::UnitStatementId,
    ) -> Result<UnitItemId, CallableLoanLinkError> {
        use CallableLoanLinkError::{Ambiguous, Unsupported};
        let ledger = self.source_collection_ledger.as_ref().ok_or(Unsupported)?;
        if !ledger.matches_original(self) {
            return Err(Unsupported);
        }
        let facts: Vec<_> = self
            .no_carry_loops()
            .iter()
            .filter(|f| f.loop_statement() == statement)
            .collect();
        let [fact] = facts.as_slice() else {
            return Err(Ambiguous);
        };
        if !self.owns_no_carry_loop(fact)
            || fact.consumers().len() != 1
            || fact.callable().source_unit() != statement.source_unit()
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
        let mut ordinals = Vec::new();
        for (call, argument) in std::iter::once((fact.initializer(), fact.source_argument()))
            .chain(fact.consumers().iter().map(|(_, c, a)| (*c, *a)))
        {
            let loans: Vec<_> = record
                .loans
                .iter()
                .enumerate()
                .filter(|(_, l)| l.call() == call)
                .collect();
            let [(ordinal, loan)] = loans.as_slice() else {
                return Err(Ambiguous);
            };
            if loan.argument() != argument
                || producers.get(*ordinal) != Some(&Some(LoanProducer::Function(fact.callable())))
                || ordinals.contains(ordinal)
            {
                return Err(Unsupported);
            }
            ordinals.push(*ordinal);
        }
        // Source segments use this same original parameter root. Check every such
        // original acquisition, not caller-supplied expression tuples or a final IR scan.
        for (ordinal, loan) in record.loans.iter().enumerate().filter(|(_,loan)| {
            matches!(loan.target(), crate::ownership_checking::UnitLoanTarget::Place(place) if place.is_root() && place.root()==fact.source())
        }) {
            if producers.get(ordinal)!=Some(&Some(LoanProducer::Function(fact.callable())))
                || record.loans.iter().filter(|other| other.call()==loan.call()).count()!=1 {
                return Err(Ambiguous);
            }
        }
        Ok(fact.callable())
    }
}
