//! Actual argument, append and normal statement completion for external-source segments.
use super::*;
impl Plans {
    pub(in crate::ssa::refine) fn bind_abi(
        &mut self,
        slots: Vec<Slot>,
        block: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.loops.is_empty() {
            return Ok(());
        }
        if self.abi_slots.is_some()
            || self.active.is_some()
            || slots.len() != self.abi_types.len()
            || slots.len() != self.abi_modes.len()
            || slots.iter().collect::<BTreeSet<_>>().len() != slots.len()
            || self.segments.len()
                != self
                    .loops
                    .len()
                    .checked_add(1)
                    .ok_or_else(|| invalid(span))?
        {
            return Err(invalid(span));
        }
        let segment = self.segments.first_mut().ok_or_else(|| invalid(span))?;
        if segment.block.is_some() {
            return Err(invalid(span));
        }
        segment.block = Some(block);
        self.abi_slots = Some(slots);
        Ok(())
    }
    pub(in crate::ssa::refine) fn has_source_call(&self, call: UnitExpressionId) -> bool {
        self.segments
            .iter()
            .any(|s| s.consumers.iter().any(|c| c.call == call))
    }
    pub(in crate::ssa::refine) fn has_source_statement(&self, statement: UnitStatementId) -> bool {
        self.segments
            .iter()
            .any(|s| s.consumers.iter().any(|c| c.statement == statement))
    }
    fn segment_index(&self, span: Span) -> Result<usize, LoweringError> {
        match self.active {
            None => Ok(0),
            Some(index) if self.loops.get(index).is_some_and(|p| p.boundary == 5) => {
                index.checked_add(1).ok_or_else(|| invalid(span))
            }
            _ => Err(invalid(span)),
        }
    }
    pub(in crate::ssa::refine) fn source_operand(
        &mut self,
        call: UnitExpressionId,
        argument: UnitExpressionId,
        source: Slot,
        block: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        if !self.has_source_call(call) {
            return Ok(());
        }
        let first = self.loops.first().ok_or_else(|| invalid(span))?;
        if self
            .abi_slots
            .as_ref()
            .and_then(|s| s.get(first.source_parameter))
            .copied()
            != Some(source)
        {
            return Err(invalid(span));
        }
        let index = self.segment_index(span)?;
        let segment = self.segments.get_mut(index).ok_or_else(|| invalid(span))?;
        let expected = segment
            .consumers
            .get(segment.completed)
            .ok_or_else(|| invalid(span))?;
        if segment.block != Some(block)
            || segment.pending
            || segment.appended
            || expected.call != call
            || expected.argument != argument
            || expected.argument_span != span
        {
            return Err(invalid(span));
        }
        segment.pending = true;
        Ok(())
    }
    pub(super) fn source_append(
        &mut self,
        op: &ROp,
        results: &[Slot],
        block: BlockRef,
        span: Span,
    ) -> Result<Option<(usize, Tag)>, LoweringError> {
        let Some(index) = self.segments.iter().position(|s| s.pending) else {
            return Ok(None);
        };
        if self.segment_index(span)? != index {
            return Err(invalid(span));
        }
        let first = self.loops.first().ok_or_else(|| invalid(span))?;
        let source = *self
            .abi_slots
            .as_ref()
            .and_then(|s| s.get(first.source_parameter))
            .ok_or_else(|| invalid(span))?;
        let segment = self.segments.get_mut(index).ok_or_else(|| invalid(span))?;
        let expected = segment
            .consumers
            .get(segment.completed)
            .ok_or_else(|| invalid(span))?;
        if segment.appended
            || segment.block != Some(block)
            || expected.span != span
            || !results.is_empty()
            || !matches!(op, ROp::DirectCall { callee, receiver: None, arguments } if *callee == expected.callee && arguments.as_slice() == [source])
        {
            return Err(invalid(span));
        }
        segment.appended = true;
        Ok(Some((index, Tag::SourceConsumer(segment.completed))))
    }
    pub(in crate::ssa::refine) fn source_complete(
        &mut self,
        statement: UnitStatementId,
        block: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        let index = self.segment_index(span)?;
        let segment = self.segments.get_mut(index).ok_or_else(|| invalid(span))?;
        let expected = segment
            .consumers
            .get(segment.completed)
            .ok_or_else(|| invalid(span))?;
        if segment.block != Some(block)
            || !segment.pending
            || !segment.appended
            || expected.statement != statement
        {
            return Err(invalid(span));
        }
        segment.completed = segment
            .completed
            .checked_add(1)
            .ok_or_else(|| invalid(span))?;
        segment.pending = false;
        segment.appended = false;
        Ok(())
    }
    pub(super) fn segment_complete(
        &self,
        index: usize,
        block: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        let segment = self.segments.get(index).ok_or_else(|| invalid(span))?;
        if segment.block != Some(block)
            || segment.pending
            || segment.appended
            || segment.completed != segment.consumers.len()
        {
            return Err(invalid(span));
        }
        Ok(())
    }
    pub(super) fn validate_segments(&self, span: Span) -> Result<(), LoweringError> {
        if self.segments.len()
            != self
                .loops
                .len()
                .checked_add(1)
                .ok_or_else(|| invalid(span))?
        {
            return Err(invalid(span));
        }
        let first = self.loops.first().ok_or_else(|| invalid(span))?;
        let mut ids = BTreeSet::new();
        let mut statements = BTreeSet::new();
        for (index, segment) in self.segments.iter().enumerate() {
            let block = if index == 0 {
                first.blocks.map(|b| b.entry)
            } else {
                self.loops[index - 1].blocks.map(|b| b.exit)
            }
            .ok_or_else(|| invalid(span))?;
            self.segment_complete(index, block, span)?;
            for consumer in &segment.consumers {
                if consumer.call.source_unit() != first.fact.source().source_unit()
                    || consumer.argument.source_unit() != consumer.call.source_unit()
                    || consumer.statement.source_unit() != consumer.call.source_unit()
                    || !ids.insert(consumer.call)
                    || !ids.insert(consumer.argument)
                    || !statements.insert(consumer.statement)
                    || consumer.span.source_id() != span.source_id()
                    || consumer.argument_span.source_id() != span.source_id()
                {
                    return Err(invalid(span));
                }
            }
        }
        Ok(())
    }
}
