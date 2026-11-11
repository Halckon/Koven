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
        if !p.valid_shape() {
            return Err(invalid(span));
        }
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
            || (i > 0
                && (!p.splits[0].done
                    || p.parent_arm(i)
                        .and_then(|arm| p.splits[0].arms.map(|a| a[arm]))
                        != Some(entry)
                    || p.source != Some(source)
                    || match p.topology {
                        UnitBorrowNestedForwardTopology::BalancedBoth => match i {
                            1 => p.paths.iter().any(|path| path.stage != 0),
                            2 => {
                                p.splits[1].join.is_none()
                                    || p.paths[..2].iter().any(|path| path.stage != 3)
                                    || p.paths[2..].iter().any(|path| path.stage != 0)
                            }
                            _ => true,
                        },
                        UnitBorrowNestedForwardTopology::NestedThen
                        | UnitBorrowNestedForwardTopology::NestedThenTail => {
                            p.paths.iter().any(|path| path.stage != 0)
                        }
                        UnitBorrowNestedForwardTopology::NestedElse => {
                            p.paths[0].stage != 3 || p.paths[1..].iter().any(|path| path.stage != 0)
                        }
                    }))
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
        if !p.valid_shape() {
            return Err(invalid(span));
        }
        let i = p
            .splits
            .iter()
            .position(|s| s.split == control)
            .ok_or_else(|| invalid(span))?;
        let predecessor_valid = if p.tail() && i == 0 {
            p.splits[0]
                .arms
                .is_some_and(|arms| predecessors == [arms[1]])
                && p.paths[..2].iter().all(|p| p.stage == 3)
                && p.paths[2].stage == 0
                && !p.goto_done
        } else {
            predecessors.is_empty()
        };
        if !predecessor_valid
            || !p.splits[i].done
            || p.splits[i].join.is_some()
            || p.splits.iter().any(|s| {
                s.entry == Some(join)
                    || s.arms.is_some_and(|a| a.contains(&join))
                    || s.join == Some(join)
            })
            || (i > 0
                && match p.topology {
                    UnitBorrowNestedForwardTopology::BalancedBoth => match i {
                        1 => {
                            p.paths[..2].iter().any(|path| path.stage != 3)
                                || p.paths[2..].iter().any(|path| path.stage != 0)
                                || p.splits[2].entry.is_some()
                        }
                        2 => {
                            p.splits[1].join.is_none() || p.paths.iter().any(|path| path.stage != 3)
                        }
                        _ => true,
                    },
                    UnitBorrowNestedForwardTopology::NestedThen
                    | UnitBorrowNestedForwardTopology::NestedThenTail => {
                        p.paths[0].stage != 3 || p.paths[1].stage != 3 || p.paths[2].stage != 0
                    }
                    UnitBorrowNestedForwardTopology::NestedElse => {
                        p.paths.iter().any(|path| path.stage != 3)
                    }
                })
            || (i == 0
                && (p.splits[1..].iter().any(|s| s.join.is_none())
                    || if p.tail() {
                        p.paths[..2].iter().any(|p| p.stage != 3) || p.paths[2].stage != 0
                    } else {
                        p.paths.iter().any(|p| p.stage != 3)
                    }))
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
        if !p.valid_shape() {
            return Err(invalid(span));
        }
        let i = p
            .paths
            .iter()
            .position(|path| path.control == control && path.value == value)
            .ok_or_else(|| invalid(span))?;
        let route = p.leaf_route(i);
        let (split, arm) = route.last().copied().ok_or_else(|| invalid(span))?;
        let expected = if p.tail() && i == 2 {
            p.splits[0].join
        } else {
            p.splits[split].arms.map(|a| a[arm])
        };
        let phase_valid = match (p.topology, i) {
            (UnitBorrowNestedForwardTopology::NestedThenTail, _) => {
                p.splits.iter().all(|s| s.done)
                    && if i == 2 {
                        p.splits[1].join.is_some() && p.splits[0].join.is_some() && p.goto_done
                    } else {
                        p.splits[0].join.is_none() && !p.goto_done
                    }
            }
            (UnitBorrowNestedForwardTopology::BalancedBoth, 0 | 1) => {
                p.splits[0].done
                    && p.splits[1].done
                    && p.splits[1].join.is_none()
                    && p.splits[2].entry.is_none()
            }
            (UnitBorrowNestedForwardTopology::BalancedBoth, 2 | 3) => {
                p.splits.iter().all(|s| s.done)
                    && p.splits[1].join.is_some()
                    && p.splits[2].join.is_none()
            }
            (UnitBorrowNestedForwardTopology::BalancedBoth, _) => false,
            (UnitBorrowNestedForwardTopology::NestedThen, _) => {
                p.splits.iter().all(|s| s.done) && (i != 2 || p.splits[1].join.is_some())
            }
            (UnitBorrowNestedForwardTopology::NestedElse, 0) => {
                p.splits[0].done && p.splits[1].entry.is_none() && p.splits[1].join.is_none()
            }
            (UnitBorrowNestedForwardTopology::NestedElse, _) => {
                p.splits.iter().all(|s| s.done) && p.splits[1].join.is_none()
            }
        };
        if expected != Some(block)
            || !phase_valid
            || p.paths[i].stage != 0
            || (i > 0 && p.paths[i - 1].stage != 3)
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
        if !p.valid_shape() {
            return Err(invalid(span));
        }
        if let Some(s) = p.splits.iter_mut().find(|s| s.entry == Some(block)) {
            if s.done
                || !matches!(term,RTerm::Branch{condition,when_true,when_false} if s.value==Some(*condition)&&s.arms==Some([*when_true,*when_false]))
            {
                return Err(invalid(span));
            }
            s.done = true;
            return Ok(());
        }
        if p.tail() && p.splits[0].arms.map(|a| a[1]) == Some(block) {
            if p.goto_done
                || p.paths[..2].iter().any(|p| p.stage != 3)
                || !matches!(term, RTerm::Goto(target) if p.splits[0].join == Some(*target))
            {
                return Err(invalid(span));
            }
            p.goto_done = true;
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
            !p.valid_shape()
                || (p.tail() != p.goto_done)
                || (!p.tail() && !self.folds.is_empty())
                || p.splits.iter().any(|s| !s.done || s.join.is_none())
                || p.paths.iter().any(|p| p.stage != 3)
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
        let Some(p) = &self.plan else {
            return Ok(());
        };
        if !p.valid_shape()
            || !p.tail()
            || !p.goto_done
            || !self.folds.is_empty()
            || p.splits[0].arms.map(|a| a[1]) != Some(block)
            || p.splits[0].join != Some(successor)
            || block == successor
        {
            return Err(invalid(span));
        }
        self.folds.push((block, successor));
        Ok(())
    }
}
