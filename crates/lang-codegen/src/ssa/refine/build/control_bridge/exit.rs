//! One-shot permits for actual next-control implicit-false root drops.
use super::*;

pub(in super::super) struct Permit {
    index: usize,
    block: BlockRef,
    instruction: usize,
    root: Slot,
    span: Span,
}
impl Builder<'_, '_> {
    pub(in super::super) fn bridge_exit_drop_authorization(
        &self,
        fact_index: usize,
        target: UnitDropTarget,
        slot: Slot,
        span: Span,
    ) -> Result<Option<Permit>, LoweringError> {
        let Some(plan) = &self.control_bridge_plan else {
            return Ok(None);
        };
        let Some((_, drops)) = plan.evidence.next_false_exit_partition() else {
            return Ok(None);
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let consumed = plan.exits.as_ref().ok_or_else(fail)?;
        let actual = self.inputs.owned.drops().get(fact_index);
        // A completed exit does not own a later generation of the same source symbol.
        // The exact old fact still matches, so duplicate old cleanup remains an error.
        let Some(index) = drops.iter().enumerate().position(|(i, fact)| {
            actual == Some(fact) || (fact.target() == target && consumed.get(i) == Some(&false))
        }) else {
            return Ok(None);
        };
        let (_, [_, _, finished, _], roots) = plan.next.as_ref().ok_or_else(fail)?;
        #[cfg(test)]
        let target = if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::BridgeExitWrongFactTarget,
        ) {
            let other = plan
                .evidence
                .targets()
                .iter()
                .copied()
                .find(|root| UnitDropTarget::Named(*root) != target)
                .ok_or_else(fail)?;
            UnitDropTarget::Named(other)
        } else {
            target
        };
        let UnitDropTarget::Named(root) = target else {
            return Err(fail());
        };
        let position = plan
            .evidence
            .targets()
            .iter()
            .position(|r| *r == root)
            .ok_or_else(fail)?;
        if drops[index].target() != target
            || self.block != Some(*finished)
            || roots.get(position) != Some(&slot)
            || self.inputs.owned.drops().get(fact_index) != Some(&drops[index])
            || consumed.get(index) != Some(&false)
            || consumed[..index].iter().any(|done| !done)
            || plan.chain.iter().any(|s| *s != consumption::Stage::Dropped)
        {
            return Err(fail());
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::MissingBridgeExitDropPermit,
        ) {
            return Ok(None);
        }
        Ok(Some(Permit {
            index,
            block: *finished,
            instruction: self.body.blocks[finished.0].statements.len(),
            root: slot,
            span,
        }))
    }
    pub(in super::super) fn bridge_exit_drop_emitted(
        &mut self,
        permit: Option<Permit>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(permit) = permit else { return Ok(()) };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        #[cfg(test)]
        let permit = {
            let mut permit = permit;
            if owner_consumer_test_support::take_fault(
                owner_consumer_test_support::OwnerFault::BridgeExitWrongInstruction,
            ) {
                permit.instruction += 1;
            }
            permit
        };
        let block = &self.body.blocks[permit.block.0];
        let statement = block.statements.get(permit.instruction).ok_or_else(fail)?;
        if self.block != Some(permit.block)
            || block.statements.len() != permit.instruction + 1
            || statement.span != permit.span
            || !statement.results.is_empty()
            || !matches!(statement.op, ROp::Drop { owner } if owner == permit.root)
        {
            return Err(fail());
        }
        let done = self
            .control_bridge_plan
            .as_mut()
            .and_then(|p| p.exits.as_mut())
            .and_then(|items| items.get_mut(permit.index))
            .ok_or_else(fail)?;
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::BridgeExitDuplicateCommit,
        ) {
            *done = true;
        }
        if *done {
            return Err(fail());
        }
        *done = true;
        #[cfg(test)]
        owner_consumer_test_support::record_action("bridge-exit-drop-consumed", permit.index);
        Ok(())
    }
}
