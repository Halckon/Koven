//! The If-join owner remains the current owner in every complete provider column.
use super::super::super::super::{query_graph::loan, query_map_normal_body as body};
use super::*;
pub(super) struct Delivery {
    pub owner: EntityId,
    pub block: BlockId,
    pub blocks: [BlockId; 3],
}
fn via(
    g: &Graph<'_>,
    edge: &Edge,
    roles: [EntityId; 3],
    flag: EntityId,
) -> Result<[EntityId; 3], Failure> {
    for &arg in &edge.arguments {
        if !roles.contains(&arg) {
            g.anchored(arg, &[flag])?;
        }
    }
    Ok([
        g.transported(edge, roles[0])?,
        g.transported(edge, roles[1])?,
        g.transported(edge, roles[2])?,
    ])
}
pub(super) fn check(
    g: &Graph<'_>,
    module: &Module,
    p: &ParsedFile,
    i: &Input,
    input: (EntityId, EntityId),
    uses: &mut Uses,
) -> Result<Delivery, Failure> {
    let (owner, flag) = input;
    let site = super::super::ast::site(p, i)?;
    let EntityId::Value(seed) = owner else {
        return Err(failure("If actual joined provider owner Value", owner));
    };
    let map = g.function.entity(owner).unwrap().ty.semantic_type();
    let (_, key, val) = module
        .map_container(map)
        .ok_or_else(|| failure("If provider actual Map storage", map))?;
    let at = |predicate: fn(&Operation) -> bool| {
        one(
            g.instructions
                .iter()
                .copied()
                .filter(|op| op.origin == Origin::Source(site.span) && predicate(&op.operation)),
            "If exact original provider operation",
        )
    };
    let root = at(|op| matches!(op, Operation::RootPlace { .. }))?;
    let begin = at(|op| matches!(op, Operation::BorrowBegin { .. }))?;
    let first = at(|op| matches!(op, Operation::MapIterationFirst { .. }))?;
    let has = at(|op| matches!(op, Operation::MapIterationHasNext { .. }))?;
    let next = at(|op| matches!(op, Operation::MapIterationNext { .. }))?;
    let EntityId::Place(place) = result(root)? else {
        return Err(failure("If provider actual RootPlace", root));
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
            && matches!(g.function.entity(owner).unwrap().definition,Definition::BlockParameter{block,index:0}if block==first.block)
            && g.position(root)? < g.position(begin)?
            && g.position(begin)? < g.position(first)?,
        "If join0 current owner roots its real provider lease before First",
        (root, begin, first),
    )?;
    uses.permit(owner, root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), begin);
    let initial = [owner, EntityId::Loan(lease), result(first)?];
    let entry = g.branch(first.block)?;
    let header = via(g, entry, initial, flag)?;
    let [EntityId::Value(_), EntityId::Loan(hs), EntityId::Value(hc)] = header else {
        return Err(failure(
            "If actual header owner/source/cursor roles",
            header,
        ));
    };
    need(
        has.block == entry.target
            && g.ops(has.block)?.len() == 1
            && has.operation
                == Operation::MapIterationHasNext {
                    source: hs,
                    cursor: hc,
                },
        "If HasNext reads actual current header source/cursor",
        has,
    )?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(has.block)?.kind
    else {
        return Err(failure("If actual provider HasNext conditional", has));
    };
    need(
        *condition == value(has)?
            && g.term(has.block)?.origin == Origin::Source(site.span)
            && !g.instructions.iter().any(|op| {
                op.operation
                    .entities()
                    .contains(&EntityId::Value(*condition))
            })
            && g.blocks.iter().all(|&block| {
                block == has.block
                    || !g
                        .term(block)
                        .unwrap()
                        .kind
                        .entities()
                        .contains(&EntityId::Value(*condition))
            })
            && !when_true.arguments.contains(&EntityId::Value(*condition))
            && !when_false.arguments.contains(&EntityId::Value(*condition)),
        "If actual provider HasNext has only its real guard use",
        has,
    )?;
    let current = via(g, when_true, header, flag)?;
    let exhausted = via(g, when_false, header, flag)?;
    let [_, EntityId::Loan(bs), EntityId::Value(bc)] = current else {
        return Err(failure("If actual body role types", current));
    };
    let checked = body::check(
        g,
        function(module, i.reader)?,
        body::Body {
            block: when_true.target,
            span: site.span,
            read: span(p, site.read),
            arguments: site.arguments.map(|e| span(p, e)),
            types: [key, val],
        },
        bs,
        bc,
        uses,
    )?;
    need(
        checked.id == next.id
            && next.operation
                == Operation::MapIterationNext {
                    source: bs,
                    cursor: bc,
                },
        "If exact current body Next after reader/reverse component Ends",
        next,
    )?;
    let back = g.branch(next.block)?;
    need(
        back.target == has.block
            && via(g, back, [current[0], current[1], result(next)?], flag)? == header,
        "If complete current owner/source/next-cursor backedge",
        back,
    )?;
    g.incoming_from(has.block, &[first.block, next.block])?;
    g.incoming_from(next.block, &[has.block])?;
    g.incoming_from(when_false.target, &[has.block])?;
    let [EntityId::Value(_), EntityId::Loan(es), EntityId::Value(_)] = exhausted else {
        return Err(failure(
            "If actual exhausted owner/source/cursor roles",
            exhausted,
        ));
    };
    let end = one(
        g.instructions
            .iter()
            .copied()
            .filter(|op| op.operation == Operation::BorrowEnd { loan: es }),
        "If unique actual exhausted provider EndSource",
    )?;
    need(
        end.block == when_false.target
            && end.origin == Origin::Source(site.span)
            && g.ops(end.block)?.first().map(|op| op.id) == Some(end.id),
        "If provider lease ends first before current owner delivery or Drop",
        end,
    )?;
    for roles in [initial, header, current, exhausted] {
        g.anchored(roles[0], &[owner])?;
        g.anchored(roles[1], &[EntityId::Loan(lease)])?;
        g.anchored(roles[2], &[result(first)?, result(next)?])?;
        need(
            g.function.entity(roles[0]).unwrap().ty == EntityType::Value(map)
                && g.function.entity(roles[1]).unwrap().ty
                    == EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: map,
                    }
                && matches!(g.function.entity(roles[2]).unwrap().ty,EntityType::Value(t)if module.type_kind(t)==Some(&SsaTypeKind::Integer{bits:64,signed:false})),
            "If all owner/source/cursor roles preserve concrete ABI",
            roles,
        )?;
        for entity in roles {
            uses.protect(entity);
        }
    }
    for op in [first, has, end] {
        for entity in op.operation.entities() {
            uses.permit(entity, op);
        }
    }
    let role_set = [initial, header, current, exhausted]
        .into_iter()
        .flatten()
        .collect::<BTreeSet<_>>();
    for block in [first.block, has.block, next.block, when_false.target] {
        for &parameter in &g.block(block)?.parameters {
            if !role_set.contains(&parameter) {
                g.anchored(parameter, &[flag])?;
                uses.protect(parameter);
            }
        }
    }
    for op in &g.instructions {
        need(
            !matches!(op.operation,Operation::BorrowEnd{loan}if [initial[1],header[1],current[1]].contains(&EntityId::Loan(loan))),
            "If provider never ends a stale initial/header/body lease",
            op,
        )?;
    }
    Ok(Delivery {
        owner: exhausted[0],
        block: when_false.target,
        blocks: [has.block, next.block, when_false.target],
    })
}
