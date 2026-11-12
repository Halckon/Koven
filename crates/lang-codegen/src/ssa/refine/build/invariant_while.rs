//! Actual logical lease state is unchanged at each reached while boundary.
use super::*;

pub(super) struct InvariantLease {
    binding: UnitSymbolId,
    root: UnitSymbolId,
    result: Slot,
    sources: Vec<Slot>,
    owner: Option<Slot>,
    parameter: Option<Slot>,
}

impl Builder<'_, '_> {
    pub(super) fn invariant_while_entry(
        &self,
        statement: StatementId,
        span: Span,
    ) -> Result<Option<InvariantLease>, LoweringError> {
        let mut selected = self
            .inputs
            .owned
            .borrow_results()
            .bindings()
            .iter()
            .filter(|fact| {
                fact.binding().source_unit() == self.inputs.source_unit
                    && crate::ssa::borrow_result_support::invariant_while::admitted(
                        self.inputs.parsed,
                        self.inputs.names,
                        self.inputs.typed,
                        self.inputs.owned,
                        fact,
                    ) == Some(statement)
            });
        let Some(fact) = selected.next() else {
            return Ok(None);
        };
        if selected.next().is_some() {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        let lang_frontend::ownership_checking::UnitLoanTarget::Place(root) = fact.origin() else {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        };
        let result = *self
            .borrowed
            .get(&fact.binding())
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?;
        let sources = self
            .result_source_loans
            .get(&fact.binding())
            .cloned()
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?;
        let lease = InvariantLease {
            binding: fact.binding(),
            root: root.root(),
            result,
            sources,
            owner: self.bindings.get(&root.root()).copied(),
            parameter: self.borrowed.get(&root.root()).copied(),
        };
        self.invariant_while_boundary(Some(&lease), span)?;
        Ok(Some(lease))
    }

    pub(super) fn invariant_while_boundary(
        &self,
        lease: Option<&InvariantLease>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(lease) = lease else {
            return Ok(());
        };
        if self.borrowed.get(&lease.binding) != Some(&lease.result)
            || self.result_source_loans.get(&lease.binding) != Some(&lease.sources)
            || self.bindings.get(&lease.root).copied() != lease.owner
            || self.borrowed.get(&lease.root).copied() != lease.parameter
            || (lease.owner.is_none() && lease.parameter.is_none())
            || self.dead.contains(&lease.root)
            || !self.pending_call_loans.is_empty()
            || !self.active_views.is_empty()
            || !self.pending_views.is_empty()
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        Ok(())
    }
}

pub(super) struct InvariantContinue {
    lease: InvariantLease,
    target: BlockRef,
}

impl Builder<'_, '_> {
    pub(super) fn invariant_continue_entry(
        &self,
        expression: ExpressionId,
        is_break: bool,
        frame: LoopFrame,
        span: Span,
    ) -> Result<Option<InvariantContinue>, LoweringError> {
        let Some(lease) = self.invariant_while_entry(frame.statement.statement(), span)? else {
            return Ok(None);
        };
        if is_break
            || frame.seq_for.is_some()
            || frame.map_for.is_some()
            || frame.statement.source_unit() != self.inputs.source_unit
            || frame.loans != self.pending_call_loans.len()
            || frame.views != self.active_views.len()
            || crate::ssa::borrow_result_support::invariant_while::tail_continue(
                self.inputs.parsed,
                frame.statement.statement(),
            ) != Some(expression)
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        Ok(Some(InvariantContinue {
            lease,
            target: frame.continue_target,
        }))
    }

    pub(super) fn invariant_continue_exit(
        &self,
        continued: Option<&InvariantContinue>,
        target: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(continued) = continued else {
            return Ok(());
        };
        if continued.target != target {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        self.invariant_while_boundary(Some(&continued.lease), span)
    }
}
