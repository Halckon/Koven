//! 真实 Unit source 的非可选所有权运输与退役；不读取旧 FE 图或生产 carrier。
//! 身份从参数、provider operands 和每条有向 edge 建立，不做同类型或双向 union 归根。
use super::{
    fixture::{Failure, failure},
    query_inputs::{Inputs, need, one},
    query_ssa::{Shape, instructions},
};
use crate::ssa::model::*;
use std::collections::BTreeSet;

fn carried(f: &Function, edge: &Edge, source: EntityId) -> Result<EntityId, Failure> {
    let column = one(
        edge.arguments
            .iter()
            .enumerate()
            .filter_map(|(index, &argument)| (argument == source).then_some(index)),
        "SSA owner contract unique directed transport",
    )?;
    f.block(edge.target)
        .and_then(|block| block.parameters.get(column))
        .copied()
        .ok_or_else(|| failure("SSA owner contract actual target column", (edge, source)))
}

fn edge_columns(
    f: &Function,
    edge: &Edge,
    expected: &[(EntityId, EntityId); 3],
) -> Result<(), Failure> {
    let target = f
        .block(edge.target)
        .ok_or_else(|| failure("SSA owner contract target", edge))?;
    need(
        edge.arguments.len() == expected.len()
            && target.parameters.len() == expected.len()
            && expected
                .iter()
                .map(|(source, _)| source)
                .collect::<BTreeSet<_>>()
                .len()
                == 3
            && expected
                .iter()
                .map(|(_, target)| target)
                .collect::<BTreeSet<_>>()
                .len()
                == 3,
        "SSA owner/source/cursor are the complete three distinct columns",
        (edge, expected),
    )?;
    for (index, (&source, &parameter)) in edge.arguments.iter().zip(&target.parameters).enumerate()
    {
        let source_data = f
            .entity(source)
            .ok_or_else(|| failure("SSA owner contract source entity", source))?;
        let target_data = f
            .entity(parameter)
            .ok_or_else(|| failure("SSA owner contract target entity", parameter))?;
        need(
            expected.contains(&(source, parameter))
                && source != parameter
                && source_data.ty == target_data.ty
                && target_data.definition
                    == Definition::BlockParameter {
                        block: target.id,
                        index,
                    },
            "SSA every actual column has its exact source and definition",
            (edge, index, source, parameter),
        )?;
    }
    Ok(())
}

// Close the real definition inventory as well as the reachable graph: an unreferenced
// block, instruction or map-typed entity cannot hide an alternate owner/loan definition.
fn definitions(f: &Function, shape: &Shape) -> Result<BTreeSet<EntityId>, Failure> {
    let blocks = shape.blocks.into_iter().collect::<BTreeSet<_>>();
    need(
        f.blocks.len() == blocks.len()
            && f.blocks.iter().enumerate().all(|(index, block)| {
                blocks.contains(&block.id)
                    && block.id.function() == f.id
                    && block.id.index() == index
            }),
        "SSA owner contract includes every actual block",
        &f.blocks,
    )?;
    let all = instructions(f, &shape.blocks)?;
    need(
        all.len() == f.instructions.len()
            && f.instructions
                .iter()
                .enumerate()
                .all(|(index, instruction)| {
                    instruction.id.function() == f.id && instruction.id.index() == index
                }),
        "SSA owner contract includes every actual instruction",
        &all,
    )?;
    let mut entities = BTreeSet::new();
    for block in &f.blocks {
        for (index, &entity) in block.parameters.iter().enumerate() {
            need(
                entities.insert(entity)
                    && f.entity(entity).is_some_and(|data| {
                        data.definition
                            == Definition::BlockParameter {
                                block: block.id,
                                index,
                            }
                    }),
                "SSA owner contract exact parameter definition",
                (block.id, index, entity),
            )?;
        }
    }
    for instruction in all {
        for (index, &entity) in instruction.results.iter().enumerate() {
            need(
                entities.insert(entity)
                    && f.entity(entity).is_some_and(|data| {
                        data.definition
                            == Definition::InstructionResult {
                                instruction: instruction.id,
                                index,
                            }
                    }),
                "SSA owner contract exact instruction result definition",
                (instruction, index, entity),
            )?;
        }
    }
    need(
        entities.len() == f.values.len() + f.places.len() + f.loans.len(),
        "SSA owner contract has no orphan entity definitions",
        &entities,
    )?;
    Ok(entities)
}

pub(super) fn check(
    module: &Module,
    f: &Function,
    input: &Inputs,
    shape: &Shape,
) -> Result<(), Failure> {
    let entities = definitions(f, shape)?;
    let [pre, header, body, exit] = shape.blocks;
    let edge = |from, to| {
        one(
            shape.edges.iter().filter_map(|(actual, edge)| {
                (*actual == from && edge.target == to).then_some(edge)
            }),
            "SSA owner contract unique real predecessor",
        )
    };
    let entry = edge(pre, header)?;
    let taken = edge(header, body)?;
    let back = edge(body, header)?;
    let exhausted = edge(header, exit)?;
    let header_owner = carried(f, entry, shape.seed)?;
    let body_owner = carried(f, taken, header_owner)?;
    let exit_owner = carried(f, exhausted, header_owner)?;
    let exit_source = carried(f, exhausted, EntityId::Loan(shape.header_source))?;
    let exit_cursor = carried(f, exhausted, EntityId::Value(shape.header_cursor))?;
    let output = |id| -> Result<EntityId, Failure> {
        let instruction = f
            .instruction(id)
            .ok_or_else(|| failure("SSA owner contract provider instruction", id))?;
        one(
            instruction.results.iter().copied(),
            "SSA owner contract provider result",
        )
    };
    let first_cursor = output(shape.operations[0])?;
    let next_cursor = output(shape.operations[4])?;
    // This is the complete incoming relation, not an existence or alias-set assertion.
    // Both header predecessors must carry the same source lineage; the body cannot
    // replace it with another same-typed owner or add a second availability column.
    edge_columns(
        f,
        entry,
        &[
            (shape.seed, header_owner),
            (
                EntityId::Loan(shape.lease),
                EntityId::Loan(shape.header_source),
            ),
            (first_cursor, EntityId::Value(shape.header_cursor)),
        ],
    )?;
    edge_columns(
        f,
        taken,
        &[
            (header_owner, body_owner),
            (
                EntityId::Loan(shape.header_source),
                EntityId::Loan(shape.body_source),
            ),
            (
                EntityId::Value(shape.header_cursor),
                EntityId::Value(shape.body_cursor),
            ),
        ],
    )?;
    edge_columns(
        f,
        back,
        &[
            (body_owner, header_owner),
            (
                EntityId::Loan(shape.body_source),
                EntityId::Loan(shape.header_source),
            ),
            (next_cursor, EntityId::Value(shape.header_cursor)),
        ],
    )?;
    edge_columns(
        f,
        exhausted,
        &[
            (header_owner, exit_owner),
            (EntityId::Loan(shape.header_source), exit_source),
            (EntityId::Value(shape.header_cursor), exit_cursor),
        ],
    )?;
    let source_chain = [shape.seed, header_owner, body_owner, exit_owner];
    let seed_type = f
        .entity(shape.seed)
        .ok_or_else(|| failure("SSA owner contract actual parameter", shape.seed))?
        .ty;
    let map_type = seed_type.semantic_type();
    need(
        source_chain.into_iter().collect::<BTreeSet<_>>().len() == 4
            && module.type_ownership(map_type) == Some(Ownership::MoveOnly)
            && source_chain.iter().all(|&entity| {
                matches!(
                    (entity, input.owned),
                    (EntityId::Value(_), true) | (EntityId::Loan(_), false)
                ) && f.entity(entity).is_some_and(|data| data.ty == seed_type)
            })
            && seed_type
                == if input.owned {
                    EntityType::Value(map_type)
                } else {
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: map_type,
                    }
                },
        "SSA nonoptional owner or caller Borrow survives every real incoming",
        source_chain,
    )?;
    for (&entity, span) in source_chain.iter().zip([
        input.scan,
        input.loop_span,
        input.loop_span,
        input.loop_span,
    ]) {
        need(
            f.entity(entity)
                .is_some_and(|data| data.origin == Origin::Source(span)),
            "SSA source seed and carrier origins match the original function/For",
            (entity, span),
        )?;
    }
    let all = instructions(f, &shape.blocks)?;
    let mut expected_maps = source_chain.into_iter().collect::<BTreeSet<_>>();
    for entity in [
        EntityId::Loan(shape.lease),
        EntityId::Loan(shape.header_source),
        EntityId::Loan(shape.body_source),
        exit_source,
    ] {
        need(
            expected_maps.insert(entity),
            "SSA caller/owner and provider lease stay distinct",
            entity,
        )?;
    }
    if input.owned {
        let root = one(all.iter().copied().filter(|instruction|
            matches!(instruction.operation, Operation::RootPlace { owner } if EntityId::Value(owner) == shape.seed)),
            "SSA owner contract exact parameter RootPlace")?;
        let place = one(
            root.results.iter().copied(),
            "SSA owner contract unique root place",
        )?;
        need(
            matches!(place, EntityId::Place(_))
                && f.entity(place)
                    .is_some_and(|data| data.ty == EntityType::Place(map_type))
                && expected_maps.insert(place),
            "SSA owner contract actual source storage identity",
            root,
        )?;
    }
    let actual_maps = entities
        .into_iter()
        .filter(|&entity| {
            f.entity(entity)
                .is_some_and(|data| data.ty.semantic_type() == map_type)
        })
        .collect::<BTreeSet<_>>();
    need(
        actual_maps == expected_maps,
        "SSA no extra map owner/loan/place definition can hide replacement or copying",
        (actual_maps, expected_maps),
    )?;
    let drops = all.iter().copied().filter(|instruction|
        matches!(instruction.operation, Operation::Drop { owner } if source_chain.contains(&EntityId::Value(owner))))
        .collect::<Vec<_>>();
    if input.owned {
        let drop = one(
            drops.into_iter(),
            "SSA owner contract unique exhaustion Drop",
        )?;
        need(
            matches!(drop.operation, Operation::Drop { owner } if EntityId::Value(owner) == exit_owner)
                && drop.block == exit
                && drop.origin.span() == input.parameter
                && input.drop_origin == Some(input.parameter),
            "SSA availability is discharged by the exact Unit source Drop",
            drop,
        )?;
    } else {
        need(
            drops.is_empty() && input.drop_origin.is_none(),
            "SSA caller Borrow has no Drop",
            drops,
        )?;
    }
    // Existing query_ssa additionally rejects every source-chain use except the
    // exact RootPlace/Drop (owned) or seed SharedReborrow (borrowed), and enforces
    // EndSource immediately before Drop. Thus no use can revive the exhausted owner.
    for (block, expected) in [(pre, entry), (body, back)] {
        need(
            matches!(f.block(block).and_then(|block| block.terminator.as_ref()),
                Some(Terminator { kind: TerminatorKind::Branch(actual), .. }) if actual == expected),
            "SSA owner entry/backedge are unconditional exact transports",
            block,
        )?;
    }
    need(
        matches!(f.block(exit).and_then(|block| block.terminator.as_ref()),
            Some(Terminator { kind: TerminatorKind::Return { values }, .. }) if values.is_empty())
            && !shape.edges.iter().any(|(from, _)| *from == exit)
            && f.return_types.is_empty()
            && f.borrow_return.is_none()
            && f.carrier_return.is_none(),
        "SSA exhausted owner cannot escape by return or regain availability on an outgoing edge",
        f.block(exit),
    )
}
