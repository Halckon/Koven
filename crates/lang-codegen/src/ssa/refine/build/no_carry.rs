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
        use lang_frontend::type_checking::ParameterMode;
        if self.borrowed.len().checked_add(self.bindings.len())
            != Some(self.inputs.parameters.len())
            || self.inputs.parameters.len() != self.body.fresh_loop_plans.abi_types.len()
            || self.inputs.modes != self.body.fresh_loop_plans.abi_modes
            || !self.result_source_loans.is_empty()
            || !self.pending_call_loans.is_empty()
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        self.inputs
            .parameters
            .iter()
            .enumerate()
            .map(|(index, symbol)| {
                let fail = || error(LoweringErrorKind::InvalidModel, span);
                if self.dead.contains(symbol) {
                    return Err(fail());
                }
                let semantic = self.body.fresh_loop_plans.abi_types[index];
                let (slot, expected) = match self.inputs.modes.get(index) {
                    Some(ParameterMode::Borrow) if !self.bindings.contains_key(symbol) => (
                        self.borrowed.get(symbol).copied().ok_or_else(fail)?,
                        EntityType::Loan {
                            kind: crate::ssa::model::LoanKind::Shared,
                            target: semantic,
                        },
                    ),
                    Some(ParameterMode::Value) if !self.borrowed.contains_key(symbol) => (
                        self.bindings.get(symbol).copied().ok_or_else(fail)?,
                        EntityType::Value(semantic),
                    ),
                    _ => return Err(fail()),
                };
                if self.body.slots.get(slot.0) != Some(&expected) {
                    return Err(fail());
                }
                Ok(slot)
            })
            .collect()
    }
    pub(super) fn fresh_loop_parameters(&mut self, span: Span) -> Result<(), LoweringError> {
        if self.body.fresh_loop_plans.loops.is_empty() {
            return Ok(());
        }
        let abi = self.fresh_loop_abi(span)?;
        let block = self.current(span)?;
        self.body.fresh_loop_plans.bind_abi(abi, block, span)
    }
    pub(super) fn fresh_source_argument(
        &mut self,
        call: UnitExpressionId,
        argument: ExpressionId,
        loan: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        if !self.body.fresh_loop_plans.has_source_call(call) {
            return Ok(());
        }
        let abi = self.fresh_loop_abi(span)?;
        if self.body.fresh_loop_plans.abi_slots.as_ref() != Some(&abi) {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        let block = self.current(span)?;
        self.body
            .fresh_loop_plans
            .source_operand(call, self.unit(argument), loan, block, span)
    }
    pub(super) fn fresh_source_completion(
        &mut self,
        statement: UnitStatementId,
        result: Lowered,
    ) -> Result<(), LoweringError> {
        if !self.body.fresh_loop_plans.has_source_statement(statement) {
            return Ok(());
        }
        let span = self.statement_span(statement.statement())?;
        let abi = self.fresh_loop_abi(span)?;
        if result != Lowered::Unit || self.body.fresh_loop_plans.abi_slots.as_ref() != Some(&abi) {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        let block = self.current(span)?;
        self.body
            .fresh_loop_plans
            .source_complete(statement, block, span)
    }
    pub(super) fn fresh_loop_condition(
        &mut self,
        expression: UnitExpressionId,
        value: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.body.fresh_loop_plans.loops.is_empty() {
            return Ok(());
        }
        let abi = self.fresh_loop_abi(span)?;
        if self.body.fresh_loop_plans.abi_slots.as_ref() != Some(&abi) {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        let block = self.current(span)?;
        self.body
            .fresh_loop_plans
            .condition(expression, value, block, span)
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
