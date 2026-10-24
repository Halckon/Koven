//! Actual alias lease, provider lease and their independent normal endpoints.
mod callers;
use super::super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, loan, result, value},
    query_inputs::one,
    query_map_normal_body as body,
};
use super::{Case, Input, facts, span};
use crate::ssa::model::*;
use lang_frontend::{parser::ParsedFile, source::SourceMap};
use std::collections::BTreeSet;
fn family(g: &Graph<'_>, anchors: &[EntityId]) -> Result<BTreeSet<EntityId>, Failure> {
    let mut all = anchors.iter().copied().collect::<BTreeSet<_>>();
    loop {
        let n = all.len();
        for (_, edge) in &g.edges {
            for (&a, &p) in edge.arguments.iter().zip(&g.block(edge.target)?.parameters) {
                if all.contains(&a) {
                    all.insert(p);
                }
            }
        }
        if n == all.len() {
            break;
        }
    }
    for &e in &all {
        g.anchored(e, anchors)?;
    }
    Ok(all)
}
fn protect(uses: &mut Uses, set: &BTreeSet<EntityId>) {
    for &e in set {
        uses.protect(e);
    }
}
pub(super) fn check(
    c: &Case,
    sources: &SourceMap,
    p: &ParsedFile,
    i: &Input,
    proof: &facts::Proof,
    program: &Program,
) -> Result<(), Failure> {
    callers::check(sources, p, i, program)?;
    let module = one(program.modules.iter(), "alias actual original module")?;
    let scan = function(module, i.scan)?;
    let g = Graph::new(scan)?;
    let parameters = &g.block(scan.entry_block().unwrap())?.parameters;
    need(
        parameters.len() == 2
            && scan.return_types.is_empty()
            && scan.receiver.is_none()
            && scan.borrow_return.is_none(),
        "alias actual original scanner ABI",
        parameters,
    )?;
    let original = parameters[0];
    let ty = scan.entity(original).unwrap().ty;
    let map = ty.semantic_type();
    let (_, key, val) = module
        .map_container(map)
        .ok_or_else(|| failure("alias original Map parameter storage", map))?;
    need(
        ty == EntityType::Loan {
            kind: LoanKind::Shared,
            target: map,
        } && module.map_container(map) == Some((MapContainerKind::Map, key, val))
            && [key, val]
                .iter()
                .all(|t| module.type_kind(*t) == Some(&SsaTypeKind::StringOwner)),
        "alias actual original Shared Map String/String parameter",
        ty,
    )?;
    let originals = family(&g, &[original])?;
    let flag = family(&g, &[parameters[1]])?;
    let begin=one(g.instructions.iter().copied().filter(|op|op.origin==Origin::Source(i.declaration)&&matches!(op.operation,Operation::SharedReborrow{source}if originals.contains(&EntityId::Loan(source)))),"alias real source parameter creates actual independent alias lease")?;
    let alias = result(begin)?;
    let aliases = family(&g, &[alias])?;
    let alias_end=one(g.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::BorrowEnd{loan}if aliases.contains(&EntityId::Loan(loan)))),"alias unique actual current parent alias End")?;
    need(
        proof.declaration == i.declaration
            && proof.point == i.endpoint
            && (!c.provider || proof.point == i.body)
            && proof.endpoint == p.ast().statements().get(i.endpoint).unwrap().span()
            && begin.block == scan.entry_block().unwrap()
            && !originals.contains(&alias),
        "alias actual declaration identity and published actual statement endpoint",
        (begin, alias_end, proof),
    )?;
    if !c.provider {
        let children = super::control(p, i, program)?;
        need(
            alias_end.origin == Origin::Source(proof.declaration)
                && g.position(alias_end)? + 1 == g.ops(alias_end.block)?.len()
                && matches!(&g.term(alias_end.block)?.kind,TerminatorKind::Return{values}if values.is_empty()),
            "alias control published observe-statement lease cleanup is last before ordinary Return",
            alias_end,
        )?;
        let expected = originals
            .iter()
            .chain(&aliases)
            .chain(&children)
            .copied()
            .collect();
        crate::ssa::map_provider_exits_pipeline_tests::query_inventory::check(
            &g,
            &[map],
            &expected,
        )?;
        return Ok(());
    }
    let site = facts::site(p, i)?;
    let mut uses = Uses::default();
    for set in [&originals, &aliases, &flag] {
        protect(&mut uses, set);
    }
    for e in begin.operation.entities() {
        uses.permit(e, begin);
    }
    let lease_begin=one(g.instructions.iter().copied().filter(|op|op.origin==Origin::Source(site.span)&&matches!(op.operation,Operation::SharedReborrow{source}if aliases.contains(&EntityId::Loan(source)))),"alias actual independent provider reborrow of this alias")?;
    let lease = result(lease_begin)?;
    let leases = family(&g, &[lease])?;
    protect(&mut uses, &leases);
    for e in lease_begin.operation.entities() {
        uses.permit(e, lease_begin);
    }
    let first = one(
        g.instructions.iter().copied().filter(|op| {
            op.origin == Origin::Source(site.span)
                && matches!(op.operation, Operation::MapIterationFirst { .. })
        }),
        "alias original First",
    )?;
    let has = one(
        g.instructions.iter().copied().filter(|op| {
            op.origin == Origin::Source(site.span)
                && matches!(op.operation, Operation::MapIterationHasNext { .. })
        }),
        "alias original HasNext",
    )?;
    let next = one(
        g.instructions.iter().copied().filter(|op| {
            op.origin == Origin::Source(site.span)
                && matches!(op.operation, Operation::MapIterationNext { .. })
        }),
        "alias original Next",
    )?;
    need(
        first.operation
            == Operation::MapIterationFirst {
                source: loan(lease_begin)?,
            }
            && first.block == begin.block
            && lease_begin.block == first.block
            && g.position(begin)? < g.position(lease_begin)?
            && g.position(lease_begin)? < g.position(first)?,
        "alias birth precedes provider lease and First in actual source order",
        (begin, lease_begin, first),
    )?;
    let cursors = family(&g, &[result(first)?, result(next)?])?;
    protect(&mut uses, &cursors);
    let Operation::MapIterationHasNext {
        source: hs,
        cursor: hc,
    } = has.operation
    else {
        unreachable!()
    };
    let pre = g.branch(first.block)?;
    let back = g.branch(next.block)?;
    need(
        pre.target == has.block
            && back.target == has.block
            && g.argument(pre, EntityId::Loan(hs))? == lease
            && g.argument(pre, EntityId::Value(hc))? == result(first)?,
        "alias exact original entry source/cursor columns",
        pre,
    )?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(has.block)?.kind
    else {
        return Err(failure("alias actual HasNext conditional", has));
    };
    need(
        *condition == value(has)? && g.ops(has.block)?.len() == 1,
        "alias sole actual header guard",
        has,
    )?;
    let EntityId::Loan(bs) = g.transported(when_true, EntityId::Loan(hs))? else {
        unreachable!()
    };
    let EntityId::Value(bc) = g.transported(when_true, EntityId::Value(hc))? else {
        unreachable!()
    };
    let checked = body::check(
        &g,
        function(module, i.reader)?,
        body::Body {
            block: when_true.target,
            span: site.span,
            read: span(p, site.read),
            arguments: site.arguments.map(|id| span(p, id)),
            types: [key, val],
        },
        bs,
        bc,
        &mut uses,
    )?;
    need(
        checked.id == next.id
            && next.operation
                == Operation::MapIterationNext {
                    source: bs,
                    cursor: bc,
                }
            && g.argument(back, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.argument(back, EntityId::Value(hc))? == result(next)?,
        "alias exact actual current Next/provider source backedge",
        back,
    )?;
    g.incoming_from(has.block, &[first.block, next.block])?;
    g.incoming_from(when_true.target, &[has.block])?;
    g.incoming_from(when_false.target, &[has.block])?;
    for op in [first, has] {
        for e in op.operation.entities() {
            uses.permit(e, op);
        }
    }
    let header_alias = one(
        g.block(has.block)?
            .parameters
            .iter()
            .copied()
            .filter(|e| aliases.contains(e)),
        "alias exact current header alias column",
    )?;
    let EntityId::Loan(exit_alias) = g.transported(when_false, header_alias)? else {
        return Err(failure(
            "alias actual exhausted alias Loan column",
            header_alias,
        ));
    };
    need(
        alias_end.operation == Operation::BorrowEnd { loan: exit_alias }
            && alias_end.block == when_false.target
            && alias_end.origin == Origin::Source(site.span),
        "alias end uses the actual exhausted alias column at the original For origin",
        alias_end,
    )?;
    let exit = g.transported(when_false, EntityId::Loan(hs))?;
    let provider_end=one(g.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::BorrowEnd{loan}if leases.contains(&EntityId::Loan(loan)))),"alias unique actual current provider EndSource")?;
    need(
        provider_end.operation
            == Operation::BorrowEnd {
                loan: match exit {
                    EntityId::Loan(l) => l,
                    _ => unreachable!(),
                },
            }
            && provider_end.block == when_false.target
            && provider_end.origin == Origin::Source(site.span)
            && g.ops(provider_end.block)?.first().map(|op| op.id) == Some(provider_end.id),
        "alias exhaustion starts by ending only actual provider lease",
        provider_end,
    )?;
    uses.permit(exit, provider_end);
    for e in alias_end.operation.entities() {
        uses.permit(e, alias_end);
    }
    // Only the actual unconditional ordinary scope tail may end the alias after
    // its provider has finished. The frontend point is the real scan Block ID.
    let mut tail = Vec::new();
    let mut block = when_false.target;
    let mut ended_provider = false;
    let mut ended_alias = false;
    loop {
        need(
            !tail.contains(&block),
            "alias ordinary body-end tail is acyclic",
            block,
        )?;
        tail.push(block);
        for op in g.ops(block)? {
            if op.id == provider_end.id {
                need(
                    !ended_provider && !ended_alias,
                    "alias provider ends once before parent",
                    op,
                )?;
                ended_provider = true;
            } else if op.id == alias_end.id {
                need(
                    ended_provider && !ended_alias,
                    "alias published scope end follows actual provider EndSource",
                    op,
                )?;
                ended_alias = true;
            } else {
                return Err(failure(
                    "alias ordinary scope tail has only provider/alias cleanup",
                    op,
                ));
            }
        }
        match &g.term(block)?.kind {
            TerminatorKind::Branch(edge) => {
                need(
                    !edge
                        .arguments
                        .iter()
                        .any(|e| leases.contains(e) || ended_alias && aliases.contains(e)),
                    "alias ended loan cannot escape through a scope-tail edge",
                    edge,
                )?;
                g.incoming_from(edge.target, &[block])?;
                block = edge.target;
            }
            TerminatorKind::Return { values } => {
                need(
                    ended_provider && ended_alias && values.is_empty(),
                    "alias published ordinary scope end precedes exact void Return",
                    g.term(block)?,
                )?;
                break;
            }
            other => {
                return Err(failure(
                    "alias actual ordinary body-end tail terminal",
                    other,
                ));
            }
        }
    }
    let expected = [first.block, has.block, next.block]
        .into_iter()
        .chain(tail)
        .collect::<BTreeSet<_>>();
    need(
        g.blocks.iter().copied().collect::<BTreeSet<_>>() == expected,
        "alias complete actual provider and scope CFG",
        &g.blocks,
    )?;
    for &block in &g.blocks {
        for &parameter in &g.block(block)?.parameters {
            need(
                [&originals, &flag, &aliases, &leases, &cursors]
                    .iter()
                    .any(|set| set.contains(&parameter)),
                "alias every full incoming column has actual parameter/alias/provider/cursor lineage",
                parameter,
            )?;
        }
    }
    for (_, edge) in &g.edges {
        for &arg in &edge.arguments {
            need(
                [&originals, &flag, &aliases, &leases, &cursors]
                    .iter()
                    .any(|set| set.contains(&arg)),
                "alias every actual edge operand belongs to a fully anchored original column",
                (edge, arg),
            )?;
        }
    }
    for set in [&aliases, &leases] {
        for &e in set {
            need(
                scan.entity(e).unwrap().ty == ty,
                "alias every carried Map capability preserves original Shared type",
                e,
            )?;
        }
    }
    for &e in &cursors {
        need(
            matches!(scan.entity(e).unwrap().ty,EntityType::Value(t)if module.type_kind(t)==Some(&SsaTypeKind::Integer{bits:64,signed:false})),
            "alias actual current u64 cursor type",
            e,
        )?;
    }
    need(!g.instructions.iter().any(|op|matches!(op.operation,Operation::BorrowEnd{loan}if originals.contains(&EntityId::Loan(loan))))&&!scan.values.iter().any(|v|module.map_container(v.ty.semantic_type()).is_some())&&!g.instructions.iter().any(|op|matches!(op.operation,Operation::MapConstruct{..}|Operation::MapConsume{..}|Operation::MapPut{..})),"alias caller-owned Map is never recreated/consumed/dropped or ended by scanner",scan.id)?;
    let expected = originals
        .iter()
        .chain(&aliases)
        .chain(&leases)
        .copied()
        .collect();
    crate::ssa::map_provider_exits_pipeline_tests::query_inventory::check(&g, &[map], &expected)?;
    uses.check(&g, None)
}
