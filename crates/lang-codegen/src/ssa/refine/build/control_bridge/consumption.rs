//! One-shot emission of the bridge body's formal-header Replace/return/drop rule.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Stage {
    Ready,
    Replaced(Slot),
    Bound(Slot),
    Dropped,
}
pub(in super::super) struct Permit {
    index: usize,
    control: UnitStatementId,
    block: BlockRef,
    instruction: usize,
    span: Span,
    stage: Stage,
    source: Slot,
    replacement: Option<Slot>,
}
impl Builder<'_, '_> {
    pub(in super::super) fn bridge_replace_authorization(
        &self,
        call: UnitExpressionId,
        root: Slot,
        replacement: Slot,
        span: Span,
    ) -> Result<Option<Permit>, LoweringError> {
        let Some(selected) =
            self.bridge_select_body(|r| r.steps().iter().any(|s| s.call() == call))?
        else {
            return Ok(None);
        };
        let rule = selected.rule;
        let Some(index) = rule.steps().iter().position(|s| s.call() == call) else {
            return Ok(None);
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let body = selected.block;
        let roots = selected.roots;
        let step = &rule.steps()[index];
        let position = rule
            .header_roots()
            .iter()
            .position(|r| *r == step.header_root())
            .ok_or_else(fail)?;
        if roots.get(position) != Some(&root)
            || selected.stages[index] != Stage::Ready
            || selected.stages[..index]
                .iter()
                .any(|s| *s != Stage::Dropped)
        {
            return Err(fail());
        }
        let origin = rule
            .body_catalog()
            .definition(step.producer())
            .ok_or_else(fail)?
            .origin();
        if self.multi_root_literal(origin, body)? != replacement {
            return Err(fail());
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::MissingBridgeReplacePermit,
        ) {
            return Ok(None);
        }
        self.bridge_permit(
            selected.control,
            index,
            Stage::Ready,
            root,
            Some(replacement),
            span,
        )
        .map(Some)
    }
    pub(in super::super) fn bridge_binding_authorization(
        &self,
        initializer: UnitExpressionId,
        symbol: UnitSymbolId,
        value: Slot,
        span: Span,
    ) -> Result<Option<Permit>, LoweringError> {
        let Some(selected) = self.bridge_select_body(|r| {
            r.steps().iter().any(|s| {
                s.call() == initializer
                    || s.returned_value_drop().target() == UnitDropTarget::Named(symbol)
            })
        })?
        else {
            return Ok(None);
        };
        let rule = selected.rule;
        let Some(index) = rule.steps().iter().position(|s| {
            s.call() == initializer
                || s.returned_value_drop().target() == UnitDropTarget::Named(symbol)
        }) else {
            return Ok(None);
        };
        let step = &rule.steps()[index];
        if step.call() != initializer
            || step.returned_value_drop().target() != UnitDropTarget::Named(symbol)
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::MissingBridgeBindingPermit,
        ) {
            return Ok(None);
        }
        self.bridge_permit(
            selected.control,
            index,
            Stage::Replaced(value),
            value,
            None,
            span,
        )
        .map(Some)
    }
    pub(in super::super) fn bridge_drop_authorization(
        &self,
        fact_index: usize,
        target: UnitDropTarget,
        value: Slot,
        span: Span,
    ) -> Result<Option<Permit>, LoweringError> {
        let Some(selected) = self.bridge_select_body(|r| {
            r.steps()
                .iter()
                .any(|s| s.returned_value_drop().target() == target)
        })?
        else {
            return Ok(None);
        };
        let rule = selected.rule;
        let Some(index) = rule
            .steps()
            .iter()
            .position(|s| s.returned_value_drop().target() == target)
        else {
            return Ok(None);
        };
        if self.inputs.owned.drops().get(fact_index)
            != Some(&rule.steps()[index].returned_value_drop())
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::MissingBridgeDropPermit,
        ) {
            return Ok(None);
        }
        #[cfg(test)]
        if self
            .control_chain_bodies
            .as_ref()
            .is_some_and(|g| g.is_last(selected.control))
            && owner_consumer_test_support::take_fault(
                owner_consumer_test_support::OwnerFault::ChainLastBodyMissingDrop,
            )
        {
            owner_consumer_test_support::record_action("control-chain-fault-hit", 1);
            return Ok(None);
        }
        #[cfg(test)]
        if self
            .control_chain_bodies
            .as_ref()
            .is_some_and(|g| g.is_leading(selected.control))
            && owner_consumer_test_support::take_fault(
                owner_consumer_test_support::OwnerFault::ChainLeadingBodyMissingDrop,
            )
        {
            owner_consumer_test_support::record_action("control-chain-fault-hit", 2);
            return Ok(None);
        }
        self.bridge_permit(
            selected.control,
            index,
            Stage::Bound(value),
            value,
            None,
            span,
        )
        .map(Some)
    }
    fn bridge_permit(
        &self,
        control: UnitStatementId,
        index: usize,
        stage: Stage,
        source: Slot,
        replacement: Option<Slot>,
        span: Span,
    ) -> Result<Permit, LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let selected = self
            .bridge_select_body(|r| r.statement() == control)?
            .ok_or_else(fail)?;
        let body = selected.block;
        if self.block != Some(body) || selected.stages.get(index) != Some(&stage) {
            return Err(fail());
        }
        Ok(Permit {
            index,
            control,
            block: body,
            instruction: self.body.blocks[body.0].statements.len(),
            span,
            stage,
            source,
            replacement,
        })
    }
    pub(in super::super) fn bridge_emitted(
        &mut self,
        permit: Option<Permit>,
        output: Option<Slot>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(permit) = permit else { return Ok(()) };
        #[cfg(test)]
        let permit = {
            let mut permit = permit;
            if let Some(group) = &self.control_chain_bodies
                && owner_consumer_test_support::take_fault(
                    owner_consumer_test_support::OwnerFault::ChainBodyWrongControl,
                )
            {
                owner_consumer_test_support::record_action("control-chain-fault-hit", 1);
                permit.control = group
                    .other_control(permit.control)
                    .unwrap_or(permit.control);
            }
            if owner_consumer_test_support::take_fault(
                owner_consumer_test_support::OwnerFault::BridgeWrongInstruction,
            ) {
                permit.instruction += 1;
            }
            if permit.stage == Stage::Ready
                && owner_consumer_test_support::take_fault(
                    owner_consumer_test_support::OwnerFault::BridgeReplaceWrongRoot,
                )
            {
                permit.source = output.unwrap_or(permit.source);
            }
            if permit.index == 1
                && permit.stage == Stage::Ready
                && owner_consumer_test_support::take_fault(
                    owner_consumer_test_support::OwnerFault::BridgeSecondReplaceFailure,
                )
            {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            }
            permit
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let selected = self
            .bridge_select_body(|r| r.statement() == permit.control)?
            .ok_or_else(fail)?;
        if selected.block != permit.block {
            return Err(fail());
        }
        let block = &self.body.blocks[permit.block.0];
        let statement = block.statements.get(permit.instruction).ok_or_else(fail)?;
        if self.block != Some(permit.block)
            || block.statements.len() != permit.instruction + 1
            || statement.span != permit.span
        {
            return Err(fail());
        }
        let next = match (permit.stage, output) {
            (Stage::Ready, Some(old))
                if old != permit.source
                    && statement.results == [permit.source, old]
                    && matches!(statement.op, ROp::RootReplace { owner, replacement, .. } if owner == permit.source && Some(replacement) == permit.replacement) =>
            {
                Stage::Replaced(old)
            }
            (Stage::Replaced(source), Some(binding))
                if source == permit.source
                    && binding != source
                    && statement.results == [binding]
                    && matches!(statement.op, ROp::Alias { source: actual } if actual == source) =>
            {
                Stage::Bound(binding)
            }
            (Stage::Bound(source), None)
                if source == permit.source
                    && statement.results.is_empty()
                    && matches!(statement.op, ROp::Drop { owner } if owner == source) =>
            {
                Stage::Dropped
            }
            _ => return Err(fail()),
        };
        let stages = self.bridge_body_stages_mut(permit.control)?;
        #[cfg(test)]
        if matches!(permit.stage, Stage::Bound(_))
            && owner_consumer_test_support::take_fault(
                owner_consumer_test_support::OwnerFault::BridgeDropDuplicateCommit,
            )
        {
            stages[permit.index] = Stage::Dropped;
        }
        if stages.get(permit.index) != Some(&permit.stage) {
            return Err(fail());
        }
        stages[permit.index] = next;
        #[cfg(test)]
        owner_consumer_test_support::record_action(
            match next {
                Stage::Replaced(_) => "bridge-body-replace-consumed",
                Stage::Bound(_) => "bridge-body-binding-consumed",
                Stage::Dropped => "bridge-body-drop-consumed",
                Stage::Ready => unreachable!(),
            },
            permit.index,
        );
        Ok(())
    }
}
