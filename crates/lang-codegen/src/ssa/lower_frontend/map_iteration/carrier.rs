//! Verify the actual seed, source lease, ordered cursor, and owner edge chain.
use super::*;
use crate::ssa::model::Instruction;

pub(super) struct Evidence<'a> {
    pub(super) function: &'a Function,
    pub(super) seed: EntityId,
    pub(super) source: LoanId,
    pub(super) first: ValueId,
    pub(super) next: Option<ValueId>,
    pub(super) body_exit: IterationExitKind,
    pub(super) body_owner: Option<(source_owner::Proof, SymbolId, ValueId)>,
    pub(super) returned: Option<ValueId>,
    pub(super) cleanup_start: Option<usize>,
    pub(super) continuation: BlockId,
    pub(super) key: LoanId,
    pub(super) value: LoanId,
    pub(super) blocks: [BlockId; 4],
    pub(super) carried: &'a LinearBindings,
    pub(super) source_slot: usize,
    pub(super) cursor_slot: usize,
    pub(super) owner: Option<(source_owner::Proof, SymbolId, ValueId)>,
    pub(super) parameter_origin: Span,
    pub(super) span: Span,
}

impl Evidence<'_> {
    pub(super) fn validate(self) -> Result<(), LoweringError> {
        let Self {
            function,
            seed,
            source,
            first,
            next,
            body_exit,
            body_owner,
            returned,
            cleanup_start,
            continuation,
            key,
            value,
            blocks,
            carried,
            source_slot,
            cursor_slot,
            owner,
            parameter_origin,
            span,
        } = self;
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let [preheader, header, body, exhausted] = blocks;
        let parameters = |block| {
            function
                .block(block)
                .map(|block| block.parameters.as_slice())
                .ok_or_else(missing)
        };
        let header_parameters = parameters(header)?;
        let body_parameters = parameters(body)?;
        let exhausted_parameters = parameters(exhausted)?;
        let (header_source, header_cursor) =
            roles(header_parameters, source_slot, cursor_slot, span)?;
        let (body_source, body_cursor) = roles(body_parameters, source_slot, cursor_slot, span)?;
        let (exhausted_source, _) = roles(exhausted_parameters, source_slot, cursor_slot, span)?;
        let source_instruction =
            definition(function, EntityId::Loan(source)).ok_or_else(missing)?;
        if source_instruction.block != preheader {
            return Err(missing());
        }
        match (seed, &source_instruction.operation) {
            (
                EntityId::Value(seed),
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
            ) => {
                let root = definition(function, EntityId::Place(*place)).ok_or_else(missing)?;
                if root.block != preheader
                    || root.operation != (Operation::RootPlace { owner: seed })
                {
                    return Err(missing());
                }
            }
            (EntityId::Loan(seed), Operation::SharedReborrow { source }) if seed == *source => {}
            _ => return Err(missing()),
        }
        let first_instruction = definition(function, EntityId::Value(first)).ok_or_else(missing)?;
        if first_instruction.block != preheader
            || first_instruction.operation != (Operation::MapIterationFirst { source })
        {
            return Err(missing());
        }
        let Some(TerminatorKind::Branch(entry)) = terminator(function, preheader) else {
            return Err(missing());
        };
        if entry.target != header
            || entry.arguments.get(source_slot) != Some(&EntityId::Loan(source))
            || entry.arguments.get(cursor_slot) != Some(&EntityId::Value(first))
        {
            return Err(missing());
        }
        let Some(TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        }) = terminator(function, header)
        else {
            return Err(missing());
        };
        let guard = definition(function, EntityId::Value(*condition)).ok_or_else(missing)?;
        if guard.block != header
            || guard.operation
                != (Operation::MapIterationHasNext {
                    source: header_source,
                    cursor: header_cursor,
                })
            || when_true.target != body
            || when_false.target != exhausted
            || when_true.arguments != header_parameters
            || when_false.arguments != header_parameters
        {
            return Err(missing());
        }
        for (loan, operation) in [
            (
                key,
                Operation::MapIterationKeyLoan {
                    source: body_source,
                    cursor: body_cursor,
                },
            ),
            (
                value,
                Operation::MapIterationValueLoan {
                    source: body_source,
                    cursor: body_cursor,
                },
            ),
        ] {
            let projected = definition(function, EntityId::Loan(loan)).ok_or_else(missing)?;
            if projected.block != body
                || projected.operation != operation
                || projected.results != [EntityId::Loan(loan)]
            {
                return Err(missing());
            }
        }
        if key == value {
            return Err(missing());
        }
        let body_instructions = instructions(function, body).ok_or_else(missing)?;
        let end_position =
            |loan| {
                unique(body_instructions.iter().enumerate().filter_map(
                    |(position, instruction)| {
                        (instruction.operation == Operation::BorrowEnd { loan }).then_some(position)
                    },
                ))
                .ok_or_else(missing)
            };
        let value_end = end_position(value)?;
        let key_end = end_position(key)?;
        if value_end >= key_end {
            return Err(missing());
        }
        let backedge = match body_exit {
            IterationExitKind::Fallthrough | IterationExitKind::Continue(_) => {
                let next = next.ok_or_else(missing)?;
                let instruction =
                    definition(function, EntityId::Value(next)).ok_or_else(missing)?;
                let position = body_instructions
                    .iter()
                    .position(|i| i.id == instruction.id)
                    .ok_or_else(missing)?;
                if instruction.block != body
                    || instruction.operation
                        != (Operation::MapIterationNext {
                            source: body_source,
                            cursor: body_cursor,
                        })
                    || key_end >= position
                    || returned.is_some()
                {
                    return Err(missing());
                }
                let Some(TerminatorKind::Branch(edge)) = terminator(function, body) else {
                    return Err(missing());
                };
                if edge.target != header
                    || edge.arguments.get(source_slot) != Some(&EntityId::Loan(body_source))
                    || edge.arguments.get(cursor_slot) != Some(&EntityId::Value(next))
                {
                    return Err(missing());
                }
                Some(edge)
            }
            IterationExitKind::Break(_) | IterationExitKind::Return(_) => {
                if next.is_some()
                    || body_instructions
                        .iter()
                        .any(|i| matches!(i.operation, Operation::MapIterationNext { .. }))
                    || end_position(body_source)? <= key_end
                {
                    return Err(missing());
                }
                match body_exit {
                    IterationExitKind::Break(_) => {
                        let Some(TerminatorKind::Branch(body_edge)) = terminator(function, body)
                        else {
                            return Err(missing());
                        };
                        let Some(TerminatorKind::Branch(false_edge)) =
                            terminator(function, exhausted)
                        else {
                            return Err(missing());
                        };
                        if returned.is_some()
                            || body_edge.target != continuation
                            || false_edge.target != continuation
                            || continuation == exhausted
                        {
                            return Err(missing());
                        }
                    }
                    IterationExitKind::Return(_) => {
                        let returned = returned.ok_or_else(missing)?;
                        let Some(TerminatorKind::Return { values }) = terminator(function, body)
                        else {
                            return Err(missing());
                        };
                        let call =
                            definition(function, EntityId::Value(returned)).ok_or_else(missing)?;
                        let position = body_instructions
                            .iter()
                            .position(|i| i.id == call.id)
                            .ok_or_else(missing)?;
                        if values.as_slice() != [returned]
                            || call.block != body
                            || !matches!(
                                call.operation,
                                Operation::DirectCall { receiver: None, .. }
                            )
                            || position >= cleanup_start.ok_or_else(missing)?
                            || cleanup_start.ok_or_else(missing)? > value_end
                            || continuation != exhausted
                        {
                            return Err(missing());
                        }
                    }
                    _ => return Err(missing()),
                }
                None
            }
            IterationExitKind::Exhaustion => return Err(missing()),
        };
        let exhausted_instructions = instructions(function, exhausted).ok_or_else(missing)?;
        if exhausted_instructions.iter().any(|instruction| {
            matches!(
                instruction.operation,
                Operation::MapIterationKeyLoan { .. }
                    | Operation::MapIterationValueLoan { .. }
                    | Operation::MapIterationNext { .. }
            )
        }) {
            return Err(missing());
        }
        let source_end = unique(exhausted_instructions.iter().enumerate().filter_map(
            |(position, instruction)| {
                (instruction.operation
                    == Operation::BorrowEnd {
                        loan: exhausted_source,
                    })
                .then_some(position)
            },
        ))
        .ok_or_else(missing)?;
        if let Some((proof, symbol, dropped)) = owner {
            let (slot, binding) = unique(
                carried
                    .slots
                    .iter()
                    .enumerate()
                    .filter(|(_, slot)| slot.symbol == Some(symbol)),
            )
            .ok_or_else(missing)?;
            if binding.source != seed
                || entry.arguments.get(slot) != Some(&seed)
                || backedge
                    .is_some_and(|edge| edge.arguments.get(slot) != body_parameters.get(slot))
                || exhausted_parameters.get(slot) != Some(&EntityId::Value(dropped))
                || proof.drop.value_origin() != parameter_origin
            {
                return Err(missing());
            }
            let (drop_position, instruction) = unique(
                exhausted_instructions
                    .iter()
                    .enumerate()
                    .filter(|(_, instruction)| {
                        instruction.operation == Operation::Drop { owner: dropped }
                    }),
            )
            .ok_or_else(missing)?;
            if drop_position <= source_end || instruction.origin != Origin::Source(parameter_origin)
            {
                return Err(missing());
            }
            let Some((body_proof, body_symbol, body_value)) = body_owner else {
                return Err(missing());
            };
            if body_symbol != symbol
                || body_parameters.get(slot) != Some(&EntityId::Value(body_value))
            {
                return Err(missing());
            }
            let body_drops = body_instructions
                .iter()
                .enumerate()
                .filter(|(_, instruction)| {
                    instruction.operation == Operation::Drop { owner: body_value }
                })
                .collect::<Vec<_>>();
            if exit_shape::terminal(body_exit) {
                let point = match body_exit {
                    IterationExitKind::Break(expression)
                    | IterationExitKind::Return(expression) => {
                        DropPoint::ControlTransfer(expression)
                    }
                    _ => return Err(missing()),
                };
                let fact = body_proof.at(point).ok_or_else(missing)?;
                let [(position, instruction)] = body_drops.as_slice() else {
                    return Err(missing());
                };
                if fact.value_origin() != parameter_origin
                    || *position <= end_position(body_source)?
                    || instruction.origin != Origin::Source(parameter_origin)
                {
                    return Err(missing());
                }
            } else if !body_drops.is_empty() {
                return Err(missing());
            }
        } else if !matches!(seed, EntityId::Loan(_)) {
            return Err(missing());
        }
        if let EntityId::Loan(entry_loan) = seed {
            if body_owner.is_some()
                || body_instructions
                    .iter()
                    .chain(exhausted_instructions.iter())
                    .any(|i| i.operation == Operation::BorrowEnd { loan: entry_loan })
            {
                return Err(missing());
            }
        }
        Ok(())
    }
}

fn definition(function: &Function, entity: EntityId) -> Option<&Instruction> {
    let Definition::InstructionResult {
        instruction,
        index: 0,
    } = function.entity(entity)?.definition
    else {
        return None;
    };
    let instruction = function.instruction(instruction)?;
    (instruction.results.as_slice() == [entity]).then_some(instruction)
}
fn terminator(function: &Function, block: BlockId) -> Option<&TerminatorKind> {
    function
        .block(block)?
        .terminator
        .as_ref()
        .map(|terminator| &terminator.kind)
}
fn instructions(function: &Function, block: BlockId) -> Option<Vec<&Instruction>> {
    function
        .block(block)?
        .instructions
        .iter()
        .map(|id| function.instruction(*id))
        .collect()
}
