//! One direct-entry concat of two distinct literal temporaries, fully cleaned.
use super::*;
use crate::ssa::model::Definition;
pub(super) fn verify(
    function: &Function,
    entry: BlockId,
    producer: InstructionId,
    result: EntityId,
    span: Span,
) -> Result<(), LoweringError> {
    let fail = || invalid(span);
    let block = function.block(entry).ok_or_else(fail)?;
    let concat = function.instruction(producer).ok_or_else(fail)?;
    let Operation::StringConcat { left, right } = concat.operation else {
        return Err(fail());
    };
    if block
        .instructions
        .iter()
        .filter(|id| {
            function
                .instruction(**id)
                .is_some_and(|i| matches!(i.operation, Operation::StringConcat { .. }))
        })
        .count()
        != 1
        || concat.block != entry
        || concat.results != [result]
        || [left, right, result]
            .into_iter()
            .collect::<BTreeSet<_>>()
            .len()
            != 3
    {
        return Err(fail());
    }
    let position = |id| {
        let indexes = block
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(index, actual)| (*actual == id).then_some(index))
            .collect::<Vec<_>>();
        match indexes.as_slice() {
            [index] => Ok(*index),
            _ => Err(fail()),
        }
    };
    let at = position(producer)?;
    for input in [left, right] {
        let EntityId::Value(_) = input else {
            return Err(fail());
        };
        let Definition::InstructionResult {
            instruction: literal,
            index: 0,
        } = function.entity(input).ok_or_else(fail)?.definition
        else {
            return Err(fail());
        };
        let source = function.instruction(literal).ok_or_else(fail)?;
        if source.block != entry
            || source.results != [input]
            || !matches!(source.operation, Operation::StringLiteral { .. })
            || position(literal)? >= at
        {
            return Err(fail());
        }
        let drops = function.instructions.iter().filter(|i|
            matches!(i.operation, Operation::Drop { owner } if EntityId::Value(owner) == input)
        ).collect::<Vec<_>>();
        let [drop] = drops.as_slice() else {
            return Err(fail());
        };
        if drop.block != entry || !drop.results.is_empty() || position(drop.id)? <= at {
            return Err(fail());
        }
        for instruction in &function.instructions {
            if instruction.results.contains(&input) != (instruction.id == literal)
                || instruction
                    .operation
                    .entities()
                    .iter()
                    .filter(|e| **e == input)
                    .count()
                    != usize::from(instruction.id == producer || instruction.id == drop.id)
            {
                return Err(fail());
            }
        }
        for b in &function.blocks {
            if b.parameters.contains(&input)
                || b.terminator
                    .as_ref()
                    .ok_or_else(fail)?
                    .kind
                    .entities()
                    .contains(&input)
            {
                return Err(fail());
            }
        }
    }
    #[cfg(test)]
    super::super::build::owner_consumer_test_support::record_action(
        "parent-initial-concat-source-ssa-complete",
        1,
    );
    Ok(())
}
