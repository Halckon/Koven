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

impl Builder<'_, '_> {
    fn fresh_loop_abi(&self, span: Span) -> Result<Vec<Slot>, LoweringError> {
        if self.borrowed.len() != self.inputs.parameters.len()
            || self.inputs.parameters.len() != self.body.fresh_loop_plans.abi_types.len()
            || !self.result_source_loans.is_empty()
            || !self.pending_call_loans.is_empty()
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        self.inputs
            .parameters
            .iter()
            .map(|symbol| {
                if self.dead.contains(symbol) {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                }
                self.borrowed
                    .get(symbol)
                    .copied()
                    .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))
            })
            .collect()
    }
    pub(super) fn fresh_loop_entry(
        &mut self,
        statement: UnitStatementId,
        blocks: [BlockRef; 5],
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.body.fresh_loop_plans.loops.is_empty() {
            return Ok(());
        }
        let p = self
            .body
            .fresh_loop_plans
            .next()
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?;
        if self.inputs.parameters.get(p.source_parameter) != Some(&p.fact.source()) {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        let abi = self.fresh_loop_abi(span)?;
        let [entry, header, body, finished, exit] = blocks;
        self.body.fresh_loop_plans.begin(
            statement,
            super::super::fresh_loop_receipt::LoopBlocks {
                entry,
                header,
                body,
                finished,
                exit,
            },
            abi,
            span,
        )
    }
    pub(super) fn fresh_loop_boundary(
        &mut self,
        phase: usize,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.body.fresh_loop_plans.loops.is_empty() {
            return Ok(());
        }
        let p = self
            .body
            .fresh_loop_plans
            .current()
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?;
        let abi = self.fresh_loop_abi(span)?;
        if self.body.fresh_loop_plans.abi_slots.as_ref() != Some(&abi)
            || self.inputs.parameters.get(p.source_parameter) != Some(&p.fact.source())
            || abi.get(p.source_parameter).copied() != p.source
            || abi.get(p.flag_parameter).copied() != p.flag
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        self.body.fresh_loop_plans.boundary(phase, span)
    }
}
