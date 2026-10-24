//! 完整定义库存只排除额外能力；expected 必须来自调用方的真实 birth 与逐边身份证明。
use super::{
    fixture::{Failure, failure, need},
    query_graph::Graph,
};
use crate::ssa::model::*;
use std::collections::BTreeSet;

pub(super) fn check(
    graph: &Graph<'_>,
    relevant_semantic_types: &[SsaTypeId],
    expected_entities: &BTreeSet<EntityId>,
) -> Result<(), Failure> {
    let f = graph.function;
    let blocks = graph.blocks.iter().copied().collect::<BTreeSet<_>>();
    let instructions = graph
        .instructions
        .iter()
        .map(|i| i.id)
        .collect::<BTreeSet<_>>();
    need(
        blocks.len() == graph.blocks.len()
            && blocks.len() == f.blocks.len()
            && f.blocks.iter().enumerate().all(|(index, block)| {
                block.id.function() == f.id
                    && block.id.index() == index
                    && blocks.contains(&block.id)
            })
            && instructions.len() == graph.instructions.len()
            && instructions.len() == f.instructions.len()
            && f.instructions
                .iter()
                .enumerate()
                .all(|(index, instruction)| {
                    instruction.id.function() == f.id
                        && instruction.id.index() == index
                        && instructions.contains(&instruction.id)
                }),
        "Unit identity inventory covers exact reachable block/instruction arenas",
        (&graph.blocks, &instructions),
    )?;
    let mut entities = BTreeSet::new();
    for block in &f.blocks {
        for (index, &entity) in block.parameters.iter().enumerate() {
            need(
                entity.function() == f.id
                    && entities.insert(entity)
                    && f.entity(entity).is_some_and(|data| {
                        data.definition
                            == Definition::BlockParameter {
                                block: block.id,
                                index,
                            }
                            && data.origin == block.origin
                    }),
                "Unit identity inventory exact parameter definition and origin",
                (block.id, index, entity),
            )?;
        }
    }
    for instruction in &f.instructions {
        need(
            f.block(instruction.block).is_some_and(|block| {
                block
                    .instructions
                    .iter()
                    .filter(|&&id| id == instruction.id)
                    .count()
                    == 1
            }),
            "Unit identity inventory exact instruction block membership",
            instruction.id,
        )?;
        for (index, &entity) in instruction.results.iter().enumerate() {
            need(
                entity.function() == f.id
                    && entities.insert(entity)
                    && f.entity(entity).is_some_and(|data| {
                        data.definition
                            == Definition::InstructionResult {
                                instruction: instruction.id,
                                index,
                            }
                            && data.origin == instruction.origin
                    }),
                "Unit identity inventory exact result definition and origin",
                (instruction.id, index, entity),
            )?;
        }
    }
    need(
        entities.len() == f.values.len() + f.loans.len() + f.places.len(),
        "Unit identity inventory excludes every orphan entity",
        &entities,
    )?;
    for &entity in expected_entities {
        let data = f
            .entity(entity)
            .ok_or_else(|| failure("Unit identity inventory actual proven entity", entity))?;
        need(
            entities.contains(&entity)
                && relevant_semantic_types.contains(&data.ty.semantic_type()),
            "Unit identity inventory expected roles belong to exact relevant types",
            (entity, data),
        )?;
    }
    let actual = entities
        .into_iter()
        .filter(|&entity| {
            f.entity(entity)
                .is_some_and(|data| relevant_semantic_types.contains(&data.ty.semantic_type()))
        })
        .collect::<BTreeSet<_>>();
    need(
        &actual == expected_entities,
        "Unit identity complete related owner/loan/place inventory has no extra capability",
        (&actual, expected_entities),
    )
}
