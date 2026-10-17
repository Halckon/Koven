//! Verify the actual seed, source lease, ordered cursor, and owner edge chain.
use super::*;
use crate::ssa::model::Instruction;

pub(super) struct Evidence<'a> {
    pub(super) function: &'a Function,
    pub(super) seed: EntityId,
    pub(super) source: LoanId,
    pub(super) first: ValueId,
    pub(super) next: ValueId,
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
        let next_instruction = definition(function, EntityId::Value(next)).ok_or_else(missing)?;
        let next_position = body_instructions
            .iter()
            .position(|instruction| instruction.id == next_instruction.id)
            .ok_or_else(missing)?;
        if next_instruction.block != body
            || next_instruction.operation
                != (Operation::MapIterationNext {
                    source: body_source,
                    cursor: body_cursor,
                })
            || !(value_end < key_end && key_end < next_position)
        {
            return Err(missing());
        }
        let Some(TerminatorKind::Branch(backedge)) = terminator(function, body) else {
            return Err(missing());
        };
        if backedge.target != header
            || backedge.arguments.get(source_slot) != Some(&EntityId::Loan(body_source))
            || backedge.arguments.get(cursor_slot) != Some(&EntityId::Value(next))
        {
            return Err(missing());
        }
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
                || backedge.arguments.get(slot) != body_parameters.get(slot)
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
        } else if !matches!(seed, EntityId::Loan(_)) {
            return Err(missing());
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
