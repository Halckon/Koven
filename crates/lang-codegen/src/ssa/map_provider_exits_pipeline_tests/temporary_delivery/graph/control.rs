//! No-For controls keep their real returned owner through size and lexical cleanup.
use super::*;

pub(super) fn check(
    case: &Case,
    g: &Graph<'_>,
    module: &Module,
    parsed: &ParsedFile,
    input: &ast::Input,
    selected: (&arguments::Delivery<'_>, &Instruction),
    uses: &mut Uses,
) -> Result<(), Failure> {
    let (delivered, after) = selected;
    let owner = value(delivered.call)?;
    let anchor = EntityId::Value(owner);
    let size = at(g, ast::expression(parsed, input.size.unwrap()), |o| {
        matches!(o, Operation::MapSize { .. })
    })?;
    need(
        size.operation == Operation::MapSize { owner: anchor }
            && size.block == delivered.call.block
            && g.position(delivered.call)? < g.position(size)?
            && delivered.cleanup.iter().all(|i| {
                i.block == size.block && g.position(i).unwrap() < g.position(size).unwrap()
            }),
        "delivery control actual output size after synchronous argument cleanup",
        size,
    )?;
    let map = g.function.entity(anchor).unwrap().ty.semantic_type();
    let mut family = BTreeSet::from([anchor]);
    loop {
        let before = family.len();
        for (_, edge) in &g.edges {
            for (&argument, &parameter) in
                edge.arguments.iter().zip(&g.block(edge.target)?.parameters)
            {
                if family.contains(&argument) {
                    family.insert(parameter);
                }
            }
        }
        if before == family.len() {
            break;
        }
    }
    for &entity in &family {
        anchored(g, entity, &[anchor])?;
        uses.protect(entity);
    }
    uses.permit(anchor, size);
    let drop=one(g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::Drop {owner} if family.contains(&EntityId::Value(owner)))),
        "delivery control unique current output Drop")?;
    let Operation::Drop { owner: current } = drop.operation else {
        unreachable!()
    };
    need(
        drop.origin == Origin::Source(input.control_root.unwrap())
            && !delivered.argument_owners.contains(&current),
        "delivery control lexical binding origin and independent output owner",
        drop,
    )?;
    uses.permit(EntityId::Value(current), drop);
    if matches!(case.delivery, Delivery::SameMapBorrow) {
        need(
            drop.block == size.block && g.position(drop)? == g.position(size)? + 1,
            "delivery Pure control drops immediately after last actual size",
            drop,
        )?;
    } else {
        need(
            drop.block == after.block && g.position(after)? < g.position(drop)?,
            "delivery Resource control retains output until after marker",
            (after, drop),
        )?;
        if drop.block != delivered.call.block {
            let actual = one(
                g.block(drop.block)?
                    .parameters
                    .iter()
                    .copied()
                    .filter(|e| family.contains(e)),
                "delivery lexical cleanup actual carried owner column",
            )?;
            need(
                actual == EntityId::Value(current),
                "delivery lexical Drop consumes terminal current owner, not stale ancestor",
                (actual, drop),
            )?;
        }
    }
    need(
        g.blocks
            .iter()
            .filter(|&&b| matches!(g.term(b).unwrap().kind, TerminatorKind::Return { .. }))
            .count()
            == 1
            && !g.instructions.iter().any(|i| {
                matches!(
                    i.operation,
                    Operation::MapIterationFirst { .. }
                        | Operation::MapIterationHasNext { .. }
                        | Operation::MapIterationNext { .. }
                        | Operation::MapIterationKeyLoan { .. }
                        | Operation::MapIterationValueLoan { .. }
                )
            }),
        "delivery no-For has one normal Return and no provider operations",
        g.function.id,
    )?;
    // Rebuild the old current-owner/availability obligation from actual Unit
    // values on every path and every edge, never from an undirected alias set.
    let mut proven = BTreeSet::new();
    let mut pending = vec![(
        g.function.entry_block().unwrap(),
        None,
        0usize,
        BTreeSet::new(),
    )];
    while let Some((block, mut current, mut count, mut visited)) = pending.pop() {
        need(
            visited.insert(block),
            "delivery control graph is acyclic",
            block,
        )?;
        for instruction in g.ops(block)? {
            for entity in instruction.operation.entities() {
                if family.contains(&entity) {
                    need(
                        current == Some(entity),
                        "Unit control operand is the actual live current returned owner",
                        (instruction, current),
                    )?;
                }
            }
            if instruction.id == delivered.call.id {
                need(
                    current.is_none(),
                    "Unit control unique owner birth",
                    instruction,
                )?;
                current = Some(anchor);
                proven.insert(anchor);
            }
            if instruction.id == drop.id {
                need(
                    matches!(drop.operation, Operation::Drop { owner } if current == Some(EntityId::Value(owner))),
                    "Unit control Drop consumes its exact current owner",
                    (instruction, current),
                )?;
                current = None;
                count += 1;
            }
        }
        match &g.term(block)?.kind {
            TerminatorKind::Return { values } => need(
                count == 1 && current.is_none() && values.is_empty(),
                "Unit control every normal Return retires the original output once",
                (block, count, current),
            )?,
            TerminatorKind::Abort => {}
            _ => {
                for (_, edge) in g.edges.iter().filter(|(from, _)| *from == block) {
                    let carried = edge
                        .arguments
                        .iter()
                        .copied()
                        .filter(|entity| family.contains(entity))
                        .collect::<Vec<_>>();
                    need(
                        carried == current.into_iter().collect::<Vec<_>>(),
                        "Unit control edge carries only the live current owner, never a retired ancestor",
                        (edge, current, &carried),
                    )?;
                    let next = current
                        .map(|entity| g.transported(edge, entity))
                        .transpose()?;
                    proven.extend(next);
                    pending.push((edge.target, next, count, visited.clone()));
                }
            }
        }
    }
    need(
        proven == family,
        "Unit control entire owner family is directionally proven",
        (&proven, &family),
    )?;
    need(
        family
            .iter()
            .all(|e| g.function.entity(*e).unwrap().ty == EntityType::Value(map)),
        "delivery output family preserves actual map representation",
        &family,
    )?;
    // Any other owned Map in this caller must be the borrowed argument's actual
    // evaluator result, which has its separate CallReturn cleanup proof.
    for (index, data) in g.function.values.iter().enumerate() {
        if module.map_container(data.ty.semantic_type()).is_some() {
            need(
                family
                    .iter()
                    .any(|e| matches!(e,EntityId::Value(v) if v.index()==index))
                    || delivered.argument_owners.iter().any(|v| v.index() == index),
                "delivery no third or swapped owning Map family",
                (index, data),
            )?;
        }
    }
    need(
        proven.is_disjoint(&delivered.map_capabilities),
        "Unit control returned owner is separate from every synchronous Map argument capability",
        (&proven, &delivered.map_capabilities),
    )?;
    proven.extend(delivered.map_capabilities.iter().copied());
    let types = std::iter::once(map)
        .chain(delivered.map_types.iter().copied())
        .collect::<Vec<_>>();
    crate::ssa::map_provider_exits_pipeline_tests::query_inventory::check(g, &types, &proven)
}
