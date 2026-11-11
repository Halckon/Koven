//! The existing RootReplace and Drop emitters establish distinct installed/old slots.
#[cfg(test)]
use super::super::owner_consumer_test_support::{OwnerFault, take_fault};
use super::*;
use lang_frontend::{
    ownership_checking::UnitOwnerReplaceBreakReturnConsumerV1,
    type_checking::OwnershipPrimitiveKind,
};

pub(in super::super) struct ReturnReplacementPlan {
    pub(super) call: UnitExpressionId,
    pub(super) argument: UnitExpressionId,
    pub(super) produced: Option<Slot>,
    pub(in super::super) old: Option<Slot>,
    pub(in super::super) drop_index: usize,
}
impl ReturnReplacementPlan {
    pub(super) fn new(
        inputs: &RefineInputs<'_>,
        consumer: &UnitOwnerReplaceBreakReturnConsumerV1,
    ) -> Result<(Self, UnitSymbolId), LoweringError> {
        let fail = || error(LoweringErrorKind::MissingFact, inputs.span);
        let [
            ReturnAction::Create(argument, owner),
            ReturnAction::Pending(call, pending_arg, pending),
            ReturnAction::Replace(commit, root, installed),
            ReturnAction::Drop(point, dropped_root, dropped),
            ReturnAction::Result(result),
            ReturnAction::Return {
                expression,
                payload,
            },
        ] = consumer.actions()
        else {
            return Err(fail());
        };
        if argument != pending_arg
            || owner != pending
            || owner != installed
            || owner != dropped
            || call != commit
            || call != result
            || call != payload
            || root != dropped_root
            || *call != consumer.payload()
            || *expression != consumer.return_expression()
            || *point != UnitDropPoint::CallReturn(*call)
        {
            return Err(fail());
        }
        let catalog = consumer.replacement_producers().ok_or_else(fail)?;
        if catalog.definitions().len() != 1
            || catalog.definition(owner.clone()).map(|d| d.origin())
                != Some(UnitOwnerOrigin::String(*argument))
        {
            return Err(fail());
        }
        let typed = inputs.typed.ownership_primitive(*call).ok_or_else(fail)?;
        let owned = inputs.owned.ownership_primitive(*call).ok_or_else(fail)?;
        if typed.kind() != OwnershipPrimitiveKind::Replace
            || typed.operands().len() != 2
            || typed.operands()[1] != *argument
            || *owned.descriptor() != typed
            || owned.places().len() != 1
            || !owned.places()[0].is_root()
            || owned.places()[0].root() != *root
            || inputs.typed.types().get(typed.value_type())
                != Some(&lang_frontend::type_checking::UnitTypeKind::Builtin(
                    lang_frontend::type_checking::BuiltinType::String,
                ))
        {
            return Err(fail());
        }
        let drops = inputs
            .owned
            .drops()
            .iter()
            .enumerate()
            .filter(|(_, d)| d.target() == UnitDropTarget::Named(*root))
            .collect::<Vec<_>>();
        let [(index, drop)] = drops.as_slice() else {
            return Err(fail());
        };
        if drop.point() != *point {
            return Err(fail());
        }
        Ok((
            Self {
                call: *call,
                argument: *argument,
                produced: None,
                old: None,
                drop_index: *index,
            },
            *root,
        ))
    }
}
impl Builder<'_, '_> {
    pub(in super::super) fn authorize_return_replace(
        &mut self,
        call: UnitExpressionId,
        root: Slot,
        replacement: Slot,
        span: Span,
    ) -> Result<bool, LoweringError> {
        let Some(plan) = &self.owner_plan else {
            return Ok(false);
        };
        let Some(frame) = plan.return_replacement.as_ref().filter(|r| r.call == call) else {
            return Ok(false);
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let exit = plan.exit.ok_or_else(fail)?;
        let [Some(condition), Some(body)] = plan.incoming else {
            return Err(fail());
        };
        if plan.stage != 0
            || !plan.replaced
            || self.block != Some(exit)
            || plan.root_slot != Some(root)
            || self.bindings.get(&plan.root) != Some(&root)
            || self.dead.contains(&plan.root)
            || frame.produced != Some(replacement)
            || frame.old.is_some()
            || condition == body
            || plan.false_block != Some(condition)
        {
            return Err(fail());
        }
        let incoming = self
            .body
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(index, b)| {
                b.terminator
                    .as_ref()
                    .filter(|(term, _)| term.successors().contains(&exit))
                    .map(|_| BlockRef(index))
            })
            .collect::<Vec<_>>();
        if incoming.len() != 2 || !incoming.contains(&condition) || !incoming.contains(&body)
            || [condition, body].iter().any(|edge| !matches!(self.body.blocks[edge.0].terminator, Some((RTerm::Goto(target), _)) if target == exit)) { return Err(fail()); }
        self.owner_plan.as_mut().ok_or_else(fail)?.stage = 7;
        Ok(true)
    }
    pub(in super::super) fn return_replace_emitted(
        &mut self,
        call: UnitExpressionId,
        root: Slot,
        replacement: Slot,
        old: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(plan) = &mut self.owner_plan else {
            return Ok(());
        };
        let Some(frame) = plan.return_replacement.as_mut().filter(|r| r.call == call) else {
            return Ok(());
        };
        #[cfg(test)]
        if take_fault(OwnerFault::MissingReturnReplaceReceipt) {
            return Ok(());
        }
        #[cfg(test)]
        let old = if take_fault(OwnerFault::ReturnReplaceOldIsRoot) {
            root
        } else {
            old
        };
        if plan.stage != 7
            || plan.root_slot != Some(root)
            || frame.produced != Some(replacement)
            || old == root
            || old == replacement
            || frame.old.replace(old).is_some()
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        plan.stage = 8;
        #[cfg(test)]
        super::super::owner_consumer_test_support::record_action("replace_result", old.0);
        #[cfg(test)]
        if take_fault(OwnerFault::DuplicateReturnReplaceReceipt) {
            return self.return_replace_emitted(call, root, replacement, old, span);
        }
        Ok(())
    }
    pub(in super::super) fn authorize_return_replace_drop(
        &self,
        index: usize,
        target: UnitDropTarget,
        slot: Slot,
        span: Span,
    ) -> Result<Option<DropReceipt>, LoweringError> {
        let Some(plan) = &self.owner_plan else {
            return Ok(None);
        };
        let Some(frame) = &plan.return_replacement else {
            return Ok(None);
        };
        // Only the suffix's real cleanup may consume this CI-only mutation.
        #[cfg(test)]
        let slot = if index == frame.drop_index
            && target == UnitDropTarget::Named(plan.root)
            && take_fault(OwnerFault::ReturnReplaceDropIsOld)
        {
            frame
                .old
                .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?
        } else {
            slot
        };
        if frame.old == Some(slot) {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        if index != frame.drop_index && target != UnitDropTarget::Named(plan.root) {
            return Ok(None);
        }
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        if plan.stage != 8
            || self.block != plan.exit
            || index != frame.drop_index
            || target != UnitDropTarget::Named(plan.root)
            || plan.root_slot != Some(slot)
            || !self.dead.contains(&plan.root)
            || self.consumed_drops.contains(&index)
        {
            return Err(fail());
        }
        Ok(Some(DropReceipt {
            selected: frame.old.ok_or_else(fail)?,
            bound: slot,
            index,
            replacement: true,
        }))
    }
    pub(in super::super) fn return_replace_drop_emitted(
        &mut self,
        receipt: DropReceipt,
        span: Span,
    ) -> Result<(), LoweringError> {
        let plan = self
            .owner_plan
            .as_mut()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        let frame = plan
            .return_replacement
            .as_ref()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        #[cfg(test)]
        if take_fault(OwnerFault::MissingReturnReplaceDropReceipt) {
            return Ok(());
        }
        if plan.stage != 8
            || frame.old != Some(receipt.selected)
            || frame.drop_index != receipt.index
            || plan.root_slot != Some(receipt.bound)
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        plan.stage = 9;
        #[cfg(test)]
        super::super::owner_consumer_test_support::record_action(
            "replace_installed_drop",
            receipt.bound.0,
        );
        #[cfg(test)]
        if take_fault(OwnerFault::DuplicateReturnReplaceDropReceipt) {
            return self.return_replace_drop_emitted(receipt, span);
        }
        Ok(())
    }
}
