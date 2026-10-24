//! Unit 六源的实际 owner/caller Borrow 与 provider lease 定义库存。
//! 角色已由 query_ssa 的有向列证明；同类型只用于排除额外实体，不用于认定同根。
use super::{
    fixture::{Exit, Failure, failure, need},
    query_graph::{Graph, Uses},
    query_inputs::{Inputs, one},
};
use crate::ssa::model::*;
use lang_frontend::source::Span;
use std::collections::BTreeSet;

pub(super) struct SourceRoles<'a> {
    pub blocks: &'a [BlockId],
    pub seeds: &'a [EntityId],
    pub leases: [EntityId; 4],
    pub root: Option<EntityId>,
}

fn definitions(graph: &Graph<'_>) -> Result<BTreeSet<EntityId>, Failure> {
    let f = graph.function;
    need(
        graph.blocks.len() == f.blocks.len()
            && f.blocks
                .iter()
                .enumerate()
                .all(|(index, block)| block.id.function() == f.id && block.id.index() == index)
            && graph.instructions.len() == f.instructions.len()
            && f.instructions
                .iter()
                .enumerate()
                .all(|(index, instruction)| {
                    instruction.id.function() == f.id && instruction.id.index() == index
                }),
        "identity SSA owner inventory covers actual block/instruction arenas",
        (&graph.blocks, &graph.instructions),
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
                            && data.origin == block.origin
                    }),
                "identity SSA owner inventory exact parameter definition/origin",
                (block.id, index, entity),
            )?;
        }
    }
    for instruction in &graph.instructions {
        for (index, &entity) in instruction.results.iter().enumerate() {
            need(
                entities.insert(entity)
                    && f.entity(entity).is_some_and(|data| {
                        data.definition
                            == Definition::InstructionResult {
                                instruction: instruction.id,
                                index,
                            }
                            && data.origin == instruction.origin
                    }),
                "identity SSA owner inventory exact result definition/origin",
                (instruction.id, index, entity),
            )?;
        }
    }
    need(
        entities.len() == f.values.len() + f.loans.len() + f.places.len(),
        "identity SSA owner inventory excludes every orphan entity",
        &entities,
    )?;
    Ok(entities)
}

fn parameter(
    graph: &Graph<'_>,
    block: BlockId,
    entity: EntityId,
    origin: Span,
) -> Result<(), Failure> {
    let index = one(
        graph
            .block(block)?
            .parameters
            .iter()
            .enumerate()
            .filter_map(|(index, &actual)| (actual == entity).then_some(index)),
        "identity SSA source role has one actual parameter column",
    )?;
    need(
        graph.function.entity(entity).is_some_and(|data| {
            data.definition == Definition::BlockParameter { block, index }
                && data.origin == Origin::Source(origin)
        }),
        "identity SSA source role exact parameter definition and source origin",
        (block, index, entity, origin),
    )
}

pub(super) fn check(
    module: &Module,
    graph: &Graph<'_>,
    input: &Inputs,
    roles: SourceRoles<'_>,
    uses: &mut Uses,
) -> Result<(), Failure> {
    let SourceRoles {
        blocks,
        seeds,
        leases,
        root,
    } = roles;
    let entities = definitions(graph)?;
    let borrowed_merge = !input.owned && input.exit == Exit::Break;
    need(
        blocks.len() == 4 + usize::from(input.exit == Exit::Break)
            && seeds.len() == 4 + usize::from(borrowed_merge),
        "identity SSA all source roles including only the live Borrow Break merge",
        (blocks, seeds),
    )?;
    let seed_type = graph
        .function
        .entity(seeds[0])
        .ok_or_else(|| failure("identity SSA owner inventory original parameter", seeds[0]))?
        .ty;
    let map_type = seed_type.semantic_type();
    need(
        matches!(
            module.type_kind(map_type),
            Some(SsaTypeKind::MapContainer { .. })
        ) && module.type_ownership(map_type) == Some(Ownership::MoveOnly)
            && seed_type
                == if input.owned {
                    EntityType::Value(map_type)
                } else {
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: map_type,
                    }
                }
            && graph.function.borrow_return.is_none()
            && graph.function.carrier_return.is_none(),
        "identity SSA nonnullable MoveOnly source and no borrowed/carrier return ABI",
        (seed_type, &graph.function.return_types),
    )?;
    let mut expected = BTreeSet::new();
    for (index, &seed) in seeds.iter().enumerate() {
        // Unit orchestrate gives entry parameters the callable instance span;
        // their FE Drop origin remains the original parameter span. All carried
        // blocks, including the borrowed Break merge, are created at the For.
        parameter(
            graph,
            blocks[index],
            seed,
            if index == 0 {
                input.scan
            } else {
                input.site.span
            },
        )?;
        need(
            expected.insert(seed)
                && matches!(
                    (seed, input.owned),
                    (EntityId::Value(_), true) | (EntityId::Loan(_), false)
                )
                && graph
                    .function
                    .entity(seed)
                    .is_some_and(|data| data.ty == seed_type),
            "identity SSA independent current source identities retain exact ownership type",
            seed,
        )?;
    }
    for (index, source) in leases.into_iter().enumerate() {
        need(
            expected.insert(source)
                && matches!(source, EntityId::Loan(_))
                && graph.function.entity(source).is_some_and(|data| {
                    data.ty
                        == EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: map_type,
                        }
                        && data.origin == Origin::Source(input.site.span)
                }),
            "identity SSA provider leases distinct from every owner/caller Borrow",
            source,
        )?;
        if index != 0 {
            parameter(graph, blocks[index], source, input.site.span)?;
        }
    }
    need(
        root.is_some() == input.owned,
        "identity SSA only owned source has an actual RootPlace",
        root,
    )?;
    if let Some(root) = root {
        need(
            expected.insert(root)
                && matches!(root, EntityId::Place(_))
                && graph.function.entity(root).is_some_and(|data| {
                    data.ty == EntityType::Place(map_type)
                        && data.origin == Origin::Source(input.site.span)
                }),
            "identity SSA exact owned source storage is separate from every lease",
            root,
        )?;
    }
    let actual = entities
        .into_iter()
        .filter(|&entity| {
            graph
                .function
                .entity(entity)
                .is_some_and(|data| module.map_container(data.ty.semantic_type()).is_some())
        })
        .collect::<BTreeSet<_>>();
    need(
        actual == expected,
        "identity SSA complete Map owner/loan/place inventory has no replacement or extra capability",
        (&actual, &expected),
    )?;
    // Every map entity, not just the initially discovered operands, is now in
    // the existing exact legal-use check. This adds no permitted operation.
    for entity in actual {
        uses.protect(entity);
    }
    Ok(())
}
