//! Full current owner/source/cursor and independent future-owner transport.
use super::*;
#[derive(Debug)]
pub(super) struct Carried<'a> {
    pub owner: EntityId,
    pub extra: Option<EntityId>,
    pub block: BlockId,
    pub blocks: [BlockId; 4],
    pub end: &'a Instruction,
}
fn via(g: &Graph<'_>, edge: &Edge, input: &[EntityId]) -> Result<Vec<EntityId>, Failure> {
    need(
        edge.arguments.len() == input.len()
            && edge.arguments.iter().copied().collect::<BTreeSet<_>>()
                == input.iter().copied().collect(),
        "update complete provider edge contains exactly all actual carried roles",
        (edge, input),
    )?;
    input.iter().map(|&e| g.transported(edge, e)).collect()
}
pub(super) fn check<'a>(
    g: &Graph<'a>,
    module: &Module,
    p: &ParsedFile,
    i: &Input,
    (owner, extra, flag_in_provider): (EntityId, Option<EntityId>, bool),
    uses: &mut Uses,
) -> Result<Carried<'a>, Failure> {
    let site = super::super::ast::site(p, i)?;
    let at = |predicate: fn(&Operation) -> bool| {
        one(
            g.instructions
                .iter()
                .copied()
                .filter(|op| op.origin == Origin::Source(site.span) && predicate(&op.operation)),
            "update exact original provider instruction",
        )
    };
    let root = at(|op| matches!(op, Operation::RootPlace { .. }))?;
    let begin = at(|op| matches!(op, Operation::BorrowBegin { .. }))?;
    let first = at(|op| matches!(op, Operation::MapIterationFirst { .. }))?;
    let has = at(|op| matches!(op, Operation::MapIterationHasNext { .. }))?;
    let next = at(|op| matches!(op, Operation::MapIterationNext { .. }))?;
    let EntityId::Value(seed) = owner else {
        return Err(failure("update actual provider source owner Value", owner));
    };
    let EntityId::Place(place) = result(root)? else {
        return Err(failure("update provider RootPlace result", root));
    };
    let lease = loan(begin)?;
    need(
        root.operation == Operation::RootPlace { owner: seed }
            && begin.operation
                == Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                }
            && first.operation == Operation::MapIterationFirst { source: lease }
            && root.block == first.block
            && begin.block == first.block
            && g.position(root)? < g.position(begin)?
            && g.position(begin)? < g.position(first)?,
        "update exact current Root/Shared/First sequence",
        (root, begin, first),
    )?;
    let map = g.function.entity(owner).unwrap().ty.semantic_type();
    let (_, key, val) = module
        .map_container(map)
        .ok_or_else(|| failure("update actual provider map storage", map))?;
    let mut initial = vec![owner, EntityId::Loan(lease), result(first)?];
    let extra_index = if let Some(extra) = extra {
        initial.push(extra);
        Some(initial.len() - 1)
    } else {
        None
    };
    if flag_in_provider {
        need(
            i.flag.is_some(),
            "update carried flag has the original Boolean formal",
            i,
        )?;
        let original = *one(
            g.block(g.function.entry_block().unwrap())?
                .parameters
                .iter(),
            "update provider original flag parameter",
        )?;
        let current = one(
            g.block(first.block)?
                .parameters
                .iter()
                .copied()
                .filter(|&e| g.anchored(e, &[original]).is_ok()),
            "update actual current flag carried into provider",
        )?;
        initial.push(current);
    }
    let entry = g.branch(first.block)?;
    let header = via(g, entry, &initial)?;
    let [
        EntityId::Value(_),
        EntityId::Loan(hs),
        EntityId::Value(hc),
        ..,
    ] = header.as_slice()
    else {
        return Err(failure("update actual header roles", &header));
    };
    need(
        has.block == entry.target
            && has.operation
                == Operation::MapIterationHasNext {
                    source: *hs,
                    cursor: *hc,
                }
            && g.ops(has.block)?.len() == 1,
        "update exact current header source/cursor",
        has,
    )?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(has.block)?.kind
    else {
        return Err(failure("update actual HasNext guard", has));
    };
    need(
        *condition == value(has)?
            && g.term(has.block)?.origin == Origin::Source(site.span)
            && !g.instructions.iter().any(|op| {
                op.operation
                    .entities()
                    .contains(&EntityId::Value(*condition))
            })
            && g.blocks.iter().all(|&b| {
                b == has.block
                    || !g
                        .term(b)
                        .unwrap()
                        .kind
                        .entities()
                        .contains(&EntityId::Value(*condition))
            })
            && !when_true.arguments.contains(&EntityId::Value(*condition))
            && !when_false.arguments.contains(&EntityId::Value(*condition)),
        "update HasNext Boolean has only its real guard use",
        has,
    )?;
    let current = via(g, when_true, &header)?;
    let exhausted = via(g, when_false, &header)?;
    let [_, EntityId::Loan(bs), EntityId::Value(bc), ..] = current.as_slice() else {
        return Err(failure("update actual body source/cursor roles", &current));
    };
    let reader = function(
        module,
        i.reader
            .ok_or_else(|| failure("update actual For reader AST", i))?,
    )?;
    let checked = body::check(
        g,
        reader,
        body::Body {
            block: when_true.target,
            span: site.span,
            read: span(p, site.read),
            arguments: site.arguments.map(|e| span(p, e)),
            types: [key, val],
        },
        *bs,
        *bc,
        uses,
    )?;
    need(
        checked.id == next.id,
        "update actual full body reader/loan cleanup/Next",
        checked,
    )?;
    let mut back_input = current.clone();
    back_input[2] = result(next)?;
    let back = g.branch(next.block)?;
    need(
        back.target == has.block && via(g, back, &back_input)? == header,
        "update complete current owner/source/next/flag/future-owner backedge",
        back,
    )?;
    g.incoming_from(has.block, &[first.block, next.block])?;
    g.incoming_from(next.block, &[has.block])?;
    g.incoming_from(when_false.target, &[has.block])?;
    let EntityId::Loan(es) = exhausted[1] else {
        unreachable!()
    };
    let end = one(
        g.instructions
            .iter()
            .copied()
            .filter(|op| op.operation == Operation::BorrowEnd { loan: es }),
        "update unique actual current provider EndSource",
    )?;
    need(
        end.block == when_false.target
            && end.origin == Origin::Source(site.span)
            && g.ops(end.block)?.first().map(|op| op.id) == Some(end.id),
        "update current source lease ends first at real exhaustion",
        end,
    )?;
    let source_entities = [initial[1], header[1], current[1], exhausted[1]]
        .into_iter()
        .collect::<BTreeSet<_>>();
    need(g.instructions.iter().filter(|op|matches!(op.operation,Operation::BorrowEnd{loan}if source_entities.contains(&EntityId::Loan(loan)))).count()==1,"update no stale pre/header/body source loan End",&source_entities)?;
    for roles in [&initial, &header, &current, &exhausted] {
        g.anchored(roles[0], &[owner])?;
        g.anchored(roles[1], &[initial[1]])?;
        g.anchored(roles[2], &[result(first)?, result(next)?])?;
        need(
            g.function.entity(roles[0]).unwrap().ty == EntityType::Value(map)
                && g.function.entity(roles[1]).unwrap().ty
                    == EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: map,
                    }
                && matches!(g.function.entity(roles[2]).unwrap().ty,EntityType::Value(t)if module.type_kind(t)==Some(&SsaTypeKind::Integer{bits:64,signed:false})),
            "update exact represented owner/source/cursor types",
            roles,
        )?;
        for (index, &entity) in roles.iter().enumerate() {
            uses.protect(entity);
            if index >= 3 {
                g.anchored(entity, &[initial[index]])?;
            }
        }
    }
    uses.permit(owner, root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), begin);
    for op in [first, has, end] {
        for entity in op.operation.entities() {
            uses.permit(entity, op);
        }
    }
    Ok(Carried {
        owner: exhausted[0],
        extra: extra_index.map(|index| exhausted[index]),
        block: when_false.target,
        blocks: [first.block, has.block, next.block, when_false.target],
        end,
    })
}
