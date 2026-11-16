//! Link original append provenance to an existing independently validated local protocol.
//! This does not certify the enclosing callable, unknown routes or general CFG reachability.
use super::*;
use crate::ownership_checking::{
    UnitIterationCleanupAction, UnitLoanAction, UnitLoanActionCoverage,
};
/// Body that actually appended a loan. Environment formation retains its outer context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoanProducer {
    /// Ordinary function body.
    Function(UnitItemId),
    /// Deinitializer body.
    Deinit(UnitItemId),
    /// Lambda body, not its capture environment formation.
    Lambda(UnitExpressionId),
}
impl LoanProducer {
    fn source(self) -> SourceUnitId {
        match self {
            Self::Function(item) | Self::Deinit(item) => item.source_unit(),
            Self::Lambda(lambda) => lambda.source_unit(),
        }
    }
}
/// Fail-closed local linkage. Unsupported is not evidence of loan absence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallableLoanLinkError {
    /// Missing, empty, partial or outside protocol 1's existing bounded normal-return scope.
    Unsupported,
    /// More than one original fact or batch can match; never select the first arbitrarily.
    Ambiguous,
}
impl SourceCollectionRecord {
    pub(crate) fn with_loan_producers(
        mut self,
        producers: Option<Vec<Option<LoanProducer>>>,
    ) -> Self {
        self.loan_producers = producers;
        self
    }
    /// One source-local ordinal per original loan after rollback, without copying loans again.
    /// Outer None is an unsealed/invalid index; an entry None is an unknown/non-callable
    /// producer, not unreachable code.
    #[must_use]
    pub fn loan_producers(&self) -> Option<&[Option<LoanProducer>]> {
        self.loan_producers.as_deref()
    }
    pub(super) fn loan_producers_match(&self) -> bool {
        let Some(producers) = &self.loan_producers else {
            return false;
        };
        producers.len() == self.loans.len()
            && producers.iter().zip(&self.loans).all(|(producer, loan)| {
                loan.call().source_unit() == self.source
                    && producer.is_none_or(|producer| producer.source() == self.source)
            })
    }
}
impl SourceCollectionLedger {
    /// Join a nonempty, already validated protocol-1 region to actual producer and Drop batches.
    /// The original validator remains the sole contract/point/order authority. This checks only
    /// cross-channel membership and returns the original callable, never a new Complete token.
    pub fn link_literal_call(
        &self,
        owned: &ValidatedCompilationUnitOwnership,
        call: UnitExpressionId,
    ) -> Result<UnitItemId, CallableLoanLinkError> {
        self.link_original(owned.ownership(), call)
    }
    fn link_original(
        &self,
        owned: &CompilationUnitOwnership,
        call: UnitExpressionId,
    ) -> Result<UnitItemId, CallableLoanLinkError> {
        use CallableLoanLinkError::{Ambiguous, Unsupported};
        if !self.matches_original(owned) {
            return Err(Unsupported);
        }
        let observations = owned.loan_action_observations();
        let mut matches = observations.iter().flat_map(|observation| {
            observation
                .regions()
                .iter()
                .filter(move |region| region.call() == call)
                .map(move |region| (observation, region))
        });
        let (observation, region) = matches.next().ok_or(Unsupported)?;
        if matches.next().is_some() {
            return Err(Ambiguous);
        }
        if !observation.owns_region(region)
            || region.protocol_version() != 1
            || region.coverage() != UnitLoanActionCoverage::Complete
        {
            return Err(Unsupported);
        }
        let callable = region.callable().ok_or(Unsupported)?;
        if observation.source() != call.source_unit()
            || callable.source_unit() != call.source_unit()
        {
            return Err(Unsupported);
        }
        let mut records = self
            .records
            .iter()
            .filter(|r| r.source == call.source_unit());
        let record = records.next().ok_or(Unsupported)?;
        if records.next().is_some() {
            return Err(Ambiguous);
        }
        let mut batches = record.callable_drop_traversals().iter().filter(|b| {
            b.callable()
                == DropCallable::Function {
                    item: callable,
                    has_body: true,
                }
        });
        let batch = batches.next().ok_or(Unsupported)?;
        if batches.next().is_some() {
            return Err(Ambiguous);
        }
        let mut begun = Vec::new();
        for id in region.action_ids() {
            match observation.action(&id).ok_or(Unsupported)? {
                UnitLoanAction::Begin(loan) => {
                    let ordinals: Vec<_> = record
                        .loans
                        .iter()
                        .enumerate()
                        .filter(|(_, original)| *original == loan)
                        .map(|(i, _)| i)
                        .collect();
                    if ordinals.len() > 1 {
                        return Err(Ambiguous);
                    }
                    let ordinal = *ordinals.first().ok_or(Unsupported)?;
                    if record.loan_producers.as_ref().ok_or(Unsupported)?[ordinal]
                        != Some(LoanProducer::Function(callable))
                    {
                        return Err(Unsupported);
                    }
                    begun.push(ordinal);
                }
                UnitLoanAction::Cleanup {
                    action: UnitIterationCleanupAction::Drop(drop),
                    ..
                } => {
                    match batch
                        .drops()
                        .iter()
                        .filter(|original| *original == drop)
                        .count()
                    {
                        1 => (),
                        0 => return Err(Unsupported),
                        _ => return Err(Ambiguous),
                    }
                }
                UnitLoanAction::Cleanup {
                    action: UnitIterationCleanupAction::EndCallLoan(_),
                    ..
                } => (),
                _ => return Err(Unsupported),
            }
        }
        // Require the complete original call batch, not a filtered subset or Complete-empty.
        let expected: Vec<_> = record
            .loans
            .iter()
            .enumerate()
            .filter(|(_, loan)| loan.call() == call)
            .map(|(i, _)| i)
            .collect();
        if begun.is_empty() || begun != expected {
            return Err(Unsupported);
        }
        Ok(callable)
    }
}

impl CompilationUnitOwnership {
    pub(crate) fn d10_literal_call_link(
        &self,
        call: UnitExpressionId,
    ) -> Result<UnitItemId, CallableLoanLinkError> {
        self.source_collection_ledger
            .as_ref()
            .ok_or(CallableLoanLinkError::Unsupported)?
            .link_original(self, call)
    }
}
