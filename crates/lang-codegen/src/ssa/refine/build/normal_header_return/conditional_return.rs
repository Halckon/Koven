//! Direct conditional Return and the normal tail own distinct completion receipts.
use super::*;
#[derive(Default)]
pub(super) struct State {
    split: Option<Split>,
    arms: [bool; 2],
    join: Option<BlockRef>,
    taken: bool,
    returned: Option<Receipt>,
    normal: Option<Edge>,
}
#[derive(Clone, Copy)]
pub(in crate::ssa::refine::build) struct Split {
    source: BlockRef,
    arms: [BlockRef; 2],
}
#[derive(Clone, Copy)]
pub(in crate::ssa::refine::build) struct Edge {
    source: BlockRef,
    target: BlockRef,
}
impl Builder<'_, '_> {
    pub(in crate::ssa::refine::build) fn cr_split(
        &self,
        control: UnitExpressionId,
        condition: UnitExpressionId,
        body: UnitStatementId,
        no_else: bool,
        arms: [BlockRef; 2],
        span: Span,
    ) -> Result<Option<Split>, LoweringError> {
        let Some(p) = &self.header_return_plan else {
            return Ok(None);
        };
        let Boundary::ConditionalReturn(v) = p.boundary else {
            return Ok(None);
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let source = self.block.ok_or_else(fail)?;
        let Some(([_, success, _], _, _)) = p.increment else {
            return Err(fail());
        };
        if control != v.split()
            || condition != v.split_condition()
            || body != v.return_body()
            || !no_else
            || v.increment().is_none()
            || source != success
            || arms[0] == arms[1]
            || p.conditional_return.split.is_some()
            || !self.consumed_drops.contains(&p.drop_index)
            || self.body.blocks[source.0].terminator.is_some()
        {
            return Err(fail());
        }
        Ok(Some(Split { source, arms }))
    }
    pub(in crate::ssa::refine::build) fn cr_split_emitted(
        &mut self,
        r: Option<Split>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(r) = r else { return Ok(()) };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        if self.block.is_some()
            || !matches!(self.body.blocks[r.source.0].terminator,
            Some((RTerm::Branch{when_true,when_false,..},_)) if [when_true,when_false]==r.arms)
        {
            return Err(fail());
        }
        let p = self.header_return_plan.as_mut().ok_or_else(fail)?;
        if p.conditional_return.split.replace(r).is_some() {
            return Err(fail());
        }
        Ok(())
    }
    pub(in crate::ssa::refine::build) fn cr_arm(
        &mut self,
        control: UnitExpressionId,
        role: usize,
        block: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &mut self.header_return_plan else {
            return Ok(());
        };
        let Boundary::ConditionalReturn(v) = p.boundary else {
            return Ok(());
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let split = p.conditional_return.split.ok_or_else(fail)?;
        if control != v.split()
            || role >= 2
            || split.arms[role] != block
            || self.block != Some(block)
            || p.conditional_return.arms[role]
        {
            return Err(fail());
        }
        p.conditional_return.arms[role] = true;
        Ok(())
    }
    pub(in crate::ssa::refine::build) fn cr_join(
        &mut self,
        control: UnitExpressionId,
        join: BlockRef,
        reached: bool,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &mut self.header_return_plan else {
            return Ok(());
        };
        let Boundary::ConditionalReturn(v) = p.boundary else {
            return Ok(());
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let split = p.conditional_return.split.ok_or_else(fail)?;
        if control != v.split()
            || !reached
            || self.block != Some(join)
            || p.conditional_return.arms != [true; 2]
            || p.conditional_return.returned.is_none()
            || !self.body.blocks[split.arms[1].0].statements.is_empty()
            || !matches!(self.body.blocks[split.arms[1].0].terminator,Some((RTerm::Goto(b),_)) if b==join)
            || p.conditional_return.join.replace(join).is_some()
        {
            return Err(fail());
        }
        Ok(())
    }
    pub(in crate::ssa::refine::build) fn cr_backedge(
        &self,
        statement: UnitStatementId,
        header: BlockRef,
        span: Span,
    ) -> Result<Option<Edge>, LoweringError> {
        let Some(p) = &self.header_return_plan else {
            return Ok(None);
        };
        let Boundary::ConditionalReturn(v) = p.boundary else {
            return Ok(None);
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let [_, expected, _, _, _] = p.geometry.ok_or_else(fail)?;
        let source = self.block.ok_or_else(fail)?;
        if statement != v.statement()
            || header != expected
            || Some(source) != p.conditional_return.join
            || p.conditional_return.normal.is_some()
            || p.taken
            || p.completed
            || self.bindings.get(&v.root()).copied() != p.root
            || self.dead.contains(&v.root())
            || !self.consumed_drops.contains(&p.drop_index)
            || !self.body.blocks[source.0].statements.is_empty()
            || self.body.blocks[source.0].terminator.is_some()
        {
            return Err(fail());
        }
        Ok(Some(Edge {
            source,
            target: header,
        }))
    }
    pub(in crate::ssa::refine::build) fn cr_backedge_emitted(
        &mut self,
        r: Option<Edge>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(r) = r else { return Ok(()) };
        self.cr_edge_emitted(r, span)?;
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let p = self.header_return_plan.as_mut().ok_or_else(fail)?;
        if p.conditional_return.normal.replace(r).is_some() {
            return Err(fail());
        }
        Ok(())
    }
    fn cr_edge_emitted(&self, r: Edge, span: Span) -> Result<(), LoweringError> {
        if self.block.is_some()
            || !self.body.blocks[r.source.0].statements.is_empty()
            || !matches!(self.body.blocks[r.source.0].terminator,Some((RTerm::Goto(b),_)) if b==r.target)
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        Ok(())
    }
    pub(super) fn cr_validate(&self, success: BlockRef, span: Span) -> Result<(), LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let p = self.header_return_plan.as_ref().ok_or_else(fail)?;
        let [entry, header, body, finished, exit] = p.geometry.ok_or_else(fail)?;
        let state = &p.conditional_return;
        let split = state.split.ok_or_else(fail)?;
        let join = state.join.ok_or_else(fail)?;
        let returned = state.returned.ok_or_else(fail)?;
        let normal = state.normal.ok_or_else(fail)?;
        let incoming = |target| {
            self.body
                .blocks
                .iter()
                .enumerate()
                .filter_map(|(i, b)| {
                    b.terminator
                        .as_ref()
                        .filter(|(t, _)| t.successors().contains(&target))
                        .map(|_| BlockRef(i))
                })
                .collect::<Vec<_>>()
        };
        let exits = [finished];
        if state.arms != [true; 2]
            || split.source != success
            || returned.block != split.arms[0]
            || !state.taken
            || Some(returned.slot) != p.root
            || normal.source != join
            || normal.target != header
            || incoming(header) != [entry, join]
            || incoming(body) != [header]
            || incoming(finished) != [header]
            || incoming(exit) != exits
            || incoming(split.arms[0]) != [success]
            || incoming(split.arms[1]) != [success]
            || incoming(join) != [split.arms[1]]
            || !matches!(self.body.blocks[header.0].terminator,Some((RTerm::Branch{when_true,when_false,..},_)) if when_true==body && when_false==finished)
            || !matches!(self.body.blocks[success.0].terminator,Some((RTerm::Branch{when_true,when_false,..},_)) if [when_true,when_false]==split.arms)
        {
            return Err(fail());
        }
        for (source, target) in [
            (entry, header),
            (finished, exit),
            (split.arms[1], join),
            (join, header),
        ] {
            if !matches!(self.body.blocks[source.0].terminator,Some((RTerm::Goto(b),_)) if b==target)
            {
                return Err(fail());
            }
        }
        if !matches!(self.body.blocks[returned.block.0].terminator, Some((RTerm::Return(Some(slot)),_)) if slot==returned.slot)
        {
            return Err(fail());
        }
        for block in [split.arms[0], split.arms[1], join] {
            if !self.body.blocks[block.0].statements.is_empty() {
                return Err(fail());
            }
        }
        Ok(())
    }
}

impl Builder<'_, '_> {
    pub(super) fn cr_name(
        &mut self,
        expression: UnitExpressionId,
        span: Span,
    ) -> Result<Option<Slot>, LoweringError> {
        let Some(p) = &self.header_return_plan else {
            return Ok(None);
        };
        let Boundary::ConditionalReturn(v) = p.boundary else {
            return Ok(None);
        };
        if expression != v.body_return_payload() {
            return Ok(None);
        }
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let split = p.conditional_return.split.ok_or_else(fail)?;
        let slot = p.root.ok_or_else(fail)?;
        if self.block != Some(split.arms[0])
            || !p.conditional_return.arms[0]
            || p.conditional_return.taken
            || p.conditional_return.returned.is_some()
            || p.taken
            || p.completed
            || p.old.is_none()
            || self.reference(span)? != v.root()
            || self.bindings.get(&v.root()) != Some(&slot)
            || self.dead.contains(&v.root())
            || !self.consumed_drops.contains(&p.drop_index)
        {
            return Err(fail());
        }
        self.header_return_plan
            .as_mut()
            .ok_or_else(fail)?
            .conditional_return
            .taken = true;
        Ok(Some(slot))
    }
    pub(super) fn cr_return(
        &self,
        expression: UnitExpressionId,
        result: Option<Slot>,
        unit: bool,
        span: Span,
    ) -> Result<Option<Receipt>, LoweringError> {
        let Some(p) = &self.header_return_plan else {
            return Ok(None);
        };
        let Boundary::ConditionalReturn(v) = p.boundary else {
            return Ok(None);
        };
        if expression != v.body_return_expression() {
            return Ok(None);
        }
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let split = p.conditional_return.split.ok_or_else(fail)?;
        let slot = p.root.ok_or_else(fail)?;
        let block = split.arms[0];
        if unit
            || result != Some(slot)
            || self.block != Some(block)
            || !p.conditional_return.taken
            || p.conditional_return.returned.is_some()
            || p.taken
            || p.completed
            || !self.dead.contains(&v.root())
            || !self.consumed_drops.contains(&p.drop_index)
            || !self.body.blocks[block.0].statements.is_empty()
            || self.body.blocks[block.0].terminator.is_some()
        {
            return Err(fail());
        }
        Ok(Some(Receipt {
            block,
            slot,
            body_return: true,
        }))
    }
    pub(super) fn cr_return_emitted(
        &mut self,
        r: Receipt,
        span: Span,
    ) -> Result<(), LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        if !r.body_return
            || self.block.is_some()
            || !self.body.blocks[r.block.0].statements.is_empty()
            || !matches!(self.body.blocks[r.block.0].terminator,Some((RTerm::Return(Some(slot)),_)) if slot==r.slot)
        {
            return Err(fail());
        }
        let p = self.header_return_plan.as_mut().ok_or_else(fail)?;
        #[cfg(test)]
        {
            use super::super::owner_consumer_test_support::{OwnerFault, take_fault};
            if take_fault(OwnerFault::MissingConditionalReturn) {
                return Ok(());
            }
            if take_fault(OwnerFault::DuplicateConditionalReturn) {
                p.conditional_return.returned = Some(r);
            }
            if take_fault(OwnerFault::WrongConditionalReturnSlot) {
                p.conditional_return.returned = Some(Receipt {
                    slot: p.old.ok_or_else(fail)?,
                    ..r
                });
                return Ok(());
            }
        }
        if p.conditional_return.returned.replace(r).is_some() {
            return Err(fail());
        }
        Ok(())
    }
}
