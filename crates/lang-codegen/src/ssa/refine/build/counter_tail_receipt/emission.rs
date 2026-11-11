//! Mandatory one-shot emission permits for exact selected counter root replacements.
use super::*;

pub(in super::super) struct ReplacePermit {
    index: usize,
    call: UnitExpressionId,
    root: Slot,
    replacement: Slot,
    block: BlockRef,
    instruction: usize,
}
impl Builder<'_, '_> {
    pub(in super::super) fn counter_replace_authorization(
        &self,
        call: UnitExpressionId,
        root: Slot,
        replacement: Slot,
        span: Span,
    ) -> Result<Option<ReplacePermit>, LoweringError> {
        let Some(plan) = &self.counter_tail_plan else {
            return Ok(None);
        };
        let Some(index) = plan.evidence.steps().iter().position(|s| s.call() == call) else {
            return Ok(None);
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let step = &plan.evidence.steps()[index];
        let block = self.block.ok_or_else(fail)?;
        if plan.replaced[index]
            || plan.geometry.map(|g| g[2]) != Some(block)
            || plan.roots.get(index) != Some(&root)
        {
            return Err(fail());
        }
        let origin = plan
            .evidence
            .body_catalog()
            .definition(step.producer())
            .ok_or_else(fail)?
            .origin();
        if self.multi_root_literal(origin, block)? != replacement {
            return Err(fail());
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::MissingCounterReplacePermit,
        ) {
            return Ok(None);
        }
        Ok(Some(ReplacePermit {
            index,
            call,
            root,
            replacement,
            block,
            instruction: self.body.blocks[block.0].statements.len(),
        }))
    }
    pub(in super::super) fn counter_replace_emitted(
        &mut self,
        permit: Option<ReplacePermit>,
        old: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(permit) = permit else {
            return Ok(());
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        #[cfg(test)]
        let permit = {
            let mut permit = permit;
            if owner_consumer_test_support::take_fault(
                owner_consumer_test_support::OwnerFault::CounterReplaceWrongRoot,
            ) {
                permit.root = old;
            }
            permit
        };
        let block = &self.body.blocks[permit.block.0];
        let statement = block.statements.get(permit.instruction).ok_or_else(fail)?;
        if self.block != Some(permit.block)
            || block.statements.len() != permit.instruction + 1
            || statement.results != [permit.root, old]
            || old == permit.root
            || statement.span != self.span(permit.call.expression())
            || !matches!(statement.op, ROp::RootReplace { owner, replacement, .. } if owner == permit.root && replacement == permit.replacement)
        {
            return Err(fail());
        }
        let plan = self.counter_tail_plan.as_mut().ok_or_else(fail)?;
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::CounterReplaceDuplicateCommit,
        ) {
            plan.replaced[permit.index] = true;
        }
        if plan.replaced[permit.index] || plan.evidence.steps()[permit.index].call() != permit.call
        {
            return Err(fail());
        }
        plan.replaced[permit.index] = true;
        #[cfg(test)]
        owner_consumer_test_support::record_action("counter-replace-permit-consumed", permit.index);
        Ok(())
    }
}
