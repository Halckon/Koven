//! Complete provider columns and normal cleanup, inside each original enclosing CFG.
use super::*;
pub(super) fn check(
    c: &Case,
    parsed: &ParsedFile,
    i: &ast::Input,
    module: &Module,
    g: &Graph<'_>,
    input: (&BTreeSet<EntityId>, &ast::Site),
    uses: &mut Uses,
) -> Result<BTreeSet<EntityId>, Failure> {
    let (originals, site) = input;
    let first = at(g, site.span, |o| {
        matches!(o, Operation::MapIterationFirst { .. })
    })?;
    let has = at(g, site.span, |o| {
        matches!(o, Operation::MapIterationHasNext { .. })
    })?;
    let key = at(g, site.span, |o| {
        matches!(o, Operation::MapIterationKeyLoan { .. })
    })?;
    let val = at(g, site.span, |o| {
        matches!(o, Operation::MapIterationValueLoan { .. })
    })?;
    let next = at(g, site.span, |o| {
        matches!(o, Operation::MapIterationNext { .. })
    })?;
    let Operation::MapIterationFirst { source: seed } = first.operation else {
        unreachable!()
    };
    let birth = g.definition(EntityId::Loan(seed))?;
    let Operation::SharedReborrow { source: parent } = birth.operation else {
        return Err(failure(
            "scope actual provider borrows original loan",
            birth,
        ));
    };
    need(
        originals.contains(&EntityId::Loan(parent))
            && birth.origin == Origin::Source(site.span)
            && birth.block == first.block,
        "scope current provider lease original source identity",
        birth,
    )?;
    before(g, birth, first)?;
    uses.permit(EntityId::Loan(parent), birth);
    let leases = family(g, &[EntityId::Loan(seed)])?;
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
    need(
        val.operation
            == Operation::MapIterationValueLoan {
                source: bs,
                cursor: bc,
            }
            && key.block == val.block
            && [seed, hs, bs, ns]
                .iter()
                .all(|l| leases.contains(&EntityId::Loan(*l)))
            && [hc, bc, nc]
                .iter()
                .all(|v| cursors.contains(&EntityId::Value(*v))),
        "scope every provider operation reads actual current source and cursor",
        (first, has, key, val, next),
    )?;
    let incoming = g.branch(first.block)?;
    let back = g.branch(next.block)?;
    need(
        incoming.target == has.block
            && back.target == has.block
            && g.argument(incoming, EntityId::Loan(hs))? == EntityId::Loan(seed)
            && g.argument(incoming, EntityId::Value(hc))? == result(first)?
            && g.argument(back, EntityId::Loan(hs))? == EntityId::Loan(ns)
            && g.argument(back, EntityId::Value(hc))? == result(next)?,
        "scope complete original entry and current Next/source backedge columns",
        (incoming, back),
    )?;
    g.incoming_from(has.block, &[first.block, next.block])?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(has.block)?.kind
    else {
        return Err(failure("scope actual provider guard", has));
    };
    need(
        *condition == value(has)?
            && when_true.target == key.block
            && g.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc),
        "scope guard true preserves exact current source/cursor",
        has,
    )?;
    g.incoming_from(key.block, &[has.block])?;
    g.incoming_from(when_false.target, &[has.block])?;
    let exit = g.transported(when_false, EntityId::Loan(hs))?;
    let end=one(g.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::BorrowEnd{loan}if leases.contains(&EntityId::Loan(loan)))),"scope sole actual current source End")?;
    need(
        end.block == when_false.target
            && end.operation
                == Operation::BorrowEnd {
                    loan: match exit {
                        EntityId::Loan(l) => l,
                        _ => return Err(failure("scope exhaustion source loan", exit)),
                    },
                }
            && end.origin == Origin::Source(site.span)
            && g.ops(end.block)?.first().map(|op| op.id) == Some(end.id),
        "scope exhaustion alone begins exact current EndSource",
        end,
    )?;
    for op in [first, has, key, val, next, end] {
        for e in op.operation.entities() {
            if leases.contains(&e) || cursors.contains(&e) {
                uses.permit(e, op);
            }
        }
    }
    need(!g.instructions.iter().any(|op|matches!(op.operation,Operation::BorrowEnd{loan}if originals.contains(&EntityId::Loan(loan))))&&g.instructions.iter().filter(|op|matches!(op.operation,Operation::MapIterationFirst{..})).count()==1,"scope lease closes independently while original parameter remains caller-owned",g.function.id)?;
    let (_, kt, vt) = module
        .map_container(
            g.function
                .entity(EntityId::Loan(seed))
                .unwrap()
                .ty
                .semantic_type(),
        )
        .unwrap();
    for (root, ty) in [(loan(key)?, kt), (loan(val)?, vt)] {
        need(
            g.function.entity(EntityId::Loan(root)).unwrap().ty
                == EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                },
            "scope actual K/V Shared capability",
            root,
        )?;
    }
    if !matches!(c.gate, 17 | 18) {
        let read = i.read.as_ref().unwrap();
        let checked = super::super::super::query_map_normal_body::check(
            g,
            function(module, i.reader.as_ref().unwrap().span)?,
            super::super::super::query_map_normal_body::Body {
                block: key.block,
                span: site.span,
                read: ast::span(parsed, read.id),
                arguments: [
                    ast::span(parsed, read.arguments[0]),
                    ast::span(parsed, read.arguments[1]),
                ],
                types: [kt, vt],
            },
            bs,
            bc,
            uses,
        )?;
        need(
            checked.id == next.id,
            "scope original normal body closes same actual Next",
            checked,
        )?;
    }
    Ok(leases)
}
