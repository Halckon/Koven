//! Complete P24 method provider columns; the normal body reuses the existing K/V checker.
use super::super::super::query_map_normal_body;
use super::*;
pub(super) fn check(
    p: &ParsedFile,
    i: &Input,
    module: &Module,
    g: &Graph<'_>,
    sources: &BTreeSet<EntityId>,
    uses: &mut Uses,
) -> Result<(), Failure> {
    let site = i
        .site
        .as_ref()
        .ok_or_else(|| failure("P24 original method provider site", i))?;
    let first = at(g, site.span, |op| {
        matches!(op, Operation::MapIterationFirst { .. })
    })?;
    let has = at(g, site.span, |op| {
        matches!(op, Operation::MapIterationHasNext { .. })
    })?;
    let key = at(g, site.span, |op| {
        matches!(op, Operation::MapIterationKeyLoan { .. })
    })?;
    let next = at(g, site.span, |op| {
        matches!(op, Operation::MapIterationNext { .. })
    })?;
    let Operation::MapIterationFirst { source } = first.operation else {
        unreachable!()
    };
    let begin = g.definition(EntityId::Loan(source))?;
    let Operation::SharedReborrow { source: parent } = begin.operation else {
        return Err(failure("P24 source lease is a real reborrow", begin));
    };
    need(
        sources.contains(&EntityId::Loan(parent))
            && begin.origin == Origin::Source(site.span)
            && begin.block == first.block
            && g.position(begin)? < g.position(first)?,
        "P24 provider lease descends from actual method Map parameter",
        begin,
    )?;
    uses.permit(EntityId::Loan(parent), begin);
    let leases = family(g, &[EntityId::Loan(source)])?;
    let cursors = family(g, &[result(first)?, result(next)?])?;
    protect(uses, &leases);
    protect(uses, &cursors);
    let Operation::MapIterationHasNext {
        source: hs,
        cursor: hc,
    } = has.operation
    else {
        unreachable!()
    };
    let Operation::MapIterationKeyLoan {
        source: bs,
        cursor: bc,
    } = key.operation
    else {
        unreachable!()
    };
    let Operation::MapIterationNext {
        source: ns,
        cursor: nc,
    } = next.operation
    else {
        unreachable!()
    };
    let entered = g.branch(first.block)?;
    let back = g.branch(next.block)?;
    need(
        entered.target == has.block
            && back.target == has.block
            && next.block == key.block
            && g.argument(entered, EntityId::Loan(hs))? == EntityId::Loan(source)
            && g.argument(entered, EntityId::Value(hc))? == result(first)?
            && g.argument(back, EntityId::Loan(hs))? == EntityId::Loan(ns)
            && g.argument(back, EntityId::Value(hc))? == result(next)?
            && [hs, bs, ns]
                .iter()
                .all(|loan| leases.contains(&EntityId::Loan(*loan)))
            && [hc, bc, nc]
                .iter()
                .all(|cursor| cursors.contains(&EntityId::Value(*cursor))),
        "P24 actual provider preheader and backedge use current independent source/cursor columns",
        (entered, back),
    )?;
    g.incoming_from(has.block, &[first.block, next.block])?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(has.block)?.kind
    else {
        return Err(failure("P24 actual provider guard", has));
    };
    need(
        *condition == value(has)?
            && when_true.target == key.block
            && g.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc),
        "P24 true guard transports exact current source/cursor",
        has,
    )?;
    g.incoming_from(key.block, &[has.block])?;
    g.incoming_from(when_false.target, &[has.block])?;
    let exit = g.transported(when_false, EntityId::Loan(hs))?;
    let end=one(g.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::BorrowEnd{loan}if leases.contains(&EntityId::Loan(loan)))),"P24 sole provider source lease End")?;
    need(
        end.block == when_false.target
            && matches!(end.operation,Operation::BorrowEnd{loan}if EntityId::Loan(loan)==exit)
            && end.origin == Origin::Source(site.span)
            && g.ops(end.block)?.first().map(|op| op.id) == Some(end.id)
            && g.blocks.iter().copied().collect::<BTreeSet<_>>()
                == BTreeSet::from([first.block, has.block, key.block, end.block]),
        "P24 exhaustion ends only actual current provider lease in complete four-block CFG",
        end,
    )?;
    let (_, key_type, value_type) = module
        .map_container(
            g.function
                .entity(EntityId::Loan(source))
                .unwrap()
                .ty
                .semantic_type(),
        )
        .ok_or_else(|| failure("P24 actual provider Map type", source))?;
    let checked = query_map_normal_body::check(
        g,
        function(module, i.reader)?,
        query_map_normal_body::Body {
            block: key.block,
            span: site.span,
            read: ast::span(p, site.read),
            arguments: [
                ast::span(p, site.arguments[0]),
                ast::span(p, site.arguments[1]),
            ],
            types: [key_type, value_type],
        },
        bs,
        bc,
        uses,
    )?;
    need(
        checked.id == next.id,
        "P24 original normal body ends both components before this exact Next",
        checked,
    )?;
    for op in [first, has, end] {
        for entity in op.operation.entities() {
            uses.permit(entity, op);
        }
    }
    let actual_maps = g
        .function
        .loans
        .iter()
        .enumerate()
        .filter_map(|(index, data)| {
            module
                .map_container(data.ty.semantic_type())
                .is_some()
                .then_some(EntityId::Loan(LoanId {
                    function: g.function.id,
                    index,
                }))
        })
        .collect::<BTreeSet<_>>();
    need(
        actual_maps == sources.union(&leases).copied().collect(),
        "P24 all actual method Map loans belong to original parameter or its provider lease",
        actual_maps,
    )
}
