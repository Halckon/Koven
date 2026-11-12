//! Record only actual append, boundary, terminator and fold events.
use super::*;
impl Plans {
    pub(in crate::ssa::refine) fn begin(
        &mut self,
        blocks: LoopBlocks,
        source: Slot,
        flag: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &mut self.plan else {
            return Ok(());
        };
        if p.boundary != 0
            || !p.events.is_empty()
            || source == flag
            || [
                blocks.entry,
                blocks.header,
                blocks.body,
                blocks.finished,
                blocks.exit,
            ]
            .into_iter()
            .collect::<BTreeSet<_>>()
            .len()
                != 5
        {
            return Err(invalid(span));
        }
        p.blocks = Some(blocks);
        p.source = Some(source);
        p.flag = Some(flag);
        p.boundary = 1;
        Ok(())
    }
    pub(in crate::ssa::refine) fn statement(
        &mut self,
        op: &ROp,
        results: &[Slot],
        block: BlockRef,
        span: Span,
    ) -> Result<Option<Tag>, LoweringError> {
        let Some(p) = &mut self.plan else {
            return Ok(None);
        };
        let b = p.blocks.ok_or_else(|| invalid(span))?;
        if p.callees.is_empty()
            || p.callees.len() != p.fact.consumers().len()
            || p.consumer_spans.len() != p.callees.len()
        {
            return Err(invalid(span));
        }
        let index = p.events.len();
        let n = p.callees.len();
        let tag = if index == 0 {
            if block != b.header
                || p.boundary != 1
                || span != p.condition_span
                || results.len() != 1
                || !matches!(op,ROp::ReadLoan{loan} if Some(*loan)==p.flag)
            {
                return Err(invalid(span));
            }
            p.condition_value = Some(results[0]);
            Tag::Condition
        } else if index == 1 {
            if block != b.body
                || p.boundary != 2
                || span != p.source_span
                || results.len() != 1
                || p.source == Some(results[0])
                || p.flag == Some(results[0])
                || !matches!(op,ROp::BorrowCall{callee,arguments,source} if *callee==p.source_callee && Some(*source)==p.source && arguments.as_slice()==[*source])
            {
                return Err(invalid(span));
            }
            p.result = Some(results[0]);
            Tag::Result
        } else if index < n + 2 {
            let i = index - 2;
            if block != b.body
                || p.boundary != 2
                || span != p.consumer_spans[i]
                || !results.is_empty()
                || !matches!(op,ROp::DirectCall{callee,arguments,receiver:None} if *callee==p.callees[i] && arguments.len()==1 && Some(arguments[0])==p.result)
            {
                return Err(invalid(span));
            }
            Tag::Consumer(i)
        } else if index == n + 2 {
            if block != b.body
                || p.boundary != 2
                || span != p.end_span
                || !results.is_empty()
                || !matches!(op,ROp::BorrowEnd{loan} if Some(*loan)==p.result)
            {
                return Err(invalid(span));
            }
            Tag::ResultEnd
        } else {
            return Err(invalid(span));
        };
        p.events.push(tag);
        Ok(Some(tag))
    }
    pub(in crate::ssa::refine) fn boundary(
        &mut self,
        phase: usize,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &mut self.plan else {
            return Ok(());
        };
        if p.boundary.checked_add(1) != Some(phase)
            || !(2..=5).contains(&phase)
            || (phase == 2 && p.events != [Tag::Condition])
            || (phase >= 3 && p.events.len() != p.callees.len() + 3)
        {
            return Err(invalid(span));
        }
        p.boundary = phase;
        Ok(())
    }
    pub(in crate::ssa::refine) fn terminate(
        &mut self,
        term: &RTerm,
        block: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &mut self.plan else {
            return Ok(());
        };
        let b = p.blocks.ok_or_else(|| invalid(span))?;
        let valid = match p.terms.len() {
            0 => {
                block == b.entry && p.boundary == 1 && matches!(term,RTerm::Goto(t) if *t==b.header)
            }
            1 => {
                block == b.header
                    && p.boundary == 1
                    && matches!(term,RTerm::Branch{condition,when_true,when_false} if Some(*condition)==p.condition_value && *when_true==b.body && *when_false==b.finished)
            }
            2 => {
                block == b.body && p.boundary == 3 && matches!(term,RTerm::Goto(t) if *t==b.header)
            }
            3 => {
                block == b.finished
                    && p.boundary == 4
                    && matches!(term,RTerm::Goto(t) if *t==b.exit)
            }
            4 => block == b.exit && p.boundary == 5 && matches!(term, RTerm::Return(None)),
            _ => false,
        };
        if !valid {
            return Err(invalid(span));
        }
        p.terms.push((block, term.clone()));
        Ok(())
    }
    pub(in crate::ssa::refine) fn fold(
        &mut self,
        from: BlockRef,
        to: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &mut self.plan else {
            return Ok(());
        };
        let b = p.blocks.ok_or_else(|| invalid(span))?;
        if !p.folds.is_empty() || from != b.finished || to != b.exit || p.terms.len() != 5 {
            return Err(invalid(span));
        }
        p.folds.push((from, to));
        Ok(())
    }
    pub(in crate::ssa::refine) fn canonical(&self, mut block: BlockRef) -> BlockRef {
        if let Some(p) = &self.plan {
            for (from, to) in &p.folds {
                if block == *to {
                    block = *from;
                }
            }
        }
        block
    }
    pub(in crate::ssa::refine) fn finish(&self, span: Span) -> Result<(), LoweringError> {
        let Some(p) = &self.plan else {
            return Ok(());
        };
        let n = p.callees.len();
        let expected = std::iter::once(Tag::Condition)
            .chain(std::iter::once(Tag::Result))
            .chain((0..n).map(Tag::Consumer))
            .chain(std::iter::once(Tag::ResultEnd))
            .collect::<Vec<_>>();
        if n == 0
            || n != p.fact.consumers().len()
            || n != p.consumer_spans.len()
            || p.boundary != 5
            || p.events != expected
            || p.terms.len() != 5
            || p.blocks.is_none()
            || [p.source, p.flag, p.condition_value, p.result]
                .iter()
                .any(Option::is_none)
            || [p.source, p.flag, p.condition_value, p.result]
                .into_iter()
                .collect::<BTreeSet<_>>()
                .len()
                != 4
        {
            return Err(invalid(span));
        }
        Ok(())
    }
}
