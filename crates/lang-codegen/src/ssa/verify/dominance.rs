//! Definition-before-use and deterministic CFG dominance verification.

use super::{
    BlockId, Definition, EntityId, Function, Origin, VerifyError, VerifyErrorKind, VerifyLocation,
};
use std::collections::BTreeSet;

pub(in crate::ssa) fn verify_dominance(function: &Function, errors: &mut Vec<VerifyError>) {
    let dominators = compute_dominators(function);
    let instruction_positions = instruction_positions(function);
    for block in &function.blocks {
        for (position, instruction_id) in block.instructions.iter().enumerate() {
            let instruction = function
                .instruction(*instruction_id)
                .expect("structure phase proved instruction existence");
            for entity in instruction.operation.entities() {
                verify_entity_dominates(
                    function,
                    entity,
                    block.id,
                    Some(position),
                    &dominators,
                    &instruction_positions,
                    VerifyLocation::Instruction(instruction.id),
                    &instruction.origin,
                    errors,
                );
            }
        }
        let terminator = block
            .terminator
            .as_ref()
            .expect("structure phase proved terminator existence");
        for entity in terminator.kind.entities() {
            verify_entity_dominates(
                function,
                entity,
                block.id,
                None,
                &dominators,
                &instruction_positions,
                VerifyLocation::Terminator(block.id),
                &terminator.origin,
                errors,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn verify_entity_dominates(
    function: &Function,
    entity: EntityId,
    use_block: BlockId,
    use_position: Option<usize>,
    dominators: &[BTreeSet<usize>],
    instruction_positions: &[usize],
    location: VerifyLocation,
    origin: &Origin,
    errors: &mut Vec<VerifyError>,
) {
    let definition = function
        .entity(entity)
        .expect("structure phase proved entity existence")
        .definition;
    let (definition_block, definition_position) = match definition {
        Definition::BlockParameter { block, .. } => (block, None),
        Definition::InstructionResult { instruction, .. } => {
            let block = function
                .instruction(instruction)
                .expect("structure phase proved definition existence")
                .block;
            (block, Some(instruction_positions[instruction.index()]))
        }
    };
    if definition_block == use_block {
        if let Some(definition_position) = definition_position
            && use_position.is_some_and(|use_position| definition_position >= use_position)
        {
            errors.push(VerifyError {
                kind: VerifyErrorKind::UseBeforeDefinition { entity },
                location,
                origin: Some(origin.clone()),
            });
        }
    } else if !dominators[use_block.index()].contains(&definition_block.index()) {
        errors.push(VerifyError {
            kind: VerifyErrorKind::NonDominatingUse { entity },
            location,
            origin: Some(origin.clone()),
        });
    }
}

fn instruction_positions(function: &Function) -> Vec<usize> {
    let mut positions = vec![0; function.instructions.len()];
    for block in &function.blocks {
        for (position, instruction) in block.instructions.iter().enumerate() {
            positions[instruction.index()] = position;
        }
    }
    positions
}

pub(in crate::ssa) fn compute_dominators(function: &Function) -> Vec<BTreeSet<usize>> {
    let block_count = function.blocks.len();
    let mut predecessors = vec![Vec::new(); block_count];
    for block in &function.blocks {
        let terminator = block
            .terminator
            .as_ref()
            .expect("structure phase proved terminator existence");
        for target in terminator.kind.targets() {
            predecessors[target.index()].push(block.id.index());
        }
    }
    for predecessor_list in &mut predecessors {
        predecessor_list.sort_unstable();
        predecessor_list.dedup();
    }

    let reachable = reachable_blocks(function);
    let all_reachable = reachable
        .iter()
        .enumerate()
        .filter_map(|(index, reachable)| reachable.then_some(index))
        .collect::<BTreeSet<_>>();
    let mut dominators = (0..block_count)
        .map(|index| {
            if index == 0 || !reachable[index] {
                BTreeSet::from([index])
            } else {
                all_reachable.clone()
            }
        })
        .collect::<Vec<_>>();

    loop {
        let mut changed = false;
        for block in 1..block_count {
            if !reachable[block] {
                continue;
            }
            let mut incoming = predecessors[block]
                .iter()
                .filter(|predecessor| reachable[**predecessor]);
            let mut next = incoming
                .next()
                .map(|predecessor| dominators[*predecessor].clone())
                .unwrap_or_default();
            for predecessor in incoming {
                next = next
                    .intersection(&dominators[*predecessor])
                    .copied()
                    .collect();
            }
            next.insert(block);
            if next != dominators[block] {
                dominators[block] = next;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    dominators
}

fn reachable_blocks(function: &Function) -> Vec<bool> {
    let mut reachable = vec![false; function.blocks.len()];
    let mut pending = vec![0_usize];
    while let Some(block_index) = pending.pop() {
        if reachable[block_index] {
            continue;
        }
        reachable[block_index] = true;
        let block = &function.blocks[block_index];
        let terminator = block
            .terminator
            .as_ref()
            .expect("structure phase proved terminator existence");
        let mut targets = terminator.kind.targets();
        targets.sort_by_key(|target| target.index());
        for target in targets.into_iter().rev() {
            pending.push(target.index());
        }
    }
    reachable
}
