//! A local BindDrop consumes H without inventing a Return or a fixed P/Q join.
use super::*;
#[derive(Default)]
pub(super) struct State {
    bound: Option<Slot>,
    duplicate: bool,
}
#[derive(Clone, Copy)]
pub(in crate::ssa::refine) struct DropReceipt {
    selected: Slot,
    bound: Slot,
    #[cfg(test)]
    index: usize,
    block: BlockRef,
    instruction: usize,
}
impl DropReceipt {
    #[cfg(test)]
    pub(in crate::ssa::refine) fn witness(self) -> (usize, usize, usize) {
        (self.selected.0, self.bound.0, self.index)
    }
}
impl Builder<'_, '_> {
    pub(super) fn header_bind_name(
        &mut self,
        expression: UnitExpressionId,
        span: Span,
    ) -> Result<Option<Slot>, LoweringError> {
        let Some(p) = &self.header_return_plan else {
            return Ok(None);
        };
        let Boundary::BindDrop(v) = p.boundary else {
            return Ok(None);
        };
        if expression != v.initializer() {
            return Ok(None);
        }
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let [_, _, _, _, exit] = p.geometry.ok_or_else(fail)?;
        let slot = p.root.ok_or_else(fail)?;
        if self.block != Some(exit)
            || p.taken
            || p.completed
            || p.old.is_none()
            || self.reference(span)? != v.root()
            || self.bindings.get(&v.root()) != Some(&slot)
            || self.dead.contains(&v.root())
            || p.bind_drop.bound.is_some()
        {
            return Err(fail());
        }
        self.header_return_plan.as_mut().ok_or_else(fail)?.taken = true;
        Ok(Some(slot))
    }
    pub(in crate::ssa::refine::build) fn header_bind_binding(
        &self,
        initializer: UnitExpressionId,
        symbol: UnitSymbolId,
        value: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &self.header_return_plan else {
            return Ok(());
        };
        let Boundary::BindDrop(v) = p.boundary else {
            return Ok(());
        };
        if symbol != v.retired() {
            return Ok(());
        }
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let [_, _, _, _, exit] = p.geometry.ok_or_else(fail)?;
        if initializer != v.initializer()
            || self.block != Some(exit)
            || p.root != Some(value)
            || !p.taken
            || p.completed
            || p.bind_drop.bound.is_some()
        {
            return Err(fail());
        }
        Ok(())
    }
    pub(in crate::ssa::refine::build) fn header_bind_binding_emitted(
        &mut self,
        symbol: UnitSymbolId,
        binding: Slot,
    ) {
        if let Some(p) = &mut self.header_return_plan
            && let Boundary::BindDrop(v) = p.boundary
            && symbol == v.retired()
        {
            p.bind_drop.duplicate |= p.bind_drop.bound.replace(binding).is_some();
        }
    }
    pub(in crate::ssa::refine) fn header_bind_drop(
        &self,
        index: usize,
        target: UnitDropTarget,
        slot: Slot,
        span: Span,
    ) -> Result<Option<DropReceipt>, LoweringError> {
        let Some(p) = &self.header_return_plan else {
            return Ok(None);
        };
        let Boundary::BindDrop(v) = p.boundary else {
            return Ok(None);
        };
        if target != UnitDropTarget::Named(v.retired()) {
            return Ok(None);
        }
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let fact = self.inputs.owned.drops().get(index).ok_or_else(fail)?;
        let [_, _, _, _, exit] = p.geometry.ok_or_else(fail)?;
        let selected = p.root.ok_or_else(fail)?;
        if fact.target() != target
            || fact.point() != v.drop_point()
            || index == p.drop_index
            || p.bind_drop.bound != Some(slot)
            || slot == selected
            || !p.taken
            || p.completed
            || p.bind_drop.duplicate
            || self.block != Some(exit)
            || self.bindings.get(&v.retired()) != Some(&slot)
            || !self.dead.contains(&v.root())
            || !self.consumed_drops.contains(&p.drop_index)
            || self.consumed_drops.contains(&index)
            || self.body.blocks[exit.0].terminator.is_some()
        {
            return Err(fail());
        }
        let defs = self
            .body
            .blocks
            .iter()
            .enumerate()
            .flat_map(|(b, block)| {
                block
                    .statements
                    .iter()
                    .enumerate()
                    .filter_map(move |(i, s)| s.results.contains(&slot).then_some((b, i, s)))
            })
            .collect::<Vec<_>>();
        let [(block, _, definition)] = defs.as_slice() else {
            return Err(fail());
        };
        if *block != exit.0
            || definition.results != [slot]
            || !matches!(definition.op,ROp::Alias{source} if source==selected)
            || self.body.blocks[exit.0]
                .statements
                .iter()
                .any(|s| matches!(s.op,ROp::Drop{owner} if owner==slot))
        {
            return Err(fail());
        }
        self.validate_header_transport(span)?;
        Ok(Some(DropReceipt {
            selected,
            bound: slot,
            #[cfg(test)]
            index,
            block: exit,
            instruction: self.body.blocks[exit.0].statements.len(),
        }))
    }
    pub(in crate::ssa::refine) fn header_bind_drop_emitted(
        &mut self,
        receipt: Option<DropReceipt>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(r) = receipt else { return Ok(()) };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let statements = &self.body.blocks[r.block.0].statements;
        if self.block != Some(r.block)
            || statements.len() != r.instruction + 1
            || !matches!(statements[r.instruction].op,ROp::Drop{owner} if owner==r.bound)
            || !statements[r.instruction].results.is_empty()
        {
            return Err(fail());
        }
        let p = self.header_return_plan.as_mut().ok_or_else(fail)?;
        #[cfg(test)]
        {
            use super::super::owner_consumer_test_support::{OwnerFault, take_fault};
            if take_fault(OwnerFault::MissingHeaderBindDrop) {
                return Ok(());
            }
            if take_fault(OwnerFault::DuplicateHeaderBindDrop) {
                p.completed = true;
            }
        }
        if p.root != Some(r.selected)
            || p.bind_drop.bound != Some(r.bound)
            || std::mem::replace(&mut p.completed, true)
        {
            return Err(fail());
        }
        Ok(())
    }
}
