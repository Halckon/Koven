//! Exact straight-line live-root move/binding/drop consumption after the false exit.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Stage {
    Ready,
    Bound(Slot),
    Dropped(Slot),
}
pub(in super::super) struct Permit {
    index: usize,
    control: UnitStatementId,
    block: BlockRef,
    instruction: usize,
    span: Span,
    source: Slot,
    stage: Stage,
}
impl Builder<'_, '_> {
    pub(in super::super) fn bridge_suffix_binding(
        &self,
        initializer: UnitExpressionId,
        symbol: UnitSymbolId,
        value: Slot,
        span: Span,
    ) -> Result<Option<Permit>, LoweringError> {
        let Some(selected) = self.bridge_select_suffix(|_, items| {
            items
                .iter()
                .any(|(_, f)| f.target() == UnitDropTarget::Named(symbol))
        })?
        else {
            return Ok(None);
        };
        let items = selected.items;
        let Some(index) = items
            .iter()
            .position(|(_, fact)| fact.target() == UnitDropTarget::Named(symbol))
        else {
            return Ok(None);
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let roots = selected.roots;
        let root = items[index].0;
        let position = selected
            .symbols
            .iter()
            .position(|r| *r == root)
            .ok_or_else(fail)?;
        if self.pending_symbol(initializer) != Some(root)
            || roots.get(position) != Some(&value)
            || self.bindings.get(&root) != Some(&value)
        {
            return Err(fail());
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::MissingBridgeSuffixBinding,
        ) {
            if self.control_chain_bodies.is_some() {
                owner_consumer_test_support::record_action("control-chain-suffix-fault-hit", index);
            }
            return Ok(None);
        }
        self.bridge_suffix_permit(selected.control, index, Stage::Ready, value, span)
            .map(Some)
    }
    pub(in super::super) fn bridge_suffix_drop(
        &self,
        fact_index: usize,
        target: UnitDropTarget,
        value: Slot,
        span: Span,
    ) -> Result<Option<Permit>, LoweringError> {
        let actual = self.inputs.owned.drops().get(fact_index);
        let Some(selected) = self.bridge_select_suffix(|_, items| {
            items
                .iter()
                .any(|(_, f)| actual == Some(f) || f.target() == target)
        })?
        else {
            return Ok(None);
        };
        let items = selected.items;
        let Some(index) = items
            .iter()
            .position(|(_, fact)| actual == Some(fact) || fact.target() == target)
        else {
            return Ok(None);
        };
        if actual != Some(&items[index].1) || target != items[index].1.target() {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::MissingBridgeSuffixDrop,
        ) {
            if self.control_chain_bodies.is_some() {
                owner_consumer_test_support::record_action("control-chain-suffix-fault-hit", index);
            }
            return Ok(None);
        }
        self.bridge_suffix_permit(selected.control, index, Stage::Bound(value), value, span)
            .map(Some)
    }
    fn bridge_suffix_permit(
        &self,
        control: UnitStatementId,
        index: usize,
        stage: Stage,
        source: Slot,
        span: Span,
    ) -> Result<Permit, LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let selected = self
            .bridge_select_suffix(|c, _| c == control)?
            .ok_or_else(fail)?;
        let exit = selected.exit;
        let stages = selected.stages;
        let instruction = index * 2 + usize::from(matches!(stage, Stage::Bound(_)));
        if self.block != Some(exit)
            || self.body.blocks[exit.0].statements.len() != instruction
            || stages.get(index) != Some(&stage)
            || stages[..index]
                .iter()
                .any(|s| !matches!(s, Stage::Dropped(_)))
            || !selected.complete
        {
            return Err(fail());
        }
        Ok(Permit {
            index,
            block: exit,
            control,
            instruction,
            span,
            source,
            stage,
        })
    }
    pub(in super::super) fn bridge_suffix_emitted(
        &mut self,
        permit: Option<Permit>,
        output: Option<Slot>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(permit) = permit else { return Ok(()) };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        #[cfg(test)]
        let permit = {
            let mut permit = permit;
            if owner_consumer_test_support::take_fault(
                owner_consumer_test_support::OwnerFault::BridgeSuffixWrongSource,
            ) {
                permit.source = Slot(usize::MAX);
                if self.control_chain_bodies.is_some() {
                    owner_consumer_test_support::record_action(
                        "control-chain-suffix-fault-hit",
                        permit.index,
                    );
                }
            }
            permit
        };
        let block = &self.body.blocks[permit.block.0];
        let statement = block.statements.get(permit.instruction).ok_or_else(fail)?;
        if self.block != Some(permit.block)
            || block.statements.len() != permit.instruction + 1
            || statement.span != permit.span
        {
            return Err(fail());
        }
        let next = match (permit.stage, output) {
            (Stage::Ready, Some(binding))
                if binding != permit.source
                    && statement.results == [binding]
                    && matches!(statement.op, ROp::Alias { source } if source == permit.source) =>
            {
                Stage::Bound(binding)
            }
            (Stage::Bound(binding), None)
                if statement.results.is_empty()
                    && matches!(statement.op, ROp::Drop { owner } if owner == binding && owner == permit.source) =>
            {
                Stage::Dropped(binding)
            }
            _ => return Err(fail()),
        };
        let selected = self
            .bridge_select_suffix(|c, _| c == permit.control)?
            .ok_or_else(fail)?;
        if selected.exit != permit.block || !selected.complete {
            return Err(fail());
        }
        #[cfg(test)]
        let is_group = self.control_chain_bodies.is_some();
        let state = self
            .bridge_suffix_states_mut(permit.control)?
            .get_mut(permit.index)
            .ok_or_else(fail)?;
        #[cfg(test)]
        if matches!(next, Stage::Dropped(_))
            && owner_consumer_test_support::take_fault(
                owner_consumer_test_support::OwnerFault::BridgeSuffixDuplicateDrop,
            )
        {
            if is_group {
                owner_consumer_test_support::record_action(
                    "control-chain-suffix-fault-hit",
                    permit.index,
                );
            }
            *state = next;
        }
        if *state != permit.stage {
            return Err(fail());
        }
        *state = next;
        #[cfg(test)]
        owner_consumer_test_support::record_action(
            if output.is_some() {
                "bridge-suffix-binding-consumed"
            } else {
                "bridge-suffix-drop-consumed"
            },
            permit.index,
        );
        Ok(())
    }
    pub(super) fn finish_bridge_suffix(&self) -> Result<(), LoweringError> {
        let Some(selected) = self.bridge_select_suffix(|_, _| true)? else {
            return Ok(());
        };
        let items = selected.items;
        let fail = || error(LoweringErrorKind::InvalidModel, self.inputs.span);
        let exit = selected.exit;
        let roots = selected.roots;
        let stages = selected.stages;
        if !selected.complete {
            return Err(fail());
        }
        if stages.len() != items.len() {
            return Err(fail());
        }
        let statements = &self.body.blocks[exit.0].statements;
        for (index, (root, _)) in items.iter().enumerate() {
            let Stage::Dropped(binding) = stages[index] else {
                return Err(fail());
            };
            let position = selected
                .symbols
                .iter()
                .position(|r| r == root)
                .ok_or_else(fail)?;
            let alias = statements.get(index * 2).ok_or_else(fail)?;
            let drop = statements.get(index * 2 + 1).ok_or_else(fail)?;
            if alias.results != [binding]
                || !matches!(alias.op, ROp::Alias { source } if source == roots[position])
                || !drop.results.is_empty()
                || !matches!(drop.op, ROp::Drop { owner } if owner == binding)
            {
                return Err(fail());
            }
        }
        Ok(())
    }
}
