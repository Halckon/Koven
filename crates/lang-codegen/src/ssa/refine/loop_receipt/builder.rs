//! Capture actual append and CFG construction before optimization; never infer a producer from final SSA.
use super::*;
impl Plans {
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
        if p.callees.is_empty() || p.callees.len() != p.lineage.uses().len() {
            return Err(invalid(span));
        }
        // The sole local declaration aliases its evaluated literal into its binding slot.
        if let ROp::Alias { source } = op {
            let [target] = results else {
                return Err(invalid(span));
            };
            if p.events != [Tag::Owner]
                || p.boundary != 0
                || p.owner_alias.is_some()
                || Some(*source) != p.literal
                || source == target
                || block != BlockRef(0)
                || span != p.owner_binding_span
            {
                return Err(invalid(span));
            }
            p.owner = Some(*target);
            p.owner_alias = Some((*source, *target));
            return Ok(None);
        }
        let index = p.events.len();
        let n = p.callees.len();
        let tag = match index {
            0 => {
                if !matches!(op, ROp::StringLiteral { .. })
                    || results.len() != 1
                    || span != p.owner_span
                {
                    return Err(invalid(span));
                }
                p.literal = Some(results[0]);
                p.owner = Some(results[0]);
                Tag::Owner
            }
            1 => {
                if !matches!(op,ROp::RootPlace{owner} if Some(*owner)==p.owner)
                    || results.len() != 1
                {
                    return Err(invalid(span));
                }
                p.place = Some(results[0]);
                Tag::Place
            }
            2 => {
                if !matches!(op,ROp::BorrowBegin{place,kind:crate::ssa::model::LoanKind::Shared} if Some(*place)==p.place)
                    || results.len() != 1
                {
                    return Err(invalid(span));
                }
                p.source = Some(results[0]);
                Tag::Source
            }
            3 => {
                if !matches!(op,ROp::BorrowCall{callee,arguments,source} if *callee==p.source_callee && Some(*source)==p.source && arguments.as_slice()==[*source])
                    || results.len() != 1
                    || p.source == Some(results[0])
                {
                    return Err(invalid(span));
                }
                p.result = Some(results[0]);
                Tag::Result
            }
            4 => {
                let ROp::ReadLoan { loan } = op else {
                    return Err(invalid(span));
                };
                if results.len() != 1
                    || p.boundary != 1
                    || p.flag != Some(*loan)
                    || span != p.condition_span
                {
                    return Err(invalid(span));
                }
                p.flag = Some(*loan);
                p.condition_value = Some(results[0]);
                Tag::Condition
            }
            i if i < 5 + n => {
                let consumer = i - 5;
                if !matches!(op,ROp::DirectCall{callee,receiver:None,arguments} if *callee==p.callees[consumer] && arguments.len()==1 && Some(arguments[0])==p.result)
                    || !results.is_empty()
                    || span != p.lineage.uses()[consumer].loan().end_span()
                    || p.boundary != 2
                {
                    return Err(invalid(span));
                }
                Tag::Consumer(consumer)
            }
            i if i == 5 + n => {
                if !matches!(op,ROp::BorrowEnd{loan} if Some(*loan)==p.result)
                    || !results.is_empty()
                    || p.boundary != 5
                {
                    return Err(invalid(span));
                }
                Tag::ResultEnd
            }
            i if i == 6 + n => {
                if !matches!(op,ROp::BorrowEnd{loan} if Some(*loan)==p.source)
                    || !results.is_empty()
                {
                    return Err(invalid(span));
                }
                Tag::SourceEnd
            }
            i if i == 7 + n => {
                if !matches!(op,ROp::Drop{owner} if Some(*owner)==p.owner) || !results.is_empty() {
                    return Err(invalid(span));
                }
                Tag::OwnerDrop
            }
            _ => return Err(invalid(span)),
        };
        if let Some(b) = p.blocks {
            let expected = match tag {
                Tag::Owner | Tag::Place | Tag::Source | Tag::Result => b.entry,
                Tag::Condition => b.header,
                Tag::Consumer(_) => b.body,
                _ => b.exit,
            };
            if block != expected {
                return Err(invalid(span));
            }
        } else if index >= 4 || block != BlockRef(0) {
            return Err(invalid(span));
        }
        p.events.push(tag);
        Ok(Some(tag))
    }
    pub(in crate::ssa::refine) fn begin(
        &mut self,
        statement: lang_frontend::ast::StatementId,
        blocks: LoopBlocks,
        lease: EntryLease<'_>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &mut self.plan else {
            return Ok(());
        };
        if statement != p.lineage.statement().statement()
            || p.boundary != 0
            || p.events != [Tag::Owner, Tag::Place, Tag::Source, Tag::Result]
            || lease.owner != p.owner
            || lease.result != p.result
            || lease.sources.len() != 1
            || Some(lease.sources[0]) != p.source
            || [
                blocks.entry,
                blocks.header,
                blocks.body,
                blocks.finished,
                blocks.exit,
            ]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
                != 5
        {
            return Err(invalid(span));
        }
        p.flag = Some(lease.flag);
        p.blocks = Some(blocks);
        p.boundary = 1;
        Ok(())
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
            || (phase == 2 && p.events.len() != 5)
            || (phase >= 3 && p.events.len() != 5 + p.callees.len())
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
        let good = match p.terms.len() {
            0 => {
                block == b.entry && matches!(term,RTerm::Goto(t) if *t==b.header) && p.boundary == 1
            }
            1 => {
                block == b.header
                    && matches!(term,RTerm::Branch{condition,when_true,when_false} if Some(*condition)==p.condition_value && *when_true==b.body && *when_false==b.finished)
                    && p.boundary == 1
            }
            2 => {
                block == b.body && matches!(term,RTerm::Goto(t) if *t==b.header) && p.boundary == 3
            }
            3 => {
                block == b.finished
                    && matches!(term,RTerm::Goto(t) if *t==b.exit)
                    && p.boundary == 4
            }
            4 => {
                block == b.exit
                    && matches!(term, RTerm::Return(None))
                    && p.boundary == 5
                    && p.events.len() == 8 + p.callees.len()
            }
            _ => false,
        };
        if !good {
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
        let Some(p) = &self.plan else { return Ok(()) };
        if p.callees.is_empty()
            || p.callees.len() != p.lineage.uses().len()
            || p.boundary != 5
            || p.events.len() != 8 + p.callees.len()
            || p.terms.len() != 5
            || p.blocks.is_none()
            || [
                p.owner,
                p.place,
                p.source,
                p.result,
                p.flag,
                p.condition_value,
            ]
            .iter()
            .any(Option::is_none)
            || [
                p.owner,
                p.place,
                p.source,
                p.result,
                p.flag,
                p.condition_value,
            ]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
                != 6
        {
            return Err(invalid(span));
        }
        let b = p.blocks.ok_or_else(|| invalid(span))?;
        if p.literal.is_none() || p.literal == p.owner || p.owner_alias != p.literal.zip(p.owner) {
            return Err(invalid(span));
        }
        let mut expected = vec![
            Tag::Owner,
            Tag::Place,
            Tag::Source,
            Tag::Result,
            Tag::Condition,
        ];
        expected.extend((0..p.callees.len()).map(Tag::Consumer));
        expected.extend([Tag::ResultEnd, Tag::SourceEnd, Tag::OwnerDrop]);
        if p.events != expected
            || [b.entry, b.header, b.body, b.finished, b.exit]
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != 5
            || !(p.folds.is_empty() || p.folds.as_slice() == [(b.finished, b.exit)])
        {
            return Err(invalid(span));
        }
        for (index, (block, term)) in p.terms.iter().enumerate() {
            let valid = match index {
                0 => *block == b.entry && matches!(term,RTerm::Goto(t) if *t==b.header),
                1 => {
                    *block == b.header
                        && matches!(term,RTerm::Branch{condition,when_true,when_false} if Some(*condition)==p.condition_value && *when_true==b.body && *when_false==b.finished)
                }
                2 => *block == b.body && matches!(term,RTerm::Goto(t) if *t==b.header),
                3 => *block == b.finished && matches!(term,RTerm::Goto(t) if *t==b.exit),
                4 => *block == b.exit && matches!(term, RTerm::Return(None)),
                _ => false,
            };
            if !valid {
                return Err(invalid(span));
            }
        }
        Ok(())
    }
}
