//! Required entry-root binding permits, grounded in the original catalog and actual slots.
use super::*;
use lang_frontend::ownership_checking::UnitOwnerOrigin;
use retirement::{Domain, Permit};
impl Builder<'_, '_> {
    pub(super) fn counter_entry_binding(
        &self,
        initializer: UnitExpressionId,
        symbol: UnitSymbolId,
        value: Slot,
        span: Span,
    ) -> Result<Option<Permit>, LoweringError> {
        let Some(plan) = &self.counter_tail_plan else {
            return Ok(None);
        };
        let Some(index) = plan
            .evidence
            .root_slots()
            .iter()
            .position(|s| s.root() == symbol)
        else {
            return self.counter_exit_binding(initializer, symbol, value, span);
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let block = self.block.ok_or_else(fail)?;
        if plan.geometry.is_some() || plan.initial[index].is_some() {
            return Err(fail());
        }
        let origin = plan
            .evidence
            .input_catalog()
            .definition(plan.evidence.root_slots()[index].entry())
            .ok_or_else(fail)?
            .origin();
        match origin {
            UnitOwnerOrigin::String(id) => {
                let resolved = multi_root_receipt::entry::prefix(
                    &self.body,
                    block,
                    value,
                    self.span(id.expression()),
                )
                .ok_or_else(fail)?;
                if self.multi_root_literal(origin, resolved.producer_block)? != resolved.producer {
                    return Err(fail());
                }
            }
            UnitOwnerOrigin::Parameter(parameter) => {
                let i = self
                    .inputs
                    .parameters
                    .iter()
                    .position(|p| *p == parameter)
                    .ok_or_else(fail)?;
                if self.inputs.modes.get(i) != Some(&ParameterMode::Value) {
                    return Err(fail());
                }
                let parameter_slot = *self
                    .body
                    .parameters
                    .get(i + usize::from(self.inputs.receiver.is_some()))
                    .ok_or_else(fail)?;
                if self.bindings.get(&parameter) != Some(&parameter_slot)
                    || !self.counter_parameter_chain(block, value, parameter_slot)
                {
                    return Err(fail());
                }
            }
            _ => return Err(fail()),
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::MissingCounterEntryPermit,
        ) {
            return Ok(None);
        }
        Ok(Some(Permit {
            index,
            block,
            source: value,
            span,
            instruction: self.body.blocks[block.0].statements.len(),
            domain: Domain::Entry,
        }))
    }
    fn counter_parameter_chain(&self, block: BlockRef, mut value: Slot, parameter: Slot) -> bool {
        let statements = &self.body.blocks[block.0].statements;
        let mut before = statements.len();
        loop {
            if value == parameter {
                return !statements[..before].iter().any(|s| {
                    s.results.contains(&parameter) || s.op.operands().contains(&parameter)
                });
            }
            let mut definition = None;
            for (i, s) in statements[..before].iter().enumerate().rev() {
                if s.op.operands().contains(&value) {
                    return false;
                }
                if s.results.contains(&value) {
                    definition = Some((i, s));
                    break;
                }
            }
            let Some((i, s)) = definition else {
                return false;
            };
            let ROp::Alias { source } = s.op else {
                return false;
            };
            if s.results != [value] {
                return false;
            }
            before = i;
            value = source;
        }
    }
    pub(super) fn counter_entry_emitted(
        &mut self,
        permit: Permit,
        binding: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        #[cfg(test)]
        let permit = {
            let mut permit = permit;
            if owner_consumer_test_support::take_fault(
                owner_consumer_test_support::OwnerFault::CounterEntryWrongSource,
            ) {
                permit.source = binding;
            }
            permit
        };
        let statement = self.counter_retirement_statement(&permit, span)?;
        if binding == permit.source
            || statement.results != [binding]
            || !matches!(statement.op, ROp::Alias { source } if source == permit.source)
        {
            return Err(fail());
        }
        let plan = self.counter_tail_plan.as_mut().ok_or_else(fail)?;
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::CounterEntryDuplicateCommit,
        ) {
            plan.initial[permit.index] = Some(binding);
        }
        if plan.geometry.is_some() || plan.initial[permit.index].is_some() {
            return Err(fail());
        }
        plan.initial[permit.index] = Some(binding);
        #[cfg(test)]
        owner_consumer_test_support::record_action("counter-entry-permit-consumed", permit.index);
        Ok(())
    }
}
