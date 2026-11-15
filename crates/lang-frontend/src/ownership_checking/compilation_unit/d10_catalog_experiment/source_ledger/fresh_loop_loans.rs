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
            || !matches!(fact.consumers().len(), 1 | 2)
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

// The ordinary branch pair has separate call occurrences and sealed arm identities.
impl CompilationUnitOwnership {
    pub(crate) fn d10_arm_loop_link(
        &self,
        branch: UnitExpressionId,
    ) -> Result<UnitItemId, CallableLoanLinkError> {
        self.link_arm_source(branch, None)
    }
    pub(crate) fn d10_arm_loop_prefix_link(
        &self,
        branch: UnitExpressionId,
        call: UnitExpressionId,
        argument: UnitExpressionId,
    ) -> Result<UnitItemId, CallableLoanLinkError> {
        self.link_arm_source(branch, Some((call, argument)))
    }
    pub(crate) fn d10_arm_break_link(
        &self,
        branch: UnitExpressionId,
    ) -> Result<UnitItemId, CallableLoanLinkError> {
        use CallableLoanLinkError::{Ambiguous, Unsupported};
        let facts: Vec<_> = self
            .no_carry_break_branches()
            .iter()
            .filter(|f| f.pair().branch() == branch)
            .collect();
        let [fact] = facts.as_slice() else {
            return Err(Ambiguous);
        };
        if !self.owns_no_carry_break_branch(fact) {
            return Err(Unsupported);
        }
        self.link_arm_pair(fact.pair(), None)
    }
    pub(crate) fn d10_arm_break_suffix_link(
        &self,
        branch: UnitExpressionId,
        call: UnitExpressionId,
        argument: UnitExpressionId,
    ) -> Result<UnitItemId, CallableLoanLinkError> {
        use CallableLoanLinkError::{Ambiguous, Unsupported};
        let facts: Vec<_> = self
            .no_carry_break_branches()
            .iter()
            .filter(|f| f.pair().branch() == branch)
            .collect();
        let [fact] = facts.as_slice() else {
            return Err(Ambiguous);
        };
        if !self.owns_no_carry_break_branch(fact) {
            return Err(Unsupported);
        }
        self.link_arm_pair(fact.pair(), Some((call, argument)))
    }
    pub(crate) fn d10_arm_break_surrounding_link(
        &self,
        branch: UnitExpressionId,
        calls: &[(UnitExpressionId, UnitExpressionId)],
    ) -> Result<UnitItemId, CallableLoanLinkError> {
        use CallableLoanLinkError::{Ambiguous, Unsupported};
        if calls.len() > 2 {
            return Err(Unsupported);
        }
        let facts: Vec<_> = self
            .no_carry_break_branches()
            .iter()
            .filter(|f| f.pair().branch() == branch)
            .collect();
        let [fact] = facts.as_slice() else {
            return Err(Ambiguous);
        };
        if !self.owns_no_carry_break_branch(fact) {
            return Err(Unsupported);
        }
        self.link_arm_pair_calls(fact.pair(), calls)
    }
    pub(crate) fn d10_arm_counter_link(
        &self,
        branch: UnitExpressionId,
    ) -> Result<UnitItemId, CallableLoanLinkError> {
        use CallableLoanLinkError::{Ambiguous, Unsupported};
        let facts: Vec<_> = self
            .no_carry_counter_branches()
            .iter()
            .filter(|f| f.pair().branch() == branch)
            .collect();
        let [fact] = facts.as_slice() else {
            return Err(Ambiguous);
        };
        if !self.owns_no_carry_counter_branch(fact) {
            return Err(Unsupported);
        }
        self.link_arm_pair_calls(fact.pair(), &[])
    }
    fn link_arm_source(
        &self,
        branch: UnitExpressionId,
        prefix: Option<(UnitExpressionId, UnitExpressionId)>,
    ) -> Result<UnitItemId, CallableLoanLinkError> {
        use CallableLoanLinkError::{Ambiguous, Unsupported};
        let facts: Vec<_> = self
            .no_carry_branches()
            .iter()
            .filter(|f| f.branch() == branch)
            .collect();
        let [pair] = facts.as_slice() else {
            return Err(Ambiguous);
        };
        if !self.owns_no_carry_branch(pair) {
            return Err(Unsupported);
        }
        self.link_arm_pair(pair, prefix)
    }
    fn link_arm_pair(
        &self,
        pair: &crate::ownership_checking::UnitLoopBranchNoCarryFact,
        prefix: Option<(UnitExpressionId, UnitExpressionId)>,
    ) -> Result<UnitItemId, CallableLoanLinkError> {
        self.link_arm_pair_calls(pair, &prefix.into_iter().collect::<Vec<_>>())
    }
    fn link_arm_pair_calls(
        &self,
        pair: &crate::ownership_checking::UnitLoopBranchNoCarryFact,
        calls: &[(UnitExpressionId, UnitExpressionId)],
    ) -> Result<UnitItemId, CallableLoanLinkError> {
        use CallableLoanLinkError::{Ambiguous, Unsupported};
        let ledger = self.source_collection_ledger.as_ref().ok_or(Unsupported)?;
        if !ledger.matches_original(self) {
            return Err(Unsupported);
        }
        let branch = pair.branch();
        let callable = pair.arms()[0].callable();
        let records: Vec<_> = ledger
            .records
            .iter()
            .filter(|r| r.source == branch.source_unit())
            .collect();
        let [record] = records.as_slice() else {
            return Err(Ambiguous);
        };
        let producers = record.loan_producers().ok_or(Unsupported)?;
        let mut ordinals = Vec::new();
        for fact in pair.arms() {
            if fact.callable() != callable
                || callable.source_unit() != branch.source_unit()
                || !matches!(fact.consumers().len(), 1 | 2)
            {
                return Err(Unsupported);
            }
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
                    || producers.get(*ordinal) != Some(&Some(LoanProducer::Function(callable)))
                    || ordinals.contains(ordinal)
                {
                    return Err(Unsupported);
                }
                ordinals.push(*ordinal);
            }
        }
        for &(call, argument) in calls {
            let loans: Vec<_> = record
                .loans
                .iter()
                .enumerate()
                .filter(|(_, l)| l.call() == call)
                .collect();
            let [(ordinal, loan)] = loans.as_slice() else {
                return Err(Ambiguous);
            };
            if ordinals.contains(ordinal)
                || loan.argument() != argument
                || producers.get(*ordinal) != Some(&Some(LoanProducer::Function(callable)))
                || !matches!(loan.target(), crate::ownership_checking::UnitLoanTarget::Place(place) if place.is_root() && place.root()==pair.arms()[0].source())
            {
                return Err(Unsupported);
            }
            ordinals.push(*ordinal);
        }
        Ok(callable)
    }
}
