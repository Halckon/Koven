//! The sealed B suffix authorizes one selected value. Existing emitters retain all
//! physical effects; this plan owns admission, action order and cleanup reconciliation.
use super::{BlockRef, Builder, RTerm, RefineInputs, Slot};
use crate::ssa::lowering_support::{LoweringError, LoweringErrorKind, error};
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::{
        UnitDropPoint, UnitDropTarget, UnitOwnerCallable, UnitOwnerOrigin,
        UnitOwnerReplaceBreakConsumerActionV1 as Action, UnitOwnerReplaceBreakObservationV1,
        UnitOwnerTransportEvent,
    },
    source::Span,
    type_checking::{UnitExpressionId, UnitItemId, UnitStatementId},
};

pub(super) struct OwnerPlan<'a> {
    boundary: &'a UnitOwnerReplaceBreakObservationV1,
    producers: [UnitExpressionId; 2],
    produced: [Option<Slot>; 2],
    root: UnitSymbolId,
    initializer: UnitExpressionId,
    retired: UnitSymbolId,
    pub(super) drop_index: usize,
    root_slot: Option<Slot>,
    bound: Option<Slot>,
    exit: Option<BlockRef>,
    false_block: Option<BlockRef>,
    // Indexed by semantic role: condition-false P, then Break Q, never CFG order.
    incoming: [Option<BlockRef>; 2],
    replaced: bool,
    stage: u8,
}

/// An authorization can only be issued after this plan's actual Take and Bind.
#[derive(Clone, Copy)]
pub(super) struct DropReceipt {
    selected: Slot,
    bound: Slot,
    index: usize,
}

impl<'a> OwnerPlan<'a> {
    pub(super) fn new(inputs: &RefineInputs<'a>) -> Result<Option<Self>, LoweringError> {
        let Some(item) = inputs.function_item else {
            return Ok(None);
        };
        let callable = UnitOwnerCallable::Function(UnitItemId::new(inputs.source_unit, item));
        let mut found = inputs
            .owned
            .owner_observations()
            .iter()
            .filter(|observation| observation.callable() == callable)
            .filter_map(|observation| observation.replace_break_observation_v1())
            .filter(|boundary| boundary.suffix_consumer_v1().is_some());
        let Some(boundary) = found.next() else {
            return Ok(None);
        };
        let fail = || error(LoweringErrorKind::MissingFact, inputs.span);
        if found.next().is_some() || !boundary.is_consistent() {
            return Err(fail());
        }
        let consumer = boundary.suffix_consumer_v1().ok_or_else(fail)?;
        let [
            Action::Take(initializer, root),
            Action::Bind(binding, retired),
            Action::Drop(point, dropped),
        ] = consumer.actions()
        else {
            return Err(fail());
        };
        if initializer != binding
            || retired != dropped
            || *root != boundary.entry().symbol()
            || initializer.source_unit() != inputs.source_unit
        {
            return Err(fail());
        }
        let UnitDropPoint::AfterStatement(statement) = point else {
            return Err(fail());
        };
        if statement.source_unit() != inputs.source_unit {
            return Err(fail());
        }
        let p = boundary
            .input_producers()
            .definition(boundary.condition_false_exit().owner())
            .ok_or_else(fail)?;
        let q = boundary
            .body_producers()
            .definition(boundary.break_boundary().owner())
            .ok_or_else(fail)?;
        let (UnitOwnerOrigin::String(p), UnitOwnerOrigin::String(q)) = (p.origin(), q.origin())
        else {
            return Err(fail());
        };
        if consumer.incoming()
            != &[
                boundary.condition_false_exit().clone(),
                boundary.break_boundary().clone(),
            ]
        {
            return Err(fail());
        }
        let indices = inputs
            .owned
            .drops()
            .iter()
            .enumerate()
            .filter(|(_, fact)| {
                fact.point() == *point && fact.target() == UnitDropTarget::Named(*retired)
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let [drop_index] = indices.as_slice() else {
            return Err(fail());
        };
        Ok(Some(Self {
            boundary,
            producers: [p, q],
            produced: [None, None],
            root: *root,
            initializer: *initializer,
            retired: *retired,
            drop_index: *drop_index,
            root_slot: None,
            bound: None,
            exit: None,
            false_block: None,
            incoming: [None, None],
            replaced: false,
            stage: 0,
        }))
    }
}

impl Builder<'_, '_> {
    #[cfg(test)]
    pub(super) fn owner_is_suffix(&self, symbol: UnitSymbolId) -> bool {
        self.owner_plan
            .as_ref()
            .is_some_and(|plan| plan.retired == symbol)
    }

    pub(super) fn owner_result(
        &mut self,
        expression: UnitExpressionId,
        slot: Slot,
    ) -> Result<(), LoweringError> {
        if let Some(plan) = &mut self.owner_plan {
            for role in 0..2 {
                if expression == plan.producers[role] && plan.produced[role].replace(slot).is_some()
                {
                    return Err(error(LoweringErrorKind::InvalidModel, self.inputs.span));
                }
            }
        }
        Ok(())
    }

    /// A selected Name is read from the admitted plan, not independently from old facts.
    pub(super) fn owner_name(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Slot, LoweringError> {
        let unit = self.unit(expression);
        #[cfg(test)]
        if let Some(plan) = self
            .owner_plan
            .as_mut()
            .filter(|plan| plan.initializer == unit)
        {
            use super::owner_consumer_test_support::{OwnerFault, take_fault};
            if take_fault(OwnerFault::MissingIncoming) {
                plan.incoming[0] = None;
            }
            if take_fault(OwnerFault::ForeignIncoming) {
                plan.incoming[1] = Some(BlockRef(usize::MAX));
            }
            if take_fault(OwnerFault::ReversedIncomingRoles) {
                plan.incoming.swap(0, 1);
            }
        }
        let Some(plan) = self.owner_plan.as_ref().filter(|p| p.initializer == unit) else {
            return self.name(expression, span);
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let selected = plan.root_slot.ok_or_else(fail)?;
        let exit = plan.exit.ok_or_else(fail)?;
        let [Some(false_edge), Some(break_edge)] = plan.incoming else {
            return Err(fail());
        };
        if plan.stage != 0
            || !plan.replaced
            || self.block != Some(exit)
            || self.reference(span)? != plan.root
            || self.bindings.get(&plan.root) != Some(&selected)
            || false_edge == break_edge
            || plan.false_block != Some(false_edge)
        {
            return Err(fail());
        }
        let incoming = self
            .body
            .blocks
            .iter()
            .enumerate()
            .filter_map(|(index, block)| {
                block
                    .terminator
                    .as_ref()
                    .filter(|(term, _)| term.successors().contains(&exit))
                    .map(|_| BlockRef(index))
            })
            .collect::<Vec<_>>();
        if incoming.len() != 2 || !incoming.contains(&false_edge) || !incoming.contains(&break_edge)
            || [false_edge, break_edge].iter().any(|edge| !matches!(self.body.blocks[edge.0].terminator, Some((RTerm::Goto(target), _)) if target == exit)) {
            return Err(fail());
        }
        self.owner_plan.as_mut().ok_or_else(fail)?.stage = 1;
        #[cfg(test)]
        {
            super::owner_consumer_test_support::record_action("take", selected.0);
            if super::owner_consumer_test_support::take_fault(
                super::owner_consumer_test_support::OwnerFault::DuplicateTake,
            ) {
                return self.owner_name(expression, span);
            }
        }
        Ok(selected)
    }

    /// Validate before the existing Alias binding is emitted; commit after it exists.
    pub(super) fn owner_binding_authorization(
        &self,
        initializer: UnitExpressionId,
        symbol: UnitSymbolId,
        value: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(plan) = &self.owner_plan else {
            return Ok(());
        };
        let invalid = if symbol == plan.root {
            initializer != plan.producers[0]
                || plan.produced[0] != Some(value)
                || plan.root_slot.is_some()
        } else if initializer == plan.initializer || symbol == plan.retired {
            initializer != plan.initializer
                || symbol != plan.retired
                || plan.stage != 1
                || plan.root_slot != Some(value)
                || plan.bound.is_some()
        } else {
            false
        };
        if invalid {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        Ok(())
    }

    pub(super) fn owner_binding_emitted(&mut self, symbol: UnitSymbolId, binding: Slot) {
        if let Some(plan) = &mut self.owner_plan {
            if symbol == plan.root {
                plan.root_slot = Some(binding);
                #[cfg(test)]
                super::owner_consumer_test_support::record_action("root", binding.0);
            }
            if symbol == plan.retired {
                plan.bound = Some(binding);
                plan.stage = 2;
                #[cfg(test)]
                super::owner_consumer_test_support::record_action("bind", binding.0);
            }
        }
    }

    pub(super) fn owner_while(
        &mut self,
        statement: UnitStatementId,
        condition: UnitExpressionId,
        body: UnitStatementId,
        finished: BlockRef,
        exit: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(plan) = self
            .owner_plan
            .as_mut()
            .filter(|p| p.boundary.statement() == statement)
        else {
            return Ok(());
        };
        plan.false_block = Some(finished);
        if plan.boundary.condition() != condition
            || plan.boundary.body() != body
            || plan.root_slot.is_none()
            || self.bindings.get(&plan.root).copied() != plan.root_slot
            || plan.exit.replace(exit).is_some()
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        Ok(())
    }

    pub(super) fn owner_replace(
        &mut self,
        call: UnitExpressionId,
        root: Slot,
        replacement: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(plan) = &mut self.owner_plan else {
            return Ok(());
        };
        let expected = plan.boundary.body_events().iter().find_map(|event| {
            if let UnitOwnerTransportEvent::Replace { call, .. } = event {
                Some(*call)
            } else {
                None
            }
        });
        if expected != Some(call) {
            return Ok(());
        }
        if plan.replaced
            || plan.exit.is_none()
            || plan.root_slot != Some(root)
            || plan.produced[1] != Some(replacement)
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        plan.replaced = true;
        Ok(())
    }

    pub(super) fn owner_false_edge(
        &mut self,
        statement: UnitStatementId,
        target: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        let from = self.current(span)?;
        let Some(plan) = self
            .owner_plan
            .as_mut()
            .filter(|p| p.boundary.statement() == statement)
        else {
            return Ok(());
        };
        if plan.exit != Some(target) || plan.incoming[0].replace(from).is_some() {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        Ok(())
    }

    pub(super) fn owner_break_edge(
        &mut self,
        expression: UnitExpressionId,
        target: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        let from = self.current(span)?;
        let Some(plan) = self
            .owner_plan
            .as_mut()
            .filter(|p| p.boundary.break_expression() == expression)
        else {
            return Ok(());
        };
        if !plan.replaced || plan.exit != Some(target) || plan.incoming[1].replace(from).is_some() {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        Ok(())
    }

    pub(super) fn owner_drop_authorization(
        &self,
        index: usize,
        target: UnitDropTarget,
        slot: Slot,
        span: Span,
    ) -> Result<Option<DropReceipt>, LoweringError> {
        let Some(plan) = &self.owner_plan else {
            return Ok(None);
        };
        #[cfg(test)]
        let (index, slot) = {
            use super::owner_consumer_test_support::{OwnerFault, take_fault};
            if target == UnitDropTarget::Named(plan.retired) {
                (
                    if take_fault(OwnerFault::WrongIndex) {
                        usize::MAX
                    } else {
                        index
                    },
                    if take_fault(OwnerFault::WrongSlot) {
                        Slot(usize::MAX)
                    } else {
                        slot
                    },
                )
            } else {
                (index, slot)
            }
        };
        if index != plan.drop_index && target != UnitDropTarget::Named(plan.retired) {
            return Ok(None);
        }
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        if index != plan.drop_index
            || target != UnitDropTarget::Named(plan.retired)
            || plan.stage != 2
            || plan.bound != Some(slot)
            || self.consumed_drops.contains(&index)
        {
            return Err(fail());
        }
        Ok(Some(DropReceipt {
            selected: plan.root_slot.ok_or_else(fail)?,
            bound: slot,
            index,
        }))
    }

    pub(super) fn owner_drop_emitted(
        &mut self,
        receipt: Option<DropReceipt>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if let Some(receipt) = receipt {
            let plan = self
                .owner_plan
                .as_mut()
                .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?;
            if plan.stage != 2
                || plan.drop_index != receipt.index
                || plan.bound != Some(receipt.bound)
                || plan.root_slot != Some(receipt.selected)
            {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            }
            plan.stage = 3;
            #[cfg(test)]
            {
                super::owner_consumer_test_support::record_action("drop", receipt.bound.0);
                if super::owner_consumer_test_support::take_fault(
                    super::owner_consumer_test_support::OwnerFault::DuplicateDrop,
                ) {
                    let target = UnitDropTarget::Named(plan.retired);
                    self.owner_drop_authorization(receipt.index, target, receipt.bound, span)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn finish_owner_plan(&self) -> Result<(), LoweringError> {
        if let Some(plan) = &self.owner_plan
            && (plan.stage != 3 || !self.consumed_drops.contains(&plan.drop_index))
        {
            return Err(error(LoweringErrorKind::MissingFact, self.inputs.span));
        }
        Ok(())
    }
}

#[cfg(test)]
impl DropReceipt {
    pub(super) fn witness(self) -> (usize, usize, usize) {
        (self.selected.0, self.bound.0, self.index)
    }
}
