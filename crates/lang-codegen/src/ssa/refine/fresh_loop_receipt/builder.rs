//! Actual sequential loop production, with global fold provenance and no final-graph inference.
use super::*;
impl Plans {
    pub(in crate::ssa::refine) fn current(&self) -> Option<&Plan> {
        self.loops.get(self.active?)
    }
    pub(in crate::ssa::refine) fn next(&self) -> Option<&Plan> {
        let index = match self.active {
            Some(index) => index.checked_add(1)?,
            None => 0,
        };
        self.loops.get(index)
    }
    pub(in crate::ssa::refine) fn begin(
        &mut self,
        statement: lang_frontend::type_checking::UnitStatementId,
        blocks: LoopBlocks,
        abi_slots: Vec<Slot>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.loops.is_empty() {
            return Ok(());
        }
        let next = self.next().ok_or_else(|| invalid(span))?;
        let source = *abi_slots
            .get(next.source_parameter)
            .ok_or_else(|| invalid(span))?;
        let flag = *abi_slots
            .get(next.flag_parameter)
            .ok_or_else(|| invalid(span))?;
        if abi_slots.len() != self.abi_types.len()
            || abi_slots.iter().collect::<BTreeSet<_>>().len() != abi_slots.len()
            || self
                .abi_slots
                .as_ref()
                .is_some_and(|original| original != &abi_slots)
        {
            return Err(invalid(span));
        }
        let index = match self.active {
            Some(index) => {
                let previous = self.loops.get(index).ok_or_else(|| invalid(span))?;
                validate_loop(previous, false, span)?;
                if previous.blocks.map(|b| b.exit) != Some(blocks.entry)
                    || previous.source != Some(source)
                {
                    return Err(invalid(span));
                }
                index.checked_add(1).ok_or_else(|| invalid(span))?
            }
            None => 0,
        };
        let prior_blocks = self.loops[..index.min(self.loops.len())]
            .iter()
            .filter_map(|p| p.blocks)
            .flat_map(|b| [b.entry, b.header, b.body, b.finished, b.exit])
            .collect::<BTreeSet<_>>();
        if [
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
            || [blocks.header, blocks.body, blocks.finished, blocks.exit]
                .iter()
                .any(|b| prior_blocks.contains(b))
        {
            return Err(invalid(span));
        }
        let p = self.loops.get_mut(index).ok_or_else(|| invalid(span))?;
        if p.fact.loop_statement() != statement
            || p.boundary != 0
            || !p.events.is_empty()
            || p.blocks.is_some()
            || source == flag
        {
            return Err(invalid(span));
        }
        p.blocks = Some(blocks);
        p.source = Some(source);
        p.flag = Some(flag);
        p.boundary = 1;
        self.active = Some(index);
        if self.abi_slots.is_none() {
            self.abi_slots = Some(abi_slots);
        }
        Ok(())
    }
    pub(in crate::ssa::refine) fn statement(
        &mut self,
        op: &ROp,
        results: &[Slot],
        block: BlockRef,
        span: Span,
    ) -> Result<Option<(usize, Tag)>, LoweringError> {
        if self.loops.is_empty() {
            return Ok(None);
        }
        let index = self.active.ok_or_else(|| invalid(span))?;
        let p = self.loops.get_mut(index).ok_or_else(|| invalid(span))?;
        let peer = index;
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
        Ok(Some((peer, tag)))
    }
    pub(in crate::ssa::refine) fn boundary(
        &mut self,
        phase: usize,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.loops.is_empty() {
            return Ok(());
        }
        let index = self.active.ok_or_else(|| invalid(span))?;
        let p = self.loops.get_mut(index).ok_or_else(|| invalid(span))?;
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
        if self.loops.is_empty() {
            return Ok(());
        }
        let index = self.active.ok_or_else(|| invalid(span))?;
        let last = index.checked_add(1) == Some(self.loops.len());
        let p = self.loops.get_mut(index).ok_or_else(|| invalid(span))?;
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
            4 => last && block == b.exit && p.boundary == 5 && matches!(term, RTerm::Return(None)),
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
        if self.loops.is_empty() {
            return Ok(());
        }
        self.finish(span)?;
        if self.folds.iter().any(|(a, b)| *a == from || *b == to)
            || !self
                .loops
                .iter()
                .any(|p| p.blocks.is_some_and(|b| b.finished == from && b.exit == to))
        {
            return Err(invalid(span));
        }
        // Validate the actual unique predecessor before consuming this fold callback.
        let mut predecessors = Vec::new();
        for p in &self.loops {
            for (raw, term) in &p.terms {
                let targets = match term {
                    RTerm::Goto(target) => vec![*target],
                    RTerm::Branch {
                        when_true,
                        when_false,
                        ..
                    } => vec![*when_true, *when_false],
                    RTerm::Return(None) => Vec::new(),
                    _ => return Err(invalid(span)),
                };
                for target in targets {
                    if self.canonical(target) == to {
                        predecessors.push(self.canonical(*raw));
                    }
                }
            }
        }
        if predecessors != [from] {
            return Err(invalid(span));
        }
        self.folds.push((from, to));
        Ok(())
    }
    pub(in crate::ssa::refine) fn canonical(&self, mut block: BlockRef) -> BlockRef {
        for (from, to) in &self.folds {
            if block == *to {
                block = *from;
            }
        }
        block
    }
    pub(in crate::ssa::refine) fn finish(&self, span: Span) -> Result<(), LoweringError> {
        if self.loops.is_empty() {
            return if self.active.is_none()
                && self.folds.is_empty()
                && self.abi_types.is_empty()
                && self.abi_slots.is_none()
            {
                Ok(())
            } else {
                Err(invalid(span))
            };
        }
        if self.active.and_then(|index| index.checked_add(1)) != Some(self.loops.len()) {
            return Err(invalid(span));
        }
        let first = &self.loops[0];
        let abi = self.abi_slots.as_ref().ok_or_else(|| invalid(span))?;
        let flags = self
            .loops
            .iter()
            .map(|p| p.flag_parameter)
            .collect::<BTreeSet<_>>();
        if abi.len() != self.abi_types.len()
            || abi.len() < 2
            || abi.iter().collect::<BTreeSet<_>>().len() != abi.len()
            || flags.contains(&first.source_parameter)
            || flags.len().checked_add(1) != Some(abi.len())
            || self.abi_types.iter().enumerate().any(|(index, ty)| {
                *ty != if index == first.source_parameter {
                    first.source_type
                } else {
                    first.flag_type
                }
            })
        {
            return Err(invalid(span));
        }
        let mut raw_blocks = BTreeSet::new();
        let mut fresh_slots = BTreeSet::new();
        for (index, p) in self.loops.iter().enumerate() {
            validate_loop(p, index + 1 == self.loops.len(), span)?;
            let b = p.blocks.ok_or_else(|| invalid(span))?;
            if p.caller != first.caller
                || p.fact.callable() != first.fact.callable()
                || p.fact.source() != first.fact.source()
                || p.source != first.source
                || p.source != abi.get(p.source_parameter).copied()
                || p.flag != abi.get(p.flag_parameter).copied()
                || abi
                    .iter()
                    .any(|slot| p.condition_value == Some(*slot) || p.result == Some(*slot))
                || p.source_parameter != first.source_parameter
                || p.source_type != first.source_type
                || p.flag_type != first.flag_type
                || !fresh_slots.insert(p.condition_value)
                || !fresh_slots.insert(p.result)
            {
                return Err(invalid(span));
            }
            if index == 0 {
                raw_blocks.insert(b.entry);
            } else if self.loops[index - 1].blocks.map(|previous| previous.exit) != Some(b.entry) {
                return Err(invalid(span));
            }
            if [b.header, b.body, b.finished, b.exit]
                .into_iter()
                .any(|block| !raw_blocks.insert(block))
            {
                return Err(invalid(span));
            }
            if self.loops[..index].iter().any(|previous| {
                previous.fact.binding() == p.fact.binding()
                    || previous.fact.loop_statement() == p.fact.loop_statement()
                    || previous.fact.body() == p.fact.body()
                    || previous.fact.initializer() == p.fact.initializer()
            }) {
                return Err(invalid(span));
            }
        }
        let allowed = self
            .loops
            .iter()
            .filter_map(|p| p.blocks.map(|b| (b.finished, b.exit)))
            .collect::<BTreeSet<_>>();
        if self.folds.iter().copied().collect::<BTreeSet<_>>().len() != self.folds.len()
            || self.folds.iter().any(|pair| !allowed.contains(pair))
        {
            return Err(invalid(span));
        }
        Ok(())
    }
}
fn validate_loop(p: &Plan, last: bool, span: Span) -> Result<(), LoweringError> {
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
        || p.terms.len() != if last { 5 } else { 4 }
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
