//! Actual logical lease state is unchanged at each reached while boundary.
use super::*;

pub(super) struct InvariantLease {
    statement: StatementId,
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
            statement,
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

pub(super) struct InvariantJump {
    lease: InvariantLease,
    target: BlockRef,
    is_break: bool,
}

impl Builder<'_, '_> {
    pub(super) fn invariant_jump_entry(
        &self,
        expression: ExpressionId,
        is_break: bool,
        frame: LoopFrame,
        span: Span,
    ) -> Result<Option<InvariantJump>, LoweringError> {
        let Some(lease) = self.invariant_while_entry(frame.statement.statement(), span)? else {
            return Ok(None);
        };
        let kind = if is_break {
            crate::ssa::borrow_result_support::invariant_while::JumpKind::Break
        } else {
            crate::ssa::borrow_result_support::invariant_while::JumpKind::Continue
        };
        let selected = crate::ssa::borrow_result_support::invariant_while::tail_jump(
            self.inputs.parsed,
            frame.statement.statement(),
        )
        .or_else(|| {
            crate::ssa::borrow_result_support::invariant_while::conditional_jump(
                self.inputs.parsed,
                frame.statement.statement(),
            )
            .map(|tail| (tail.jump, tail.kind))
        });
        if frame.seq_for.is_some()
            || frame.map_for.is_some()
            || frame.statement.source_unit() != self.inputs.source_unit
            || frame.loans != self.pending_call_loans.len()
            || frame.views != self.active_views.len()
            || selected != Some((expression, kind))
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        Ok(Some(InvariantJump {
            lease,
            target: if is_break {
                frame.exit
            } else {
                frame.continue_target
            },
            is_break,
        }))
    }

    pub(super) fn invariant_jump_exit(
        &self,
        continued: Option<&InvariantJump>,
        is_break: bool,
        target: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(continued) = continued else {
            return Ok(());
        };
        if continued.is_break != is_break || continued.target != target {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        self.invariant_while_boundary(Some(&continued.lease), span)
    }
}

impl Builder<'_, '_> {
    pub(super) fn invariant_if_entry(
        &self,
        control: ExpressionId,
        span: Span,
    ) -> Result<Option<InvariantLease>, LoweringError> {
        let Some(frame) = self.loops.last() else {
            return Ok(None);
        };
        let Some(tail) = crate::ssa::borrow_result_support::invariant_while::conditional_jump(
            self.inputs.parsed,
            frame.statement.statement(),
        ) else {
            return Ok(None);
        };
        if tail.control != control {
            return Ok(None);
        }
        self.invariant_while_entry(frame.statement.statement(), span)
    }
}

/// Captured immediately before the original return cleanup, not reconstructed from final SSA.
pub(super) struct InvariantReturn {
    lease: InvariantLease,
    block: BlockRef,
    start: usize,
    expression: ExpressionId,
}

impl Builder<'_, '_> {
    pub(super) fn invariant_return_entry(
        &self,
        expression: ExpressionId,
        value: Option<ExpressionId>,
        span: Span,
    ) -> Result<Option<InvariantReturn>, LoweringError> {
        let Some(frame) = self.loops.last() else {
            return Ok(None);
        };
        let Some(lease) = self.invariant_while_entry(frame.statement.statement(), span)? else {
            return Ok(None);
        };
        if value.is_some()
            || !self.is_unit(self.inputs.return_type)
            || crate::ssa::borrow_result_support::invariant_while::tail_return(
                self.inputs.parsed,
                frame.statement.statement(),
            ) != Some(expression)
            || frame.loans != self.pending_call_loans.len()
            || frame.views != self.active_views.len()
            || frame.seq_for.is_some()
            || frame.map_for.is_some()
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        let block = self.current(span)?;
        Ok(Some(InvariantReturn {
            lease,
            block,
            start: self.body.blocks[block.0].statements.len(),
            expression,
        }))
    }

    pub(super) fn invariant_return_emitted(
        &self,
        returned: Option<&InvariantReturn>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(returned) = returned else {
            return Ok(());
        };
        self.invariant_return_ending(
            &returned.lease,
            returned.block,
            Some(returned.start),
            returned.expression,
            span,
        )
    }

    pub(super) fn invariant_while_body_end(
        &self,
        lease: Option<&InvariantLease>,
        body: BlockRef,
        reached: bool,
        span: Span,
    ) -> Result<(), LoweringError> {
        if let Some(lease) = lease
            && let Some(returned) = crate::ssa::borrow_result_support::invariant_while::tail_return(
                self.inputs.parsed,
                lease.statement,
            )
        {
            if reached {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            }
            return self.invariant_return_ending(lease, body, None, returned, span);
        }
        self.invariant_while_boundary(lease, span)
    }

    fn invariant_return_ending(
        &self,
        lease: &InvariantLease,
        block: BlockRef,
        captured: Option<usize>,
        returned: ExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let invalid = || error(LoweringErrorKind::InvalidModel, span);
        let original = self.body.blocks.get(block.0).ok_or_else(invalid)?;
        let return_span = self
            .inputs
            .parsed
            .ast()
            .expressions()
            .get(returned)
            .map_err(|_| invalid())?
            .span();
        if self.block.is_some()
            || !matches!(original.terminator, Some((RTerm::Return(None), actual)) if actual == return_span)
            || self.borrowed.get(&lease.binding) != Some(&lease.result)
            || self.result_source_loans.get(&lease.binding) != Some(&lease.sources)
            || self.bindings.get(&lease.root).copied() != lease.owner
            || self.borrowed.get(&lease.root).copied() != lease.parameter
            || self.dead.contains(&lease.root) != lease.owner.is_some()
            || !self.pending_call_loans.is_empty()
            || !self.active_views.is_empty()
            || !self.pending_views.is_empty()
            || lease.sources.contains(&lease.result)
            || lease
                .sources
                .iter()
                .enumerate()
                .any(|(index, source)| lease.sources[..index].contains(source))
        {
            return Err(invalid());
        }
        let count = 1 + lease.sources.len() + usize::from(lease.owner.is_some());
        let start = original
            .statements
            .len()
            .checked_sub(count)
            .ok_or_else(invalid)?;
        if captured.is_some_and(|captured| captured != start) {
            return Err(invalid());
        }
        let cleanup = &original.statements[start..];
        for (statement, loan) in cleanup
            .iter()
            .zip(std::iter::once(lease.result).chain(lease.sources.iter().rev().copied()))
        {
            if !statement.results.is_empty()
                || !matches!(statement.op, ROp::BorrowEnd { loan: actual } if actual == loan)
            {
                return Err(invalid());
            }
        }
        if let Some(owner) = lease.owner {
            let last = cleanup.last().ok_or_else(invalid)?;
            if !last.results.is_empty()
                || !matches!(last.op, ROp::Drop { owner: actual } if actual == owner)
            {
                return Err(invalid());
            }
        }
        Ok(())
    }
}
