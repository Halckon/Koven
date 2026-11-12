//! Check actual fresh-result absence and external loan preservation at reached loop boundaries.
use super::*;
use lang_frontend::ownership_checking::UnitLoopNoCarryFact;

pub(super) struct NoCarryLease {
    proof: UnitLoopNoCarryFact,
    borrowed: BTreeMap<UnitSymbolId, Slot>,
    result_sources: BTreeMap<UnitSymbolId, Vec<Slot>>,
    pending: Vec<Slot>,
}
impl Builder<'_, '_> {
    pub(super) fn no_carry_entry(
        &self,
        statement: StatementId,
        span: Span,
    ) -> Result<Option<NoCarryLease>, LoweringError> {
        let id = UnitStatementId::new(self.inputs.source_unit, statement);
        let mut selected = self
            .inputs
            .owned
            .no_carry_loops()
            .iter()
            .filter(|p| p.loop_statement() == id);
        let Some(proof) = selected.next() else {
            return Ok(None);
        };
        let invalid = || error(LoweringErrorKind::InvalidModel, span);
        if self.inputs.function_item != Some(proof.callable().item())
            || selected.next().is_some()
            || !self.inputs.owned.owns_no_carry_loop(proof)
            || !self.borrowed.contains_key(&proof.source())
            || self.borrowed.contains_key(&proof.binding())
            || self.result_source_loans.contains_key(&proof.binding())
            || !self.pending_call_loans.is_empty()
        {
            return Err(invalid());
        }
        let binding = self
            .inputs
            .owned
            .borrow_results()
            .bindings()
            .iter()
            .find(|b| b.binding() == proof.binding())
            .ok_or_else(invalid)?;
        if crate::ssa::borrow_result_support::no_carry::admitted(
            self.inputs.parsed,
            self.inputs.owned,
            binding,
        ) != Some(statement)
        {
            return Err(invalid());
        }
        Ok(Some(NoCarryLease {
            proof: proof.clone(),
            borrowed: self.borrowed.clone(),
            result_sources: self.result_source_loans.clone(),
            pending: self.pending_call_loans.clone(),
        }))
    }
    pub(super) fn no_carry_boundary(
        &self,
        lease: Option<&NoCarryLease>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(lease) = lease else {
            return Ok(());
        };
        if self.borrowed != lease.borrowed
            || self.result_source_loans != lease.result_sources
            || self.pending_call_loans != lease.pending
            || self.dead.contains(&lease.proof.source())
            || self.borrowed.contains_key(&lease.proof.binding())
            || self
                .result_source_loans
                .contains_key(&lease.proof.binding())
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        Ok(())
    }
}
