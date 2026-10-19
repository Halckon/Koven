//! Exact owner/source/cursor columns over all four real provider edges.
use super::*;
use crate::ssa::map_provider_exits_pipeline_tests::query_map_normal_body as body;
pub(super) struct Carried<'a> {
    pub owner: EntityId,
    pub pre: BlockId,
    pub header: BlockId,
    pub body: BlockId,
    pub exit: BlockId,
    pub end: &'a Instruction,
}
fn via(g: &Graph<'_>, edge: &Edge, roles: [EntityId; 3]) -> Result<[EntityId; 3], Failure> {
    need(
        edge.arguments.len() == 3
            && edge.arguments.iter().copied().collect::<BTreeSet<_>>()
                == roles.into_iter().collect(),
        "D every complete edge carries exactly current owner/source/cursor",
        (edge, roles),
    )?;
    Ok([
        g.transported(edge, roles[0])?,
        g.transported(edge, roles[1])?,
        g.transported(edge, roles[2])?,
    ])
}
pub(super) fn check<'a>(
    g: &Graph<'a>,
    m: &Module,
    p: &ParsedFile,
    s: &ast::Site,
    owner: EntityId,
    uses: &mut Uses,
) -> Result<Carried<'a>, Failure> {
    let at = |test: fn(&Operation) -> bool| {
        one(
            g.instructions
                .iter()
                .copied()
                .filter(|op| op.origin == Origin::Source(s.span) && test(&op.operation)),
            "D unique actual original provider operation",
        )
    };
    let root = at(|op| matches!(op, Operation::RootPlace { .. }))?;
    let begin = at(|op| matches!(op, Operation::BorrowBegin { .. }))?;
    let first = at(|op| matches!(op, Operation::MapIterationFirst { .. }))?;
    let has = at(|op| matches!(op, Operation::MapIterationHasNext { .. }))?;
    let next = at(|op| matches!(op, Operation::MapIterationNext { .. }))?;
    let EntityId::Value(seed) = owner else {
        return Err(failure("D source actual owned Value", owner));
    };
    let EntityId::Place(place) = result(root)? else {
        return Err(failure("D provider actual RootPlace result", root));
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
            && [root, begin].iter().all(|op| op.block == first.block)
            && g.position(root)? < g.position(begin)?
            && g.position(begin)? < g.position(first)?,
        "D actual current source Root/Shared lease/First sequence",
        (root, begin, first),
    )?;
    let map = g.function.entity(owner).unwrap().ty.semantic_type();
    let (_, key, val) = m
        .map_container(map)
        .ok_or_else(|| failure("D represented source Map", map))?;
    let initial = [owner, EntityId::Loan(lease), result(first)?];
    let enter = g.branch(first.block)?;
    let header = via(g, enter, initial)?;
    let [_, EntityId::Loan(hs), EntityId::Value(hc)] = header else {
        return Err(failure("D exact Header role kinds", header));
    };
    need(
        has.block == enter.target
            && has.operation
                == Operation::MapIterationHasNext {
                    source: hs,
                    cursor: hc,
                }
            && g.ops(has.block)?.len() == 1,
        "D Header guard uses current source/cursor only",
        has,
    )?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(has.block)?.kind
    else {
        return Err(failure("D actual HasNext conditional", has));
    };
    need(
        *condition == value(has)?
            && g.term(has.block)?.origin == Origin::Source(s.span)
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
        "D HasNext result is used only as its actual guard",
        has,
    )?;
    let current = via(g, when_true, header)?;
    let exhausted = via(g, when_false, header)?;
    let [_, EntityId::Loan(bs), EntityId::Value(bc)] = current else {
        return Err(failure("D exact body roles", current));
    };
    let reader = function(m, s.reader)?;
    let checked = body::check(
        g,
        reader,
        body::Body {
            block: when_true.target,
            span: s.span,
            read: span(p, s.read),
            arguments: s.arguments.map(|e| span(p, e)),
            types: [key, val],
        },
        bs,
        bc,
        uses,
    )?;
    need(
        checked.id == next.id,
        "D full normal body reader/reborrows/reverse component Ends/Next",
        checked,
    )?;
    let back = g.branch(next.block)?;
    need(
        back.target == has.block
            && via(g, back, [current[0], current[1], result(next)?])? == header,
        "D complete current owner/source/next backedge",
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
        "D exact current provider lease End",
    )?;
    need(
        end.block == when_false.target
            && end.origin == Origin::Source(s.span)
            && g.ops(end.block)?.first().map(|op| op.id) == Some(end.id),
        "D EndSource is first at actual exhaustion",
        end,
    )?;
    let sources = [initial[1], header[1], current[1], exhausted[1]]
        .into_iter()
        .collect::<BTreeSet<_>>();
    need(g.instructions.iter().filter(|op|matches!(op.operation,Operation::BorrowEnd{loan}if sources.contains(&EntityId::Loan(loan)))).count()==1,"D no stale earlier provider source End",&sources)?;
    for roles in [initial, header, current, exhausted] {
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
                && matches!(g.function.entity(roles[2]).unwrap().ty,EntityType::Value(t)if m.type_kind(t)==Some(&SsaTypeKind::Integer{bits:64,signed:false})),
            "D exact represented current owner/source/cursor types",
            roles,
        )?;
        for e in roles {
            uses.protect(e);
        }
    }
    uses.permit(owner, root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), begin);
    for op in [first, has, end] {
        for e in op.operation.entities() {
            uses.permit(e, op);
        }
    }
    Ok(Carried {
        owner: exhausted[0],
        pre: first.block,
        header: has.block,
        body: next.block,
        exit: when_false.target,
        end,
    })
}
