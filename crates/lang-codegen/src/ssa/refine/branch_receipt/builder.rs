//! Capture original refined producer transitions, not a post-hoc CFG search.
use super::*;
impl Plans {
    pub(in crate::ssa::refine) fn split(
        &mut self,
        control: UnitExpressionId,
        condition: UnitExpressionId,
        entry: BlockRef,
        arms: [BlockRef; 2],
        borrowed: &BTreeMap<UnitSymbolId, Slot>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = self.plan.as_mut() else {
            return Ok(());
        };
        if p.entry.is_some()
            || control != p.split
            || condition != p.condition
            || entry != BlockRef(0)
            || arms[0] == arms[1]
        {
            return Err(invalid(span));
        }
        p.entry = Some(entry);
        p.arms = Some(arms);
        p.source = Some(*borrowed.get(&p.root).ok_or_else(|| invalid(span))?);
        p.flag_slot = Some(*borrowed.get(&p.flag).ok_or_else(|| invalid(span))?);
        if p.source == p.flag_slot {
            return Err(invalid(span));
        }
        Ok(())
    }
    pub(in crate::ssa::refine) fn join(
        &mut self,
        control: UnitExpressionId,
        join: BlockRef,
        predecessors: &[BlockRef],
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = self.plan.as_mut() else {
            return Ok(());
        };
        let arms = p.arms.ok_or_else(|| invalid(span))?;
        if control != p.split
            || p.join.is_some()
            || predecessors != [arms[1]]
            || p.paths[0].stage != 3
            || arms.contains(&join)
            || p.entry == Some(join)
        {
            return Err(invalid(span));
        }
        p.join = Some(join);
        Ok(())
    }
    pub(in crate::ssa::refine) fn enter(
        &mut self,
        control: UnitExpressionId,
        value: UnitExpressionId,
        block: BlockRef,
        borrowed: &BTreeMap<UnitSymbolId, Slot>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = self.plan.as_mut() else {
            return Ok(());
        };
        let index = p
            .paths
            .iter()
            .position(|path| path.branch.control() == control && path.branch.value() == value)
            .ok_or_else(|| invalid(span))?;
        let expected = if index == 0 {
            p.arms.map(|a| a[0])
        } else {
            p.join
        };
        let path = &mut p.paths[index];
        if path.stage != 0
            || Some(block) != expected
            || !p.split_done
            || (index == 1 && !p.goto_done)
        {
            return Err(invalid(span));
        }
        path.source = Some(*borrowed.get(&p.root).ok_or_else(|| invalid(span))?);
        if path.source != p.source {
            return Err(invalid(span));
        }
        path.block = Some(block);
        path.stage = 1;
        Ok(())
    }
    pub(in crate::ssa::refine) fn source_call(
        &self,
        call: UnitExpressionId,
        source: BorrowSourceLoan<UnitExpressionId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &self.plan else { return Ok(()) };
        if !p.paths.iter().any(|path| {
            path.stage == 1 && path.branch.value() == call && path.source_loan == source
        }) {
            return Err(invalid(span));
        }
        Ok(())
    }
    pub(in crate::ssa::refine) fn statement(
        &mut self,
        op: &ROp,
        results: &[Slot],
        block: BlockRef,
        span: Span,
    ) -> Result<Option<Tag>, LoweringError> {
        let Some(p) = self.plan.as_mut() else {
            return Ok(None);
        };
        let [result] = results else {
            return Err(invalid(span));
        };
        if Some(block) == p.entry {
            if p.condition_slot.is_some()
                || p.split_done
                || !matches!(op,ROp::ReadLoan{loan} if Some(*loan)==p.flag_slot)
            {
                return Err(invalid(span));
            }
            p.condition_slot = Some(*result);
            return Ok(Some(Tag::Condition));
        }
        let (index, path) = p
            .paths
            .iter_mut()
            .enumerate()
            .find(|(_, path)| path.block == Some(block) && path.stage == 1)
            .ok_or_else(|| invalid(span))?;
        if !matches!(op,ROp::BorrowCall{callee,arguments,source} if *callee==path.callee && Some(*source)==path.source && arguments==&[*source] && source!=result)
        {
            return Err(invalid(span));
        }
        path.result = Some(*result);
        path.stage = 2;
        Ok(Some(Tag::Call(index)))
    }
    pub(in crate::ssa::refine) fn terminate(
        &mut self,
        term: &RTerm,
        block: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = self.plan.as_mut() else {
            return Ok(());
        };
        if Some(block) == p.entry {
            if p.split_done
                || !matches!(term,RTerm::Branch{condition,when_true,when_false} if p.condition_slot==Some(*condition) && p.arms==Some([*when_true,*when_false]))
            {
                return Err(invalid(span));
            }
            p.split_done = true;
            return Ok(());
        }
        if p.arms.is_some_and(|a| a[1] == block) {
            if p.goto_done || !matches!(term,RTerm::Goto(target) if p.join==Some(*target)) {
                return Err(invalid(span));
            }
            p.goto_done = true;
            return Ok(());
        }
        let path = p
            .paths
            .iter_mut()
            .find(|path| path.block == Some(block))
            .ok_or_else(|| invalid(span))?;
        if path.stage != 2 || !matches!(term,RTerm::BorrowReturn{loan} if path.result==Some(*loan))
        {
            return Err(invalid(span));
        }
        path.stage = 3;
        Ok(())
    }
    pub(in crate::ssa::refine) fn finish(&self, span: Span) -> Result<(), LoweringError> {
        if self.plan.as_ref().is_some_and(|p| {
            !p.split_done
                || !p.goto_done
                || p.paths.len() != 2
                || p.paths.iter().any(|path| path.stage != 3)
        }) {
            return Err(invalid(span));
        }
        Ok(())
    }
    pub(in crate::ssa::refine) fn fold(
        &mut self,
        block: BlockRef,
        successor: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &self.plan else { return Ok(()) };
        if !self.folds.is_empty()
            || p.arms.map(|a| a[1]) != Some(block)
            || p.join != Some(successor)
        {
            return Err(invalid(span));
        }
        self.folds.push((block, successor));
        Ok(())
    }
}
