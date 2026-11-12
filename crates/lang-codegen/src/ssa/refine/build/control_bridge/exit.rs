//! One-shot permits for actual next-control implicit-false root drops.
use super::*;

pub(in super::super) struct Permit {
    index: usize,
    control: UnitStatementId,
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
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let actual = self.inputs.owned.drops().get(fact_index);
        let Some(selected) = self.bridge_select_exit(|_, drops, consumed| {
            drops.iter().enumerate().any(|(i, fact)| {
                actual == Some(fact) || (fact.target() == target && consumed.get(i) == Some(&false))
            })
        })?
        else {
            return Ok(None);
        };
        let drops = selected.drops;
        let consumed = selected.consumed;
        // A completed exit does not own a later generation of the same source symbol.
        // The exact old fact still matches, so duplicate old cleanup remains an error.
        let Some(index) = drops.iter().enumerate().position(|(i, fact)| {
            actual == Some(fact) || (fact.target() == target && consumed.get(i) == Some(&false))
        }) else {
            return Ok(None);
        };
        let finished = selected.finished;
        let roots = selected.roots;
        #[cfg(test)]
        let target = if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::BridgeExitWrongFactTarget,
        ) {
            let other = selected
                .symbols
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
        let position = selected
            .symbols
            .iter()
            .position(|r| *r == root)
            .ok_or_else(fail)?;
        if drops[index].target() != target
            || self.block != Some(finished)
            || roots.get(position) != Some(&slot)
            || self.inputs.owned.drops().get(fact_index) != Some(&drops[index])
            || consumed.get(index) != Some(&false)
            || consumed[..index].iter().any(|done| !done)
            || selected
                .stages
                .iter()
                .any(|s| *s != consumption::Stage::Dropped)
        {
            return Err(fail());
        }
        #[cfg(test)]
        if self
            .control_chain_bodies
            .as_ref()
            .is_some_and(|g| g.is_leading(selected.control))
            && owner_consumer_test_support::take_fault(
                owner_consumer_test_support::OwnerFault::ChainLeadingExitMissingPermit,
            )
        {
            owner_consumer_test_support::record_action("control-chain-leading-exit-fault-hit", 1);
            return Ok(None);
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::MissingBridgeExitDropPermit,
        ) {
            if self.control_chain_bodies.is_some() {
                owner_consumer_test_support::record_action("control-chain-exit-fault-hit", 1);
            }
            return Ok(None);
        }
        Ok(Some(Permit {
            index,
            control: selected.control,
            block: finished,
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
            if let Some(group) = &self.control_chain_bodies
                && owner_consumer_test_support::take_fault(
                    owner_consumer_test_support::OwnerFault::ChainExitWrongControl,
                )
            {
                owner_consumer_test_support::record_action("control-chain-exit-fault-hit", 1);
                permit.control = group
                    .other_control(permit.control)
                    .unwrap_or(permit.control);
            }
            if owner_consumer_test_support::take_fault(
                owner_consumer_test_support::OwnerFault::BridgeExitWrongInstruction,
            ) {
                permit.instruction += 1;
            }
            permit
        };
        let selected = self
            .bridge_select_exit(|control, _, _| control == permit.control)?
            .ok_or_else(fail)?;
        if selected.finished != permit.block {
            return Err(fail());
        }
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
            .bridge_exit_states_mut(permit.control)?
            .get_mut(permit.index)
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
