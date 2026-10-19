//! Every Int-provider owner/source/cursor column and both normal endpoints are actual.
use super::super::super::super::{births::ast::Site, query_map_normal_body as body};
use super::*;
pub(super) fn check(
    g: &Graph<'_>,
    module: &Module,
    reader: &Function,
    p: &ParsedFile,
    selected: (&Site, EntityId, &Instruction),
    uses: &mut Uses,
) -> Result<(), Failure> {
    let (site, seed, drop) = selected;
    let EntityId::Value(owner) = seed else {
        return Err(failure("Int actual source is owner Value", seed));
    };
    let map = g.function.entity(seed).unwrap().ty.semantic_type();
    let (_, kt, vt) = module.map_container(map).unwrap();
    let root = one(
        g.instructions
            .iter()
            .copied()
            .filter(|op| op.operation == Operation::RootPlace { owner }),
        "Int actual current source root",
    )?;
    let EntityId::Place(place) = result(root)? else {
        return Err(failure("Int actual source Place", root));
    };
    let begin = one(
        g.instructions.iter().copied().filter(|op| {
            op.operation
                == Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                }
        }),
        "Int actual source Shared lease",
    )?;
    let first = at(g, site.span, |op| {
        matches!(op, Operation::MapIterationFirst { .. })
    })?;
    let has = at(g, site.span, |op| {
        matches!(op, Operation::MapIterationHasNext { .. })
    })?;
    let source = loan(begin)?;
    let cursor = value(first)?;
    need(
        first.operation == Operation::MapIterationFirst { source }
            && [root, begin, first].iter().all(|op| {
                op.origin == Origin::Source(site.span)
                    && op.block == g.definition(seed).unwrap().block
            })
            && g.position(g.definition(seed)?)? < g.position(root)?
            && g.position(root)? < g.position(begin)?
            && g.position(begin)? < g.position(first)?,
        "Int actual birth/Root/Shared/First sequence",
        (root, begin, first),
    )?;
    uses.permit(seed, root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), begin);
    uses.permit(EntityId::Loan(source), first);
    let via = |frame: [EntityId; 3], edge: &Edge| -> Result<[EntityId; 3], Failure> {
        need(
            edge.arguments.len() == 3
                && edge.arguments.iter().copied().collect::<BTreeSet<_>>()
                    == frame.into_iter().collect(),
            "Int complete current three-column transport",
            (frame, edge),
        )?;
        Ok([
            g.transported(edge, frame[0])?,
            g.transported(edge, frame[1])?,
            g.transported(edge, frame[2])?,
        ])
    };
    let initial = [seed, EntityId::Loan(source), EntityId::Value(cursor)];
    let header = via(initial, g.branch(first.block)?)?;
    need(
        g.branch(first.block)?.target == has.block
            && has.operation
                == Operation::MapIterationHasNext {
                    source: match header[1] {
                        EntityId::Loan(l) => l,
                        _ => unreachable!(),
                    },
                    cursor: match header[2] {
                        EntityId::Value(v) => v,
                        _ => unreachable!(),
                    },
                },
        "Int actual current header source/cursor",
        has,
    )?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(has.block)?.kind
    else {
        return Err(failure("Int actual HasNext conditional", has));
    };
    need(
        *condition == value(has)? && g.ops(has.block)?.len() == 1,
        "Int sole actual header guard",
        has,
    )?;
    let current = via(header, when_true)?;
    let exit = via(header, when_false)?;
    let EntityId::Loan(body_source) = current[1] else {
        unreachable!()
    };
    let EntityId::Value(body_cursor) = current[2] else {
        unreachable!()
    };
    let next = body::check(
        g,
        reader,
        body::Body {
            block: when_true.target,
            span: site.span,
            read: ast::span(p, site.read),
            arguments: site.arguments.map(|id| ast::span(p, id)),
            types: [kt, vt],
        },
        body_source,
        body_cursor,
        uses,
    )?;
    let back = g.branch(next.block)?;
    need(
        back.target == has.block && via([current[0], current[1], result(next)?], back)? == header,
        "Int exact current Next/source/owner backedge",
        back,
    )?;
    g.incoming_from(has.block, &[first.block, next.block])?;
    g.incoming_from(when_true.target, &[has.block])?;
    g.incoming_from(when_false.target, &[has.block])?;
    need(
        g.blocks.iter().copied().collect::<BTreeSet<_>>()
            == [first.block, has.block, next.block, when_false.target]
                .into_iter()
                .collect(),
        "Int complete actual provider CFG closure",
        &g.blocks,
    )?;
    for frame in [initial, header, current, exit] {
        for e in frame {
            uses.protect(e);
        }
        g.anchored(frame[0], &[seed])?;
        g.anchored(frame[1], &[EntityId::Loan(source)])?;
        g.anchored(frame[2], &[EntityId::Value(cursor), result(next)?])?;
        need(
            g.function.entity(frame[0]).unwrap().ty == EntityType::Value(map)
                && g.function.entity(frame[1]).unwrap().ty
                    == EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: map,
                    }
                && matches!(g.function.entity(frame[2]).unwrap().ty,EntityType::Value(ty)if module.type_kind(ty)==Some(&SsaTypeKind::Integer{bits:64,signed:false})),
            "Int every current owner/source/cursor column type",
            frame,
        )?;
    }
    for e in [header[1], header[2]] {
        uses.permit(e, has);
    }
    let end = one(
        g.ops(when_false.target)?.into_iter().filter(
            |op| matches!(op.operation,Operation::BorrowEnd{loan}if EntityId::Loan(loan)==exit[1]),
        ),
        "Int actual exhaustion EndSource",
    )?;
    let ops = g.ops(end.block)?;
    need(
        end.origin == Origin::Source(site.span)
            && drop.operation
                == Operation::Drop {
                    owner: match exit[0] {
                        EntityId::Value(v) => v,
                        _ => unreachable!(),
                    },
                }
            && drop.block == end.block
            && ops.first().map(|op| op.id) == Some(end.id)
            && ops.get(1).map(|op| op.id) == Some(drop.id)
            && ops.len() == 2
            && matches!(&g.term(end.block)?.kind,TerminatorKind::Return{values}if values.is_empty()),
        "Int current source End/current owner Drop/void Return exact terminal order",
        (end, drop),
    )?;
    uses.permit(exit[1], end);
    Ok(())
}
