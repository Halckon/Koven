//! Observe real refined operations and both actual terminal joins.
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
        let i = p
            .splits
            .iter()
            .position(|s| s.split == control && s.condition == condition)
            .ok_or_else(|| invalid(span))?;
        let source = *borrowed.get(&p.root).ok_or_else(|| invalid(span))?;
        let flag = *borrowed.get(&p.flags[i]).ok_or_else(|| invalid(span))?;
        if arms[0] == arms[1]
            || arms.contains(&entry)
            || source == flag
            || p.splits[i].entry.is_some()
            || (i == 0 && (entry != BlockRef(0) || p.source.is_some()))
            || (i == 1
                && (!p.splits[0].done
                    || p.splits[0].arms.map(|a| a[0]) != Some(entry)
                    || p.source != Some(source)))
        {
            return Err(invalid(span));
        }
        p.source = Some(source);
        let s = &mut p.splits[i];
        s.entry = Some(entry);
        s.arms = Some(arms);
        s.flag = Some(flag);
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
        let i = p
            .splits
            .iter()
            .position(|s| s.split == control)
            .ok_or_else(|| invalid(span))?;
        if !predecessors.is_empty()
            || !p.splits[i].done
            || p.splits[i].join.is_some()
            || p.splits.iter().any(|s| {
                s.entry == Some(join)
                    || s.arms.is_some_and(|a| a.contains(&join))
                    || s.join == Some(join)
            })
            || (i == 1 && (p.paths[0].stage != 3 || p.paths[1].stage != 3 || p.paths[2].stage != 0))
            || (i == 0 && (p.splits[1].join.is_none() || p.paths.iter().any(|p| p.stage != 3)))
        {
            return Err(invalid(span));
        }
        p.splits[i].join = Some(join);
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
        let i = p
            .paths
            .iter()
            .position(|path| path.control == control && path.value == value)
            .ok_or_else(|| invalid(span))?;
        let expected = if i < 2 {
            p.splits[1].arms.map(|a| a[i])
        } else {
            p.splits[0].arms.map(|a| a[1])
        };
        if expected != Some(block)
            || !p.splits.iter().all(|s| s.done)
            || p.paths[i].stage != 0
            || (i > 0 && p.paths[i - 1].stage != 3)
            || (i == 2 && p.splits[1].join.is_none())
        {
            return Err(invalid(span));
        }
        let source = *borrowed.get(&p.root).ok_or_else(|| invalid(span))?;
        if p.source != Some(source) {
            return Err(invalid(span));
        }
        let path = &mut p.paths[i];
        path.source = Some(source);
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
        if self.plan.as_ref().is_some_and(|p| {
            !p.paths
                .iter()
                .any(|path| path.stage == 1 && path.value == call && path.source_loan == source)
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
        if let Some((i, s)) = p
            .splits
            .iter_mut()
            .enumerate()
            .find(|(_, s)| s.entry == Some(block))
        {
            if s.value.is_some()
                || s.done
                || !matches!(op,ROp::ReadLoan{loan} if Some(*loan)==s.flag)
            {
                return Err(invalid(span));
            }
            s.value = Some(*result);
            return Ok(Some(Tag::Condition(i)));
        }
        let (i, path) = p
            .paths
            .iter_mut()
            .enumerate()
            .find(|(_, p)| p.block == Some(block) && p.stage == 1)
            .ok_or_else(|| invalid(span))?;
        if !matches!(op,ROp::BorrowCall{callee,arguments,source} if *callee==path.callee&&Some(*source)==path.source&&arguments==&[*source]&&source!=result)
        {
            return Err(invalid(span));
        }
        path.result = Some(*result);
        path.stage = 2;
        Ok(Some(Tag::Call(i)))
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
        if let Some(s) = p.splits.iter_mut().find(|s| s.entry == Some(block)) {
            if s.done
                || !matches!(term,RTerm::Branch{condition,when_true,when_false} if s.value==Some(*condition)&&s.arms==Some([*when_true,*when_false]))
            {
                return Err(invalid(span));
            }
            s.done = true;
            return Ok(());
        }
        let path = p
            .paths
            .iter_mut()
            .find(|p| p.block == Some(block))
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
            p.splits.len() != 2
                || p.paths.len() != 3
                || p.splits.iter().any(|s| !s.done || s.join.is_none())
                || p.paths.iter().any(|p| p.stage != 3)
        }) {
            return Err(invalid(span));
        }
        Ok(())
    }
    pub(in crate::ssa::refine) fn fold(
        &self,
        _block: BlockRef,
        _successor: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.plan.is_some() {
            Err(invalid(span))
        } else {
            Ok(())
        }
    }
}
