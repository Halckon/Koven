//! Whole-function SSA ownership and definition audit, independent of producer plans.
use super::invalid;
use crate::ssa::{
    lowering_support::LoweringError,
    model::{
        BlockId, Definition, EntityId, EntityType, Function, InstructionId, LoanId, PlaceId,
        TerminatorKind, ValueId,
    },
};
use lang_frontend::source::Span;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub(super) fn check(
    function: &Function,
    covered_blocks: &BTreeSet<BlockId>,
    covered_instructions: &BTreeSet<InstructionId>,
    span: Span,
) -> Result<(), LoweringError> {
    let id = function.id();
    let entry = function.entry_block().ok_or_else(|| invalid(span))?;
    let mut blocks = BTreeSet::new();
    let mut instructions = BTreeSet::new();
    let mut definitions = BTreeSet::new();
    let entity = |e: EntityId, definition: Definition| -> Result<(), LoweringError> {
        let data = function.entity(e).ok_or_else(|| invalid(span))?;
        if e.function() != id
            || data.definition != definition
            || data.ty.semantic_type().module() != id.module()
            || !matches!(
                (e, data.ty),
                (EntityId::Value(_), EntityType::Value(_))
                    | (EntityId::Place(_), EntityType::Place(_))
                    | (EntityId::Loan(_), EntityType::Loan { .. })
            )
        {
            return Err(invalid(span));
        }
        Ok(())
    };
    let mut edges = BTreeMap::new();
    for (index, block) in function.blocks.iter().enumerate() {
        if block.id.function() != id || block.id.index() != index || !blocks.insert(block.id) {
            return Err(invalid(span));
        }
        for (index, e) in block.parameters.iter().enumerate() {
            if !definitions.insert(*e) {
                return Err(invalid(span));
            }
            entity(
                *e,
                Definition::BlockParameter {
                    block: block.id,
                    index,
                },
            )?;
        }
        for instruction in &block.instructions {
            let data = function
                .instruction(*instruction)
                .ok_or_else(|| invalid(span))?;
            if instruction.function() != id
                || data.id != *instruction
                || data.block != block.id
                || !instructions.insert(*instruction)
            {
                return Err(invalid(span));
            }
            for (index, e) in data.results.iter().enumerate() {
                if !definitions.insert(*e) {
                    return Err(invalid(span));
                }
                entity(
                    *e,
                    Definition::InstructionResult {
                        instruction: *instruction,
                        index,
                    },
                )?;
            }
        }
        let term = &block.terminator.as_ref().ok_or_else(|| invalid(span))?.kind;
        let successors = match term {
            TerminatorKind::Branch(edge) => vec![edge],
            TerminatorKind::Conditional {
                condition,
                when_true,
                when_false,
            } => {
                let data = function
                    .entity(EntityId::Value(*condition))
                    .ok_or_else(|| invalid(span))?;
                if !matches!(data.ty, EntityType::Value(_)) {
                    return Err(invalid(span));
                }
                vec![when_true, when_false]
            }
            TerminatorKind::Return { values } if values.is_empty() => Vec::new(),
            _ => return Err(invalid(span)),
        };
        let mut targets = Vec::new();
        for edge in successors {
            let target = function.block(edge.target).ok_or_else(|| invalid(span))?;
            if target.id != edge.target || target.parameters.len() != edge.arguments.len() {
                return Err(invalid(span));
            }
            for (argument, parameter) in edge.arguments.iter().zip(&target.parameters) {
                let a = function.entity(*argument).ok_or_else(|| invalid(span))?;
                let p = function.entity(*parameter).ok_or_else(|| invalid(span))?;
                if a.ty != p.ty {
                    return Err(invalid(span));
                }
            }
            targets.push(edge.target);
        }
        edges.insert(block.id, targets);
    }
    for (index, instruction) in function.instructions.iter().enumerate() {
        if instruction.id.function() != id
            || instruction.id.index() != index
            || !instructions.contains(&instruction.id)
        {
            return Err(invalid(span));
        }
    }
    let metadata = (0..function.values.len())
        .map(|index| {
            EntityId::Value(ValueId {
                function: id,
                index,
            })
        })
        .chain((0..function.places.len()).map(|index| {
            EntityId::Place(PlaceId {
                function: id,
                index,
            })
        }))
        .chain((0..function.loans.len()).map(|index| {
            EntityId::Loan(LoanId {
                function: id,
                index,
            })
        }))
        .collect::<BTreeSet<_>>();
    if definitions != metadata
        || &blocks != covered_blocks
        || &instructions != covered_instructions
        || instructions.len() != function.instructions.len()
    {
        return Err(invalid(span));
    }
    let mut reached = BTreeSet::new();
    let mut pending = VecDeque::from([entry]);
    while let Some(block) = pending.pop_front() {
        if reached.insert(block) {
            for target in edges.get(&block).ok_or_else(|| invalid(span))? {
                pending.push_back(*target);
            }
        }
    }
    if reached != blocks {
        return Err(invalid(span));
    }
    Ok(())
}
