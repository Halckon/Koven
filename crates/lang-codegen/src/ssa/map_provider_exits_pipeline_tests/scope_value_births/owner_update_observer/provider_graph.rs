//! Complete actual provider plus the original conditional update and later source delivery.
mod conditional;
mod provider;
use super::super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, loan, result, value},
    query_inputs::one,
    query_map_normal_body as body,
};
use super::{
    Case,
    ast::{Input, span},
    facts::Proof,
};
use crate::ssa::model::*;
use lang_frontend::{parser::ParsedFile, source::Span};
use std::collections::BTreeSet;
pub(super) fn check(
    c: &Case,
    p: &ParsedFile,
    i: &Input,
    proof: &Proof,
    program: &Program,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "update For actual module")?;
    let scan = function(module, i.scan)?;
    let g = Graph::new(scan)?;
    println!(
        "SCOPE C UPDATE PROVIDER SSA label={} function={scan:#?} edges={:#?}",
        c.label, g.edges
    );
    let mut uses = Uses::default();
    let mut calls = Vec::new();
    let mut anchors = Vec::new();
    for b in &i.births {
        let factory = function(module, b.factory)?;
        let call = one(
            g.instructions.iter().copied().filter(|op| {
                op.origin == Origin::Source(span(p, b.expression))
                    && op.operation
                        == Operation::DirectCall {
                            callee: factory.id,
                            receiver: None,
                            arguments: vec![],
                        }
            }),
            "update For exact selected epoch Call",
        )?;
        let owner = result(call)?;
        need(
            factory.return_types == [scan.entity(owner).unwrap().ty.semantic_type()]
                && matches!(owner, EntityId::Value(_)),
            "update For actual owned Call ABI",
            call,
        )?;
        calls.push(call);
        anchors.push(owner);
    }
    let site = super::ast::site(p, i)?;
    let provider_root = one(
        g.instructions.iter().copied().filter(|op| {
            op.origin == Origin::Source(site.span)
                && matches!(op.operation, Operation::RootPlace { .. })
        }),
        "update actual provider RootPlace",
    )?;
    let Operation::RootPlace { owner: seed } = provider_root.operation else {
        unreachable!()
    };
    let seed = EntityId::Value(seed);
    let put = if let Some(expr) = i.put {
        super::graph::put_clones(&g, p, expr)?;
        Some(one(
            g.instructions.iter().copied().filter(|op| {
                op.origin == Origin::Source(span(p, expr))
                    && matches!(op.operation, Operation::MapPut { .. })
            }),
            "update original conditional put SSA",
        )?)
    } else {
        None
    };
    if let Some(put) = put {
        anchors.push(result(put)?);
    }
    let before = if matches!(c.code, 7 | 9) {
        Some(conditional::check(
            &g,
            p,
            i,
            (
                anchors[0],
                if c.code == 9 {
                    result(put.unwrap())?
                } else {
                    anchors[1]
                },
            ),
            proof.old_origin,
            &mut uses,
        )?)
    } else {
        None
    };
    let expected = if let Some(before) = &before {
        before.owner
    } else if c.code == 8 {
        anchors[0]
    } else {
        anchors[1]
    };
    need(
        seed == expected,
        "update provider borrows exact current source epoch rather than another Map",
        (seed, expected),
    )?;
    if i.flag.is_some() && !proof.flag_in_provider {
        let prior = before
            .as_ref()
            .ok_or_else(|| failure("update exact entry-only prior If flag proof", i))?;
        need(
            prior.blocks[0] == scan.entry_block().unwrap() && prior.block == provider_root.block,
            "update entry flag If dominates the provider through both complete original arms",
            prior.block,
        )?;
    }
    let extra = if c.code == 11 { Some(anchors[0]) } else { None };
    let carried = provider::check(
        &g,
        module,
        p,
        i,
        (seed, extra, proof.flag_in_provider),
        &mut uses,
    )?;
    let after = if c.code == 8 {
        Some(conditional::check(
            &g,
            p,
            i,
            (carried.owner, anchors[1]),
            proof.old_origin,
            &mut uses,
        )?)
    } else {
        None
    };
    let final_owner = if let Some(after) = &after {
        after.owner
    } else if c.code == 11 {
        carried
            .extra
            .ok_or_else(|| failure("update independently carried future next owner", &carried))?
    } else {
        carried.owner
    };
    let final_block = after.as_ref().map_or(carried.block, |q| q.block);
    let observer = function(module, i.observer)?;
    let (call, arg) = *one(i.observes.iter(), "update For exact final observe AST")?;
    let observed = one(
        g.instructions.iter().copied().filter(|op| {
            op.origin == Origin::Source(span(p, call))
                && matches!(op.operation,Operation::DirectCall{callee,..}if callee==observer.id)
        }),
        "update For final actual observe Call",
    )?;
    let Operation::DirectCall {
        receiver: None,
        arguments,
        ..
    } = &observed.operation
    else {
        return Err(failure("update For actual observe ABI", observed));
    };
    let [EntityId::Loan(observe_loan)] = arguments.as_slice() else {
        return Err(failure(
            "update For actual current Map observe loan",
            observed,
        ));
    };
    let begin = g.definition(EntityId::Loan(*observe_loan))?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = begin.operation
    else {
        return Err(failure("update For actual final Shared loan birth", begin));
    };
    let root = g.definition(EntityId::Place(place))?;
    let EntityId::Value(final_value) = final_owner else {
        return Err(failure("update For final owner Value", final_owner));
    };
    need(
        root.operation == Operation::RootPlace { owner: final_value }
            && [root, begin, observed]
                .iter()
                .all(|op| op.block == final_block)
            && root.origin == Origin::Source(span(p, arg))
            && begin.origin == Origin::Source(span(p, arg))
            && g.position(root)? < g.position(begin)?
            && g.position(begin)? < g.position(observed)?,
        "update final observe borrows exact post-provider/assignment current owner",
        (root, begin, observed),
    )?;
    let end = one(
        g.instructions.iter().copied().filter(|op| {
            op.operation
                == Operation::BorrowEnd {
                    loan: *observe_loan,
                }
        }),
        "update exact final observe End",
    )?;
    need(
        end.block == final_block
            && end.origin == Origin::Source(span(p, call))
            && g.position(end)? == g.position(observed)? + 1,
        "update final actual CallReturn loan cleanup",
        end,
    )?;
    uses.permit(final_owner, root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), begin);
    uses.protect(EntityId::Loan(*observe_loan));
    uses.permit(EntityId::Loan(*observe_loan), observed);
    uses.permit(EntityId::Loan(*observe_loan), end);
    let final_drop = one(
        g.instructions
            .iter()
            .copied()
            .filter(|op| op.operation == Operation::Drop { owner: final_value }),
        "update exact unique final current owner Drop",
    )?;
    need(
        final_drop.block == final_block
            && final_drop.origin == Origin::Source(proof.final_origin)
            && g.position(final_drop)? == g.position(end)? + 1
            && g.ops(final_block)?.last().map(|op| op.id) == Some(final_drop.id)
            && matches!(&g.term(final_block)?.kind,TerminatorKind::Return{values}if values.is_empty()),
        "update exact public final owner Drop after loan End before normal Return",
        final_drop,
    )?;
    uses.permit(final_owner, final_drop);
    let old_drop = if let Some(q) = before.as_ref().or(after.as_ref()) {
        q.old_drop
    } else if c.code == 10 {
        let EntityId::Value(old) = anchors[0] else {
            unreachable!()
        };
        let drop = one(
            g.instructions
                .iter()
                .copied()
                .filter(|op| op.operation == Operation::Drop { owner: old }),
            "update moved old declaration epoch unique Drop",
        )?;
        need(
            drop.origin == Origin::Source(proof.old_origin.unwrap())
                && drop.block == calls[0].block
                && drop.block == calls[1].block
                && g.position(calls[0])? < g.position(drop)?
                && g.position(drop)? < g.position(calls[1])?,
            "update P10 old declaration AfterStatement Drop precedes the second make",
            drop,
        )?;
        uses.permit(anchors[0], drop);
        Some(drop)
    } else if c.code == 11 {
        let EntityId::Value(old) = carried.owner else {
            unreachable!()
        };
        let drop = one(
            g.instructions
                .iter()
                .copied()
                .filter(|op| op.operation == Operation::Drop { owner: old }),
            "update actual old source LoopExit Drop",
        )?;
        need(
            drop.block == carried.block
                && drop.origin == Origin::Source(proof.old_origin.unwrap())
                && g.position(drop)? == g.position(carried.end)? + 1
                && g.position(drop)? < g.position(root)?,
            "update P11 old source End/Drop precedes exact future Name owner observe",
            drop,
        )?;
        uses.permit(carried.owner, drop);
        Some(drop)
    } else {
        None
    };
    let map_drops=g.instructions.iter().filter(|op|matches!(op.operation,Operation::Drop{owner}if module.map_container(scan.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())).collect::<Vec<_>>();
    need(
        map_drops.len() == 1 + usize::from(old_drop.is_some())
            && map_drops
                .iter()
                .all(|op| op.id == final_drop.id || old_drop.is_some_and(|old| old.id == op.id)),
        "update no extra or moved-name duplicate Map Drop sites",
        &map_drops,
    )?;
    if let Some(put) = put {
        let Operation::MapPut { owner, .. } = put.operation else {
            unreachable!()
        };
        uses.permit(EntityId::Value(owner), put);
    }
    let mut blocks = BTreeSet::from([scan.entry_block().unwrap()]);
    blocks.extend(carried.blocks);
    if let Some(q) = before.as_ref().or(after.as_ref()) {
        blocks.extend(q.blocks);
    }
    need(
        blocks == g.blocks.iter().copied().collect(),
        "update complete original conditional/provider CFG, no hidden path",
        &g.blocks,
    )?;
    for &block in &g.blocks {
        for entity in g.block(block)?.parameters.iter().copied().chain(
            g.ops(block)?
                .into_iter()
                .flat_map(|op| op.results.iter().copied()),
        ) {
            if matches!(scan.entity(entity).unwrap().ty,EntityType::Value(t)if module.map_container(t).is_some())
            {
                g.anchored(entity, &anchors)?;
                uses.protect(entity);
            }
        }
    }
    uses.check(&g, None)
}
