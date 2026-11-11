//! Complete false-exit/suffix consumption using the existing Alias/Drop emission sites.
use super::*;
use lang_frontend::ownership_checking::UnitDropFact;
use retirement::{Permit, Stage};
pub(super) struct Item {
    root: UnitSymbolId,
    fact: UnitDropFact,
    suffix: bool,
    pub(super) stage: Stage,
}
pub(super) fn items(evidence: &UnitOwnerCounterTailEvidenceV1) -> Option<Vec<Item>> {
    let (live, drops) = evidence.false_exit_partition()?;
    let suffix = evidence.false_exit_suffix().unwrap_or(&[]);
    if suffix.len() != live.len() {
        return None;
    }
    let mut items = Vec::new();
    for fact in drops {
        let UnitDropTarget::Named(root) = fact.target() else {
            return None;
        };
        items.push(Item {
            root,
            fact: *fact,
            suffix: false,
            stage: Stage::Ready,
        });
    }
    for (root, fact) in suffix {
        items.push(Item {
            root: *root,
            fact: *fact,
            suffix: true,
            stage: Stage::Ready,
        });
    }
    Some(items)
}
impl Builder<'_, '_> {
    fn counter_exit_permit(
        &self,
        index: usize,
        source: Slot,
        span: Span,
    ) -> Result<Permit, LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let plan = self.counter_tail_plan.as_ref().ok_or_else(fail)?;
        let items = plan.exits.as_ref().ok_or_else(fail)?;
        let block = self.block.ok_or_else(fail)?;
        let expected = plan.geometry.ok_or_else(fail)?[if items[index].suffix { 4 } else { 3 }];
        if block != expected
            || plan.chain.iter().any(|s| *s != Stage::Dropped)
            || items[..index].iter().any(|i| i.stage != Stage::Dropped)
        {
            return Err(fail());
        }
        Ok(Permit {
            index,
            block,
            source,
            span,
            instruction: self.body.blocks[block.0].statements.len(),
            exit: true,
        })
    }
    pub(super) fn counter_exit_binding(
        &self,
        initializer: UnitExpressionId,
        symbol: UnitSymbolId,
        value: Slot,
        span: Span,
    ) -> Result<Option<Permit>, LoweringError> {
        let Some(plan) = &self.counter_tail_plan else {
            return Ok(None);
        };
        let Some(items) = &plan.exits else {
            return Ok(None);
        };
        let Some(index) = items
            .iter()
            .position(|i| i.suffix && i.fact.target() == UnitDropTarget::Named(symbol))
        else {
            return Ok(None);
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let item = &items[index];
        if item.stage != Stage::Ready
            || self.pending_symbol(initializer) != Some(item.root)
            || self.bindings.get(&item.root) != Some(&value)
        {
            return Err(fail());
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::MissingCounterExitBindingPermit,
        ) {
            return Ok(None);
        }
        self.counter_exit_permit(index, value, span).map(Some)
    }
    pub(super) fn counter_exit_binding_emitted(
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
                owner_consumer_test_support::OwnerFault::CounterExitWrongSource,
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
        let item = &mut self
            .counter_tail_plan
            .as_mut()
            .ok_or_else(fail)?
            .exits
            .as_mut()
            .ok_or_else(fail)?[permit.index];
        if !item.suffix || item.stage != Stage::Ready {
            return Err(fail());
        }
        item.stage = Stage::Bound(binding);
        #[cfg(test)]
        owner_consumer_test_support::record_action(
            "counter-exit-binding-permit-consumed",
            permit.index,
        );
        Ok(())
    }
    pub(super) fn counter_exit_drop(
        &self,
        fact_index: usize,
        target: UnitDropTarget,
        slot: Slot,
        span: Span,
    ) -> Result<Option<Permit>, LoweringError> {
        let Some(plan) = &self.counter_tail_plan else {
            return Ok(None);
        };
        let Some(items) = &plan.exits else {
            return Ok(None);
        };
        let actual = self.inputs.owned.drops().get(fact_index);
        let Some(index) = items.iter().position(|i| {
            actual == Some(&i.fact) || (i.fact.target() == target && i.stage != Stage::Dropped)
        }) else {
            return Ok(None);
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let item = &items[index];
        if self.inputs.owned.drops().get(fact_index) != Some(&item.fact)
            || (if item.suffix {
                item.stage != Stage::Bound(slot)
            } else {
                item.stage != Stage::Ready || self.bindings.get(&item.root) != Some(&slot)
            })
        {
            return Err(fail());
        }
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::MissingCounterExitDropPermit,
        ) {
            return Ok(None);
        }
        self.counter_exit_permit(index, slot, span).map(Some)
    }
    pub(super) fn counter_exit_drop_emitted(
        &mut self,
        permit: Permit,
        span: Span,
    ) -> Result<(), LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let statement = self.counter_retirement_statement(&permit, span)?;
        if !statement.results.is_empty()
            || !matches!(statement.op, ROp::Drop { owner } if owner == permit.source)
        {
            return Err(fail());
        }
        let item = &mut self
            .counter_tail_plan
            .as_mut()
            .ok_or_else(fail)?
            .exits
            .as_mut()
            .ok_or_else(fail)?[permit.index];
        #[cfg(test)]
        if owner_consumer_test_support::take_fault(
            owner_consumer_test_support::OwnerFault::CounterExitDuplicateDrop,
        ) {
            item.stage = Stage::Dropped;
        }
        if (item.suffix && item.stage != Stage::Bound(permit.source))
            || (!item.suffix && item.stage != Stage::Ready)
        {
            return Err(fail());
        }
        item.stage = Stage::Dropped;
        #[cfg(test)]
        owner_consumer_test_support::record_action(
            "counter-exit-drop-permit-consumed",
            permit.index,
        );
        Ok(())
    }
}
