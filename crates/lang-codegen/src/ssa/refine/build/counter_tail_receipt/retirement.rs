//! Selected Replace -> returned binding -> actual drop, with one-shot emission permits.
use super::*;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Stage {
    Ready,
    Replaced(Slot),
    Bound(Slot),
    Dropped,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Domain {
    Body,
    Exit,
    Entry,
}
pub(in super::super) struct Permit {
    pub(super) index: usize,
    pub(super) block: BlockRef,
    pub(super) instruction: usize,
    pub(super) source: Slot,
    pub(super) span: Span,
    pub(super) domain: Domain,
}
impl Builder<'_, '_> {
    pub(in super::super) fn counter_binding_authorization(
        &self,
        initializer: UnitExpressionId,
        symbol: UnitSymbolId,
        value: Slot,
        span: Span,
    ) -> Result<Option<Permit>, LoweringError> {
        let Some(plan) = &self.counter_tail_plan else {
            return Ok(None);
        };
        let Some(index) = plan.evidence.steps().iter().position(|s| {
            s.call() == initializer
                || s.returned_value_drop().target() == UnitDropTarget::Named(symbol)
        }) else {
            return self.counter_entry_binding(initializer, symbol, value, span);
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let step = &plan.evidence.steps()[index];
        if step.call() != initializer
            || step.returned_value_drop().target() != UnitDropTarget::Named(symbol)
            || plan.chain[index] != Stage::Replaced(value)
        {
            return Err(fail());
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::MissingCounterBindingPermit,
        ) {
            return Ok(None);
        }
        self.counter_retirement_permit(index, value, span).map(Some)
    }
    fn counter_retirement_permit(
        &self,
        index: usize,
        source: Slot,
        span: Span,
    ) -> Result<Permit, LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let plan = self.counter_tail_plan.as_ref().ok_or_else(fail)?;
        let block = self.block.ok_or_else(fail)?;
        if plan.geometry.map(|g| g[2]) != Some(block) {
            return Err(fail());
        }
        Ok(Permit {
            domain: Domain::Body,
            index,
            source,
            block,
            instruction: self.body.blocks[block.0].statements.len(),
            span,
        })
    }
    pub(in super::super) fn counter_binding_emitted(
        &mut self,
        permit: Option<Permit>,
        binding: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(permit) = permit else {
            return Ok(());
        };
        if permit.domain == Domain::Entry {
            return self.counter_entry_emitted(permit, binding, span);
        }
        if permit.domain == Domain::Exit {
            return self.counter_exit_binding_emitted(permit, binding, span);
        }
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        #[cfg(test)]
        let permit = {
            let mut permit = permit;
            if owner_consumer_test_support::take_fault(
                owner_consumer_test_support::OwnerFault::CounterBindingWrongSource,
            ) {
                permit.source = binding;
            }
            permit
        };
        let statement = self.counter_retirement_statement(&permit, span)?;
        if statement.results != [binding]
            || binding == permit.source
            || !matches!(statement.op, ROp::Alias { source } if source == permit.source)
        {
            return Err(fail());
        }
        let plan = self.counter_tail_plan.as_mut().ok_or_else(fail)?;
        if plan.chain[permit.index] != Stage::Replaced(permit.source) {
            return Err(fail());
        }
        plan.chain[permit.index] = Stage::Bound(binding);
        #[cfg(test)]
        owner_consumer_test_support::record_action("counter-binding-permit-consumed", permit.index);
        Ok(())
    }
    pub(in super::super) fn counter_drop_authorization(
        &self,
        fact_index: usize,
        target: UnitDropTarget,
        slot: Slot,
        span: Span,
    ) -> Result<Option<Permit>, LoweringError> {
        let Some(plan) = &self.counter_tail_plan else {
            return Ok(None);
        };
        let Some(index) = plan
            .evidence
            .steps()
            .iter()
            .position(|s| s.returned_value_drop().target() == target)
        else {
            return self.counter_exit_drop(fact_index, target, slot, span);
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        if self.inputs.owned.drops().get(fact_index)
            != Some(&plan.evidence.steps()[index].returned_value_drop())
            || plan.chain[index] != Stage::Bound(slot)
        {
            return Err(fail());
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::MissingCounterDropPermit,
        ) {
            return Ok(None);
        }
        self.counter_retirement_permit(index, slot, span).map(Some)
    }
    pub(in super::super) fn counter_drop_emitted(
        &mut self,
        permit: Option<Permit>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(permit) = permit else {
            return Ok(());
        };
        if permit.domain == Domain::Exit {
            return self.counter_exit_drop_emitted(permit, span);
        }
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let statement = self.counter_retirement_statement(&permit, span)?;
        if !statement.results.is_empty()
            || !matches!(statement.op, ROp::Drop { owner } if owner == permit.source)
        {
            return Err(fail());
        }
        let plan = self.counter_tail_plan.as_mut().ok_or_else(fail)?;
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::CounterDropDuplicateCommit,
        ) {
            plan.chain[permit.index] = Stage::Dropped;
        }
        if plan.chain[permit.index] != Stage::Bound(permit.source) {
            return Err(fail());
        }
        plan.chain[permit.index] = Stage::Dropped;
        #[cfg(test)]
        owner_consumer_test_support::record_action("counter-drop-permit-consumed", permit.index);
        Ok(())
    }
    pub(super) fn counter_retirement_statement(
        &self,
        permit: &Permit,
        span: Span,
    ) -> Result<&super::super::super::body::RStmt, LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let block = &self.body.blocks[permit.block.0];
        if self.block != Some(permit.block) || block.statements.len() != permit.instruction + 1 {
            return Err(fail());
        }
        let statement = block.statements.get(permit.instruction).ok_or_else(fail)?;
        if statement.span != permit.span {
            return Err(fail());
        }
        Ok(statement)
    }
}
