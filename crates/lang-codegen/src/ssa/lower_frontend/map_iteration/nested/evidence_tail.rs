//! Exact C/B/R topology and constructor-anchored parent transport. Cleanup event
//! order and returned-result identity are validated separately by nested_events.
use super::*;

impl State {
    pub(in super::super) fn validate_tail_transport(
        &self,
        evidence: carrier::Evidence<'_>,
    ) -> Result<(), LoweringError> {
        let carrier::Evidence {
            function,
            seed,
            source,
            first,
            blocks,
            carried,
            source_slot,
            cursor_slot,
            key,
            value,
            span,
            ..
        } = evidence;
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let shape = self.pair.tail.ok_or_else(missing)?;
        let [preheader, header, body, exhausted] = blocks;
        if evidence.conditional.is_some()
            || evidence.owner.is_some()
            || evidence.body_owner.is_some()
            || self.birth.guard.is_some()
            || self.birth.key != key
            || self.birth.value != value
            || source_slot != carried.slots.len()
            || cursor_slot != source_slot + 1
        {
            return Err(missing());
        }
        let entry_block = function.entry_block().ok_or_else(missing)?;
        if !matches!(function.entity(EntityId::Loan(self.anchor)).map(|e| e.definition),
            Some(Definition::BlockParameter { block, .. }) if block == entry_block)
        {
            return Err(missing());
        }
        let EntityId::Loan(seed) = seed else {
            return Err(missing());
        };
        transport(
            function,
            EntityId::Loan(seed),
            EntityId::Loan(self.anchor),
            span,
        )?;
        let expected_entry = carried
            .slots
            .iter()
            .map(|s| s.source)
            .chain([EntityId::Loan(source), EntityId::Value(first)])
            .collect::<Vec<_>>();
        let entry = edge_to(function, preheader, header, span)?;
        if entry.arguments != expected_entry {
            return Err(missing());
        }
        let caller_column = unique(
            carried
                .slots
                .iter()
                .enumerate()
                .filter_map(|(index, slot)| {
                    (slot.borrow_symbols.as_slice() == [self.caller]).then_some(index)
                }),
        )
        .ok_or_else(missing)?;
        let parameters = |block| {
            function
                .block(block)
                .map(|b| b.parameters.as_slice())
                .ok_or_else(missing)
        };
        let (body_source, body_cursor) = roles(parameters(body)?, source_slot, cursor_slot, span)?;
        if parameters(body)?.get(caller_column) != Some(&EntityId::Loan(self.birth.caller))
            || body_source != self.birth.source
            || body_cursor != self.birth.cursor
        {
            return Err(missing());
        }
        let (current, backedge, parent) = match (self.parent, self.resumed) {
            (Some(parent), None) => {
                if parent.checkpoint.statement != self.pair.outer
                    || parent.blocks != blocks
                    || evidence.cleanup_body != body
                    || evidence.body_exit != shape.kind
                    || evidence.continuation != parent.continuation
                    || parent.checkpoint.before.source == source
                    || parent.checkpoint.before.caller != seed
                {
                    return Err(missing());
                }
                parent.validate_tail(function, shape, span)?;
                (
                    self.birth,
                    matches!(shape.kind, IterationExitKind::Continue(_)).then_some(body),
                    parent,
                )
            }
            (None, Some(parent)) => {
                if parent.checkpoint.statement != self.pair.outer
                    || parent.blocks[0] != body
                    || parent.checkpoint.before.base_entities() != self.birth.base_entities()
                    || evidence.cleanup_body != parent.continuation
                    || evidence.body_exit != IterationExitKind::Fallthrough
                    || evidence.continuation != exhausted
                    || parent.resume_start.is_none()
                {
                    return Err(missing());
                }
                parent.validate_tail(function, shape, span)?;
                (parent.resumed_roles, Some(parent.continuation), parent)
            }
            _ => return Err(missing()),
        };
        self.validate_tail_event_roles(evidence, parent, span)?;
        for (current, birth) in current
            .base_entities()
            .into_iter()
            .zip(self.birth.base_entities())
        {
            transport(function, current, birth, span)?;
        }
        exact_predecessors(
            function,
            header,
            &std::iter::once(preheader)
                .chain(backedge)
                .collect::<Vec<_>>(),
            span,
        )?;
        exact_predecessors(function, body, &[header], span)?;
        exact_predecessors(function, exhausted, &[header], span)?;
        for target in [body, exhausted] {
            let edge = edge_to(function, header, target, span)?;
            if edge.arguments != parameters(header)?
                || parameters(target)?.len() != expected_entry.len()
            {
                return Err(missing());
            }
        }
        if let Some(backedge) = backedge {
            let edge = edge_to(function, backedge, header, span)?;
            if edge.arguments.len() != expected_entry.len()
                || edge.arguments.get(caller_column) != Some(&EntityId::Loan(current.caller))
                || edge.arguments.get(source_slot) != Some(&EntityId::Loan(current.source))
                || edge.arguments.get(cursor_slot)
                    != evidence
                        .next
                        .as_ref()
                        .map(|next| EntityId::Value(*next))
                        .as_ref()
            {
                return Err(missing());
            }
            for (column, slot) in carried.slots.iter().enumerate() {
                transport(
                    function,
                    *edge.arguments.get(column).ok_or_else(missing)?,
                    slot.source,
                    span,
                )?;
            }
        } else if evidence.next.is_some() {
            return Err(missing());
        }
        let mut caller_aliases = vec![
            self.anchor,
            seed,
            self.birth.caller,
            current.caller,
            parent.checkpoint.before.caller,
            parent.header.caller,
            parent.body.caller,
            parent.exhausted.caller,
            parent.resumed_roles.caller,
        ];
        for block in [header, body, exhausted] {
            let EntityId::Loan(caller) =
                *parameters(block)?.get(caller_column).ok_or_else(missing)?
            else {
                return Err(missing());
            };
            transport(
                function,
                EntityId::Loan(caller),
                EntityId::Loan(self.anchor),
                span,
            )?;
            caller_aliases.push(caller);
        }
        if function.instructions.iter().any(|instruction| {
            matches!(instruction.operation,
            Operation::BorrowEnd { loan } if caller_aliases.contains(&loan))
        }) {
            return Err(missing());
        }
        Ok(())
    }
}

impl State {
    /// Event records are tied back to independently recovered live edge roles.
    /// Matching the operations to a record alone would allow a self-consistent
    /// record for a same-typed but unrelated owner or component.
    fn validate_tail_event_roles(
        &self,
        evidence: carrier::Evidence<'_>,
        parent: Parent,
        span: Span,
    ) -> Result<(), LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let shape = self.pair.tail.ok_or_else(missing)?;
        let inner = if self.parent.is_some() {
            self.birth
        } else {
            let entry = edge_to(evidence.function, parent.blocks[0], parent.blocks[1], span)?;
            let source_column = entry.arguments.len().checked_sub(2).ok_or_else(missing)?;
            let parameters = &evidence
                .function
                .block(parent.blocks[2])
                .ok_or_else(missing)?
                .parameters;
            let (source, cursor) = roles(parameters, source_column, source_column + 1, span)?;
            let projected = |operation| {
                let instruction =
                    unique(evidence.function.instructions.iter().filter(|instruction| {
                        instruction.block == parent.blocks[2] && instruction.operation == operation
                    }))
                    .ok_or_else(missing)?;
                match instruction.results.as_slice() {
                    [EntityId::Loan(loan)] => Ok(*loan),
                    _ => Err(missing()),
                }
            };
            Roles {
                caller: parent.body.caller,
                source,
                cursor,
                key: projected(Operation::MapIterationKeyLoan { source, cursor })?,
                value: projected(Operation::MapIterationValueLoan { source, cursor })?,
                guard: None,
            }
        };
        for event in evidence.events {
            let expected = if event.statement == self.pair.inner && event.kind == shape.kind {
                inner
            } else if event.statement == self.pair.outer {
                match event.kind {
                    IterationExitKind::Fallthrough => parent.resumed_roles,
                    IterationExitKind::Return(_) if event.kind == shape.kind => parent.body,
                    _ => return Err(missing()),
                }
            } else {
                return Err(missing());
            };
            if event.source != expected.source
                || event.cursor != expected.cursor
                || event.key != expected.key
                || event.value != expected.value
                || expected.guard.is_some_and(|guard| guard != event.guard)
            {
                return Err(missing());
            }
            if event.statement == self.pair.inner {
                validate_guard_birth(
                    evidence.function,
                    event.guard,
                    parent.blocks[2],
                    shape.inner_guard,
                    span,
                )?;
            }
            if let Some(peer) = event.peer {
                let expected = if peer.statement == self.pair.outer
                    && event.statement == self.pair.inner
                {
                    parent.body
                } else if peer.statement == self.pair.inner && event.statement == self.pair.outer {
                    validate_guard_birth(
                        evidence.function,
                        peer.guard,
                        parent.blocks[2],
                        shape.inner_guard,
                        span,
                    )?;
                    inner
                } else {
                    return Err(missing());
                };
                if peer.source != expected.source
                    || peer.cursor != expected.cursor
                    || peer.key != expected.key
                    || peer.value != expected.value
                    || expected.guard.is_some_and(|guard| guard != peer.guard)
                {
                    return Err(missing());
                }
            }
        }
        Ok(())
    }
}

fn validate_guard_birth(
    function: &Function,
    guard: ValueId,
    block: BlockId,
    local: tail::Local,
    span: Span,
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let birth = carrier::definition(function, EntityId::Value(guard)).ok_or_else(missing)?;
    let Operation::HeapAllocate { owner, payload } = birth.operation else {
        return Err(missing());
    };
    let payload = carrier::definition(function, EntityId::Value(payload)).ok_or_else(missing)?;
    if birth.block != block
        || birth.origin != Origin::Source(local.initializer_span)
        || function
            .entity(EntityId::Value(guard))
            .map(|entity| entity.ty)
            != Some(EntityType::Value(owner))
        || payload.block != block
        || payload.origin != birth.origin
        || !matches!(&payload.operation, Operation::AggregateConstruct { fields, .. } if fields.is_empty())
    {
        return Err(missing());
    }
    Ok(())
}

impl Parent {
    fn validate_tail(
        self,
        function: &Function,
        shape: tail::Shape,
        span: Span,
    ) -> Result<(), LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let [preheader, header, body, exhausted] = self.blocks;
        let anchor = self.checkpoint.guard_anchor.ok_or_else(missing)?;
        if self.checkpoint.before.guard != Some(anchor) || self.checkpoint.guard_column.is_none() {
            return Err(missing());
        }
        let constructor =
            carrier::definition(function, EntityId::Value(anchor)).ok_or_else(missing)?;
        let Operation::HeapAllocate { payload, owner } = constructor.operation else {
            return Err(missing());
        };
        let payload =
            carrier::definition(function, EntityId::Value(payload)).ok_or_else(missing)?;
        if constructor.block != preheader
            || constructor.origin != Origin::Source(shape.outer_guard.initializer_span)
            || function.entity(EntityId::Value(anchor)).map(|e| e.ty)
                != Some(EntityType::Value(owner))
            || payload.block != preheader
            || payload.origin != constructor.origin
            || !matches!(&payload.operation, Operation::AggregateConstruct { fields, .. } if fields.is_empty())
        {
            return Err(missing());
        }
        let parameters = |block| {
            function
                .block(block)
                .map(|b| b.parameters.as_slice())
                .ok_or_else(missing)
        };
        for (from, to, before, expected) in [
            (preheader, header, self.checkpoint.before, self.header),
            (header, body, self.header, self.body),
            (header, exhausted, self.header, self.exhausted),
        ] {
            let edge = edge_to(function, from, to, span)?;
            if forwarded(
                function,
                self.checkpoint.columns,
                self.checkpoint.guard_column,
                before,
                &edge.arguments,
                parameters(to)?,
                span,
            )? != expected
            {
                return Err(missing());
            }
        }
        let backedge = match shape.kind {
            IterationExitKind::Continue(_) => Some(body),
            IterationExitKind::Break(_) | IterationExitKind::Return(_) => None,
            _ => return Err(missing()),
        };
        exact_predecessors(
            function,
            header,
            &std::iter::once(preheader)
                .chain(backedge)
                .collect::<Vec<_>>(),
            span,
        )?;
        exact_predecessors(function, body, &[header], span)?;
        exact_predecessors(function, exhausted, &[header], span)?;
        if let Some(backedge) = backedge {
            let edge = edge_to(function, backedge, header, span)?;
            if forwarded(
                function,
                self.checkpoint.columns,
                self.checkpoint.guard_column,
                self.body,
                &edge.arguments,
                parameters(header)?,
                span,
            )? != self.header
            {
                return Err(missing());
            }
        }
        let entry = edge_to(function, preheader, header, span)?;
        let source_column = entry.arguments.len().checked_sub(2).ok_or_else(missing)?;
        let EntityId::Loan(source) = *entry.arguments.get(source_column).ok_or_else(missing)?
        else {
            return Err(missing());
        };
        let source_instruction =
            carrier::definition(function, EntityId::Loan(source)).ok_or_else(missing)?;
        if source_instruction.block != preheader
            || source_instruction.operation
                != (Operation::SharedReborrow {
                    source: self.checkpoint.before.caller,
                })
            || source == self.checkpoint.before.source
            || source == self.checkpoint.before.caller
        {
            return Err(missing());
        }
        if matches!(shape.kind, IterationExitKind::Break(_)) {
            if self.continuation == exhausted
                || parameters(self.continuation)?.len() != source_column
            {
                return Err(missing());
            }
            exact_predecessors(function, self.continuation, &[body, exhausted], span)?;
            for (from, before) in [(body, self.body), (exhausted, self.exhausted)] {
                let edge = edge_to(function, from, self.continuation, span)?;
                if forwarded(
                    function,
                    self.checkpoint.columns,
                    self.checkpoint.guard_column,
                    before,
                    &edge.arguments,
                    parameters(self.continuation)?,
                    span,
                )? != self.resumed_roles
                {
                    return Err(missing());
                }
                // Prove every ordinary carry too, including non-role Copy values.
                for (column, argument) in edge.arguments.iter().enumerate() {
                    transport(
                        function,
                        *argument,
                        *entry.arguments.get(column).ok_or_else(missing)?,
                        span,
                    )?;
                }
                let EntityId::Loan(inner_source) =
                    *parameters(from)?.get(source_column).ok_or_else(missing)?
                else {
                    return Err(missing());
                };
                unique_source_end(function, from, inner_source, None, span)?;
            }
        } else if self.continuation != exhausted || self.resumed_roles != self.exhausted {
            return Err(missing());
        }
        let (false_source, false_cursor) = roles(
            parameters(exhausted)?,
            source_column,
            source_column + 1,
            span,
        )?;
        let resumed_instructions =
            carrier::instructions(function, self.continuation).ok_or_else(missing)?;
        let resume_start = self.resume_start.unwrap_or(resumed_instructions.len());
        if resume_start > resumed_instructions.len()
            || resumed_instructions
                .iter()
                .take(resume_start)
                .any(|instruction| {
                    matches!(instruction.operation, Operation::MapIterationNext { .. })
                })
        {
            return Err(missing());
        }
        // Recheck at outer validation too: the inner validation runs before the
        // shared false block has its outer-resume suffix. A false inner provider
        // never advances, and only this parent may advance after resumption.
        let false_instructions = carrier::instructions(function, exhausted).ok_or_else(missing)?;
        for (index, instruction) in false_instructions.iter().enumerate() {
            if let Operation::MapIterationNext { source, cursor } = instruction.operation
                && (source == false_source
                    || cursor == false_cursor
                    || self.continuation != exhausted
                    || index < resume_start
                    || source != self.resumed_roles.source
                    || cursor != self.resumed_roles.cursor)
            {
                return Err(missing());
            }
        }
        unique_source_end(
            function,
            exhausted,
            false_source,
            (self.continuation == exhausted).then_some(resume_start),
            span,
        )?;
        for roles in [self.header, self.body, self.exhausted, self.resumed_roles] {
            let guard = roles.guard.ok_or_else(missing)?;
            for (current, birth) in roles
                .entities()
                .into_iter()
                .zip(self.checkpoint.before.entities())
            {
                transport(function, current, birth, span)?;
            }
            transport(
                function,
                EntityId::Value(guard),
                EntityId::Value(anchor),
                span,
            )?;
        }
        if carrier::instructions(function, body)
            .ok_or_else(missing)?
            .iter()
            .any(|instruction| {
                matches!(instruction.operation, Operation::MapIterationNext { source, cursor }
                if source == self.body.source || cursor == self.body.cursor)
            })
        {
            return Err(missing());
        }
        // Parent resources must stay untouched until the branch's real outer
        // cleanup point. The one admitted exception is the full Return event.
        if !matches!(shape.kind, IterationExitKind::Return(_)) {
            forbid_consumption(function, body, self.body, None, span)?;
        }
        forbid_consumption(
            function,
            exhausted,
            self.exhausted,
            (self.continuation == exhausted).then_some(resume_start),
            span,
        )?;
        if self.continuation != exhausted {
            forbid_consumption(
                function,
                self.continuation,
                self.resumed_roles,
                Some(resume_start),
                span,
            )?;
        }
        Ok(())
    }
}

fn exact_predecessors(
    function: &Function,
    target: BlockId,
    expected: &[BlockId],
    span: Span,
) -> Result<(), LoweringError> {
    let actual = incoming(function, target);
    if actual.len() != expected.len()
        || expected
            .iter()
            .any(|block| actual.iter().filter(|(actual, _)| actual == block).count() != 1)
    {
        return Err(error(LoweringErrorKind::MissingFact, span));
    }
    Ok(())
}

fn unique_source_end(
    function: &Function,
    block: BlockId,
    source: LoanId,
    limit: Option<usize>,
    span: Span,
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let instructions = carrier::instructions(function, block).ok_or_else(missing)?;
    let end = unique(
        instructions
            .iter()
            .enumerate()
            .filter_map(|(index, instruction)| {
                (instruction.operation == Operation::BorrowEnd { loan: source }).then_some(index)
            }),
    )
    .ok_or_else(missing)?;
    if limit.is_some_and(|limit| end >= limit) {
        return Err(missing());
    }
    Ok(())
}

fn forbid_consumption(
    function: &Function,
    block: BlockId,
    roles: Roles,
    limit: Option<usize>,
    span: Span,
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let instructions = carrier::instructions(function, block).ok_or_else(missing)?;
    let entities = roles.entities();
    if instructions
        .iter()
        .take(limit.unwrap_or(instructions.len()))
        .any(|instruction| match instruction.operation {
            Operation::BorrowEnd { loan } => entities.contains(&EntityId::Loan(loan)),
            Operation::Drop { owner } => entities.contains(&EntityId::Value(owner)),
            _ => false,
        })
    {
        return Err(missing());
    }
    Ok(())
}
