//! Birth, resumed cleanup, and every incoming column are checked independently.
use super::*;

impl State {
    pub(in super::super) fn validate(
        &self,
        evidence: carrier::Evidence<'_>,
    ) -> Result<Roles, LoweringError> {
        let carrier::Evidence {
            function,
            seed,
            blocks,
            carried,
            source,
            first,
            source_slot,
            cursor_slot,
            cleanup_body,
            body_exit,
            key,
            value,
            span,
            ..
        } = evidence;
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let [preheader, header, body, exhausted] = blocks;
        if body_exit != IterationExitKind::Fallthrough
            || evidence.conditional.is_some()
            || evidence.owner.is_some()
            || evidence.body_owner.is_some()
            || evidence.continuation != exhausted
            || self.birth.key != key
            || self.birth.value != value
        {
            return Err(missing());
        }
        let entry_block = function.entry_block().ok_or_else(missing)?;
        if !matches!(function.entity(EntityId::Loan(self.anchor)).map(|entity| entity.definition),
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
            .map(|slot| slot.source)
            .chain([EntityId::Loan(source), EntityId::Value(first)])
            .collect::<Vec<_>>();
        let entry = edge_to(function, preheader, header, span)?;
        if entry.arguments != expected_entry
            || source_slot != carried.slots.len()
            || cursor_slot != source_slot + 1
        {
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
                .map(|block| block.parameters.as_slice())
                .ok_or_else(missing)
        };
        if parameters(body)?.get(caller_column) != Some(&EntityId::Loan(self.birth.caller)) {
            return Err(missing());
        }
        let (birth_source, birth_cursor) =
            roles(parameters(body)?, source_slot, cursor_slot, span)?;
        if birth_source != self.birth.source || birth_cursor != self.birth.cursor {
            return Err(missing());
        }
        let current = match (self.parent, self.resumed) {
            (Some(parent), None) => {
                if parent.checkpoint.statement != self.pair.outer
                    || parent.blocks != blocks
                    || cleanup_body != body
                    || parent.checkpoint.before.source == source
                    || seed != parent.checkpoint.before.caller
                {
                    return Err(missing());
                }
                parent.validate(function, span)?;
                self.birth
            }
            (None, Some(resumed)) => {
                if resumed.checkpoint.statement != self.pair.outer
                    || resumed.blocks[0] != body
                    || resumed.checkpoint.before != self.birth
                    || cleanup_body != resumed.blocks[3]
                    || resumed.resume_start.is_none()
                {
                    return Err(missing());
                }
                resumed.validate(function, span)?;
                resumed.exhausted
            }
            _ => return Err(missing()),
        };
        for (current, birth) in current.entities().into_iter().zip(self.birth.entities()) {
            transport(function, current, birth, span)?;
        }
        let backedge = edge_to(function, cleanup_body, header, span)?;
        let predecessors = incoming(function, header);
        if predecessors.len() != 2
            || predecessors
                .iter()
                .filter(|(block, _)| *block == preheader)
                .count()
                != 1
            || predecessors
                .iter()
                .filter(|(block, _)| *block == cleanup_body)
                .count()
                != 1
            || backedge.arguments.len() != expected_entry.len()
            || backedge.arguments.get(caller_column) != Some(&EntityId::Loan(current.caller))
        {
            return Err(missing());
        }
        for (column, slot) in carried.slots.iter().enumerate() {
            transport(
                function,
                *backedge.arguments.get(column).ok_or_else(missing)?,
                slot.source,
                span,
            )?;
        }
        for block in [header, body, exhausted] {
            let parameter = *parameters(block)?.get(caller_column).ok_or_else(missing)?;
            transport(function, parameter, EntityId::Loan(self.anchor), span)?;
            let EntityId::Loan(caller) = parameter else {
                return Err(missing());
            };
            if function
                .instructions
                .iter()
                .any(|instruction| instruction.operation == Operation::BorrowEnd { loan: caller })
            {
                return Err(missing());
            }
        }
        if function.instructions.iter().any(|instruction| {
            instruction.operation == Operation::BorrowEnd { loan: self.anchor }
                || instruction.operation
                    == Operation::BorrowEnd {
                        loan: current.caller,
                    }
        }) {
            return Err(missing());
        }
        Ok(current)
    }
}

impl Parent {
    fn validate(self, function: &Function, span: Span) -> Result<(), LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let [preheader, header, body, exhausted] = self.blocks;
        let parameters = |block| {
            function
                .block(block)
                .map(|block| block.parameters.as_slice())
                .ok_or_else(missing)
        };
        for (from, to, before, expected) in [
            (preheader, header, self.checkpoint.before, self.header),
            (header, body, self.header, self.body),
            (header, exhausted, self.header, self.exhausted),
            (body, header, self.body, self.header),
        ] {
            let edge = edge_to(function, from, to, span)?;
            if forwarded(
                function,
                self.checkpoint.columns,
                before,
                &edge.arguments,
                parameters(to)?,
                span,
            )? != expected
            {
                return Err(missing());
            }
        }
        let predecessors = incoming(function, header);
        if predecessors.len() != 2
            || predecessors
                .iter()
                .filter(|(block, _)| *block == preheader)
                .count()
                != 1
            || predecessors
                .iter()
                .filter(|(block, _)| *block == body)
                .count()
                != 1
        {
            return Err(missing());
        }
        let entry = edge_to(function, preheader, header, span)?;
        let source_column = entry.arguments.len().checked_sub(2).ok_or_else(missing)?;
        let EntityId::Loan(inner_source) =
            *entry.arguments.get(source_column).ok_or_else(missing)?
        else {
            return Err(missing());
        };
        let source =
            carrier::definition(function, EntityId::Loan(inner_source)).ok_or_else(missing)?;
        if source.block != preheader
            || source.operation
                != (Operation::SharedReborrow {
                    source: self.checkpoint.before.caller,
                })
            || inner_source == self.checkpoint.before.source
            || inner_source == self.checkpoint.before.caller
        {
            return Err(missing());
        }
        let EntityId::Loan(inner_false_source) = *parameters(exhausted)?
            .get(source_column)
            .ok_or_else(missing)?
        else {
            return Err(missing());
        };
        let exhausted_instructions =
            carrier::instructions(function, exhausted).ok_or_else(missing)?;
        let end = unique(exhausted_instructions.iter().enumerate().filter_map(
            |(index, instruction)| {
                (instruction.operation
                    == Operation::BorrowEnd {
                        loan: inner_false_source,
                    })
                .then_some(index)
            },
        ))
        .ok_or_else(missing)?;
        let resume_start = self.resume_start.unwrap_or(exhausted_instructions.len());
        if end >= resume_start || resume_start > exhausted_instructions.len() {
            return Err(missing());
        }
        for (block, roles, limit) in [
            (body, self.body, None),
            (exhausted, self.exhausted, Some(resume_start)),
        ] {
            let instructions = carrier::instructions(function, block).ok_or_else(missing)?;
            for instruction in instructions
                .iter()
                .take(limit.unwrap_or(instructions.len()))
            {
                if let Operation::BorrowEnd { loan } = instruction.operation
                    && roles.entities().contains(&EntityId::Loan(loan))
                {
                    return Err(missing());
                }
            }
        }
        Ok(())
    }
}
