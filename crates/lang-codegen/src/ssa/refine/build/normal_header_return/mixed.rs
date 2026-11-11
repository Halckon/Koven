//! Mixed edges have distinct live receipts, not roles inferred from a final Goto.
use super::*;

#[derive(Default)]
pub(super) struct State {
    split: Option<Split>,
    entered: [bool; 2],
    jumps: [Option<Jump>; 2],
}
#[derive(Clone, Copy)]
pub(in crate::ssa::refine::build) struct Split {
    source: BlockRef,
    arms: [BlockRef; 2],
}
#[derive(Clone, Copy)]
pub(in crate::ssa::refine::build) struct Jump {
    role: usize,
    expression: UnitExpressionId,
    source: BlockRef,
    target: BlockRef,
    statements: usize,
}
impl Builder<'_, '_> {
    pub(in crate::ssa::refine::build) fn mixed_header_split(
        &self,
        control: UnitExpressionId,
        condition: UnitExpressionId,
        first: UnitStatementId,
        second: Option<UnitStatementId>,
        arms: [BlockRef; 2],
        span: Span,
    ) -> Result<Option<Split>, LoweringError> {
        let Some(p) = &self.header_return_plan else {
            return Ok(None);
        };
        let Boundary::Mixed(v) = p.boundary else {
            return Ok(None);
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let source = self.block.ok_or_else(fail)?;
        let (_, Some(([_, success, _], _, _))) = (v.increment(), p.increment) else {
            return Err(fail());
        };
        if control != v.split()
            || condition != v.split_condition()
            || first != v.continue_body()
            || second != Some(v.break_body())
            || source != success
            || arms[0] == arms[1]
            || p.mixed.split.is_some()
            || p.mixed.entered != [false; 2]
            || p.mixed.jumps.iter().any(Option::is_some)
            || self.body.blocks[source.0].terminator.is_some()
            || !self.consumed_drops.contains(&p.drop_index)
        {
            return Err(fail());
        }
        Ok(Some(Split { source, arms }))
    }
    pub(in crate::ssa::refine::build) fn mixed_header_split_emitted(
        &mut self,
        receipt: Option<Split>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(r) = receipt else { return Ok(()) };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        if self.block.is_some()
            || !matches!(self.body.blocks[r.source.0].terminator,
            Some((RTerm::Branch {when_true, when_false, ..}, _))
                if when_true == r.arms[0] && when_false == r.arms[1])
        {
            return Err(fail());
        }
        let p = self.header_return_plan.as_mut().ok_or_else(fail)?;
        if p.mixed.split.replace(r).is_some() {
            return Err(fail());
        }
        Ok(())
    }
    pub(in crate::ssa::refine::build) fn mixed_header_arm(
        &mut self,
        control: UnitExpressionId,
        role: usize,
        block: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &mut self.header_return_plan else {
            return Ok(());
        };
        let Boundary::Mixed(v) = p.boundary else {
            return Ok(());
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let split = p.mixed.split.ok_or_else(fail)?;
        if control != v.split()
            || role >= 2
            || split.arms[role] != block
            || self.block != Some(block)
            || p.mixed.entered[role]
        {
            return Err(fail());
        }
        p.mixed.entered[role] = true;
        Ok(())
    }
    pub(in crate::ssa::refine::build) fn mixed_header_jump(
        &self,
        expression: UnitExpressionId,
        is_break: bool,
        frame: (UnitStatementId, BlockRef, bool),
        span: Span,
    ) -> Result<Option<Jump>, LoweringError> {
        let Some(p) = &self.header_return_plan else {
            return Ok(None);
        };
        let Boundary::Mixed(v) = p.boundary else {
            return Ok(None);
        };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let split = p.mixed.split.ok_or_else(fail)?;
        let [_, header, _, _, exit] = p.geometry.ok_or_else(fail)?;
        let source = self.block.ok_or_else(fail)?;
        let role = usize::from(is_break);
        let target = if is_break { exit } else { header };
        let expected = if is_break {
            v.break_jump()
        } else {
            v.continue_jump()
        };
        if expression != expected
            || frame != (v.statement(), target, true)
            || source != split.arms[role]
            || !p.mixed.entered[role]
            || p.mixed.jumps[role].is_some()
            || p.taken
            || p.completed
            || p.old.is_none()
            || p.root.is_none()
            || self.bindings.get(&v.root()).copied() != p.root
            || self.dead.contains(&v.root())
            || !self.consumed_drops.contains(&p.drop_index)
            || self.body.blocks[source.0].terminator.is_some()
        {
            return Err(fail());
        }
        Ok(Some(Jump {
            role,
            expression,
            source,
            target,
            statements: self.body.blocks[source.0].statements.len(),
        }))
    }
    pub(in crate::ssa::refine::build) fn mixed_header_jump_emitted(
        &mut self,
        receipt: Option<Jump>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(r) = receipt else { return Ok(()) };
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        if self.block.is_some()
            || self.body.blocks[r.source.0].statements.len() != r.statements
            || !matches!(self.body.blocks[r.source.0].terminator,
                Some((RTerm::Goto(target), _)) if target == r.target)
        {
            return Err(fail());
        }
        let p = self.header_return_plan.as_mut().ok_or_else(fail)?;
        #[cfg(test)]
        {
            use super::super::owner_consumer_test_support::{OwnerFault, take_fault};
            if take_fault(OwnerFault::MissingMixedJump) {
                return Ok(());
            }
            if take_fault(OwnerFault::DuplicateMixedJump) {
                p.mixed.jumps[r.role] = Some(r);
            }
            if take_fault(OwnerFault::WrongMixedJumpTarget) {
                p.mixed.jumps[r.role] = Some(Jump {
                    target: r.source,
                    ..r
                });
                return Ok(());
            }
            if take_fault(OwnerFault::WrongMixedJumpIdentity) {
                p.mixed.jumps[r.role] = Some(Jump {
                    expression: p.boundary.return_payload().ok_or_else(fail)?,
                    ..r
                });
                return Ok(());
            }
        }
        if p.mixed.jumps[r.role].replace(r).is_some() {
            return Err(fail());
        }
        Ok(())
    }
    pub(super) fn mixed_header_validate(
        &self,
        success: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        let fail = || error(LoweringErrorKind::InvalidModel, span);
        let p = self.header_return_plan.as_ref().ok_or_else(fail)?;
        let Boundary::Mixed(v) = p.boundary else {
            return Err(fail());
        };
        let [entry, header, body, finished, exit] = p.geometry.ok_or_else(fail)?;
        let split = p.mixed.split.ok_or_else(fail)?;
        let [Some(continued), Some(broken)] = p.mixed.jumps else {
            return Err(fail());
        };
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
        let mut exit_sources = vec![finished, broken.source];
        exit_sources.sort_by_key(|b| b.0);
        if split.source != success
            || p.mixed.entered != [true; 2]
            || continued.expression != v.continue_jump()
            || continued.role != 0
            || broken.expression != v.break_jump()
            || broken.role != 1
            || continued.source != split.arms[0]
            || broken.source != split.arms[1]
            || continued.target != header
            || broken.target != exit
            || incoming(header) != [entry, continued.source]
            || incoming(body) != [header]
            || incoming(finished) != [header]
            || incoming(exit) != exit_sources
            || incoming(split.arms[0]) != [success]
            || incoming(split.arms[1]) != [success]
            || !matches!(self.body.blocks[entry.0].terminator,Some((RTerm::Goto(b),_)) if b==header)
            || !matches!(self.body.blocks[finished.0].terminator,Some((RTerm::Goto(b),_)) if b==exit)
            || !matches!(self.body.blocks[header.0].terminator,Some((RTerm::Branch {when_true,when_false,..},_)) if when_true==body && when_false==finished)
            || !matches!(self.body.blocks[success.0].terminator,Some((RTerm::Branch {when_true,when_false,..},_)) if when_true==continued.source && when_false==broken.source)
        {
            return Err(fail());
        }
        for r in [continued, broken] {
            // The accepted arms contain only the sealed jump; no hidden transfer,
            // ownership mutation, or post-receipt instruction is permitted.
            if r.statements != 0
                || !self.body.blocks[r.source.0].statements.is_empty()
                || !matches!(self.body.blocks[r.source.0].terminator,Some((RTerm::Goto(b),_)) if b==r.target)
            {
                return Err(fail());
            }
        }
        Ok(())
    }
}
