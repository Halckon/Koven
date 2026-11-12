//! Real producer events consume an independently validated DFS sequence.
use super::tree::Event;
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
        let Some(Event::Split(i)) = p.next() else {
            return Err(invalid(span));
        };
        let s = &p.splits[i];
        let source = *borrowed.get(&p.root).ok_or_else(|| invalid(span))?;
        let flag = *borrowed.get(&p.flags[i]).ok_or_else(|| invalid(span))?;
        let entry_ok = match s.parent {
            None => i == 0 && entry == BlockRef(0) && p.source.is_none(),
            Some((parent, arm)) => {
                p.splits[parent].done
                    && p.splits[parent].arms.map(|a| a[arm]) == Some(entry)
                    && p.source == Some(source)
            }
        };
        if !entry_ok
            || s.split != control
            || s.condition != condition
            || s.entry.is_some()
            || source == flag
            || arms[0] == arms[1]
            || arms.contains(&entry)
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
        pred: &[BlockRef],
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = self.plan.as_mut() else {
            return Ok(());
        };
        let Some(Event::Join(i)) = p.next() else {
            return Err(invalid(span));
        };
        if p.splits[i].split != control
            || !p.splits[i].done
            || p.splits[i].join.is_some()
            || !pred.is_empty()
            || p.splits.iter().any(|s| {
                s.entry == Some(join)
                    || s.arms.is_some_and(|a| a.contains(&join))
                    || s.join == Some(join)
            })
        {
            return Err(invalid(span));
        }
        p.splits[i].join = Some(join);
        p.cursor += 1;
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
        let Some(Event::Leaf(i)) = p.next() else {
            return Err(invalid(span));
        };
        let path = &p.paths[i];
        let (split, arm) = path.route.last().copied().ok_or_else(|| invalid(span))?;
        let source = *borrowed.get(&p.root).ok_or_else(|| invalid(span))?;
        if path.control != control
            || path.value != value
            || path.stage != 0
            || !p.splits[split].done
            || p.splits[split].arms.map(|a| a[arm]) != Some(block)
            || p.source != Some(source)
        {
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
        if !p.valid_shape() {
            return Err(invalid(span));
        }
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
        match p.next() {
            Some(Event::Split(i)) => {
                let s = &mut p.splits[i];
                if s.entry != Some(block)
                    || s.done
                    || !matches!(term,RTerm::Branch{condition,when_true,when_false} if s.value==Some(*condition)&&s.arms==Some([*when_true,*when_false]))
                {
                    return Err(invalid(span));
                }
                s.done = true;
            }
            Some(Event::Leaf(i)) => {
                let path = &mut p.paths[i];
                if path.block != Some(block)
                    || path.stage != 2
                    || !matches!(term,RTerm::BorrowReturn{loan} if path.result==Some(*loan))
                {
                    return Err(invalid(span));
                }
                path.stage = 3;
            }
            _ => return Err(invalid(span)),
        }
        p.cursor += 1;
        Ok(())
    }
    pub(in crate::ssa::refine) fn finish(&self, span: Span) -> Result<(), LoweringError> {
        if let Some(p) = &self.plan {
            let events = p.events().ok_or_else(|| invalid(span))?;
            if p.cursor != events.len()
                || p.source.is_none()
                || p.splits.iter().any(|s| !s.done || s.join.is_none())
                || p.paths.iter().any(|p| p.stage != 3)
            {
                return Err(invalid(span));
            }
        }
        Ok(())
    }
    pub(in crate::ssa::refine) fn fold(
        &mut self,
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
