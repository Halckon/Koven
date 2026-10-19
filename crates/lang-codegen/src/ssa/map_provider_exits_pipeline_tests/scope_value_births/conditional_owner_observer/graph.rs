//! Actual Unit conditional deliveries and ordinary control cleanup, never a fabricated If Call.
mod provider;
use super::super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, result, value},
    query_inputs::one,
};
use super::{
    Case,
    ast::{Input, span},
    facts::Proof,
};
use crate::ssa::model::*;
use lang_frontend::parser::ParsedFile;
use std::collections::BTreeSet;

pub(super) fn check(
    c: &Case,
    p: &ParsedFile,
    i: &Input,
    proof: &Proof,
    program: &Program,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "If actual original SSA module")?;
    let scan = function(module, i.scan)?;
    let make = function(module, i.make)?;
    let g = Graph::new(scan)?;
    println!(
        "SCOPE B IF SSA label={} unit=true function={:#?} edges={:#?}",
        c.label, scan, g.edges
    );
    let parameters = &g.block(scan.entry_block().unwrap())?.parameters;
    let [flag] = parameters.as_slice() else {
        return Err(failure("If actual scanner one flag parameter", parameters));
    };
    let flag_type = scan.entity(*flag).unwrap().ty;
    need(
        matches!(flag_type,EntityType::Loan{kind:LoanKind::Shared,target}if module.type_kind(target)==Some(&SsaTypeKind::Boolean))
            && scan.return_types.is_empty(),
        "If original Borrow Boolean scanner ABI",
        flag_type,
    )?;
    let header = one(
        g.blocks.iter().copied().filter(|&b| {
            g.term(b).is_ok_and(|t| {
                t.origin == Origin::Source(span(p, i.conditional))
                    && matches!(t.kind, TerminatorKind::Conditional { .. })
            })
        }),
        "If actual original Conditional terminator",
    )?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(header)?.kind
    else {
        unreachable!()
    };
    let read = g.definition(EntityId::Value(*condition))?;
    need(
        read.origin == Origin::Source(span(p, i.condition))
            && read.operation
                == Operation::Read {
                    source: match flag {
                        EntityId::Loan(loan) => PlaceAccess::Loan(*loan),
                        _ => return Err(failure("If flag really is a loan", flag)),
                    },
                }
            && read.block == header
            && header == scan.entry_block().unwrap(),
        "If condition reads the original actual flag without polarity substitution",
        read,
    )?;
    let mut uses = Uses::default();
    uses.protect(*flag);
    uses.permit(*flag, read);
    let mut births = Vec::new();
    let mut exits = Vec::new();
    for (index, edge) in [when_true, when_false].into_iter().enumerate() {
        let call = one(
            g.instructions.iter().copied().filter(|op| {
                op.origin == Origin::Source(span(p, i.births[index]))
                    && matches!(op.operation,Operation::DirectCall{callee,..}if callee==make.id)
            }),
            "If actual original arm make Call",
        )?;
        need(
            call.block == edge.target
                && call.operation
                    == Operation::DirectCall {
                        callee: make.id,
                        receiver: None,
                        arguments: vec![],
                    }
                && g.block(edge.target)?.origin
                    == Origin::Source(p.ast().statements().get(i.arms[index]).unwrap().span()),
            "If actual true/false arm selects its own original owned make result",
            call,
        )?;
        g.incoming_from(call.block, &[header])?;
        let born = result(call)?;
        let ty = scan.entity(born).unwrap().ty;
        need(
            matches!(ty,EntityType::Value(t)if module.map_container(t).is_some() && make.return_types == [t]),
            "If arm call has the real concrete owned Map ABI",
            ty,
        )?;
        uses.protect(born);
        let branch = g.branch(call.block)?;
        need(
            branch.arguments.first() == Some(&born)
                && !g.instructions.iter().any(|op| {
                    op.block == call.block
                        && op.operation
                            == Operation::Drop {
                                owner: value(call).unwrap(),
                            }
                }),
            "If complete arm delivery places its live result in join column zero",
            branch,
        )?;
        births.push(born);
        exits.push((call.block, branch));
    }
    need(
        exits[0].1.target == exits[1].1.target,
        "If both original arms join the same actual block",
        &exits,
    )?;
    let join = exits[0].1.target;
    g.incoming_from(join, &[exits[0].0, exits[1].0])?;
    let owner = *g
        .block(join)?
        .parameters
        .first()
        .ok_or_else(|| failure("If actual result join column zero", join))?;
    need(
        scan.entity(owner).unwrap().definition
            == Definition::BlockParameter {
                block: join,
                index: 0,
            }
            && g.argument(exits[0].1, owner)? == births[0]
            && g.argument(exits[1].1, owner)? == births[1],
        "If complete original true/false incoming owner column",
        owner,
    )?;
    g.anchored(owner, &births)?;
    uses.protect(owner);
    let (owner, tail, provider_blocks) = if c.provider {
        let delivered = provider::check(&g, module, p, i, (owner, *flag), &mut uses)?;
        (delivered.owner, delivered.block, delivered.blocks.to_vec())
    } else {
        (owner, join, vec![])
    };
    for &block in &g.blocks {
        for &parameter in &g.block(block)?.parameters {
            if parameter == owner
                || c.provider && (block == join || provider_blocks.contains(&block))
            {
                continue;
            }
            g.anchored(parameter, &[*flag])?;
        }
    }
    need(
        g.blocks.iter().copied().collect::<BTreeSet<_>>()
            == [header, exits[0].0, exits[1].0, join]
                .into_iter()
                .chain(provider_blocks)
                .collect(),
        "If complete original branch/provider CFG",
        &g.blocks,
    )?;
    let EntityId::Value(owner_value) = owner else {
        return Err(failure("If joined owned Map is a Value", owner));
    };
    let mut ends = Vec::new();
    for &(call, arg) in &i.observes {
        let observer = function(module, i.observe)?;
        let actual = one(
            g.instructions.iter().copied().filter(|op| {
                op.origin == Origin::Source(span(p, call))
                    && matches!(op.operation,Operation::DirectCall{callee,..}if callee==observer.id)
            }),
            "If actual final observe Call",
        )?;
        let Operation::DirectCall {
            receiver: None,
            arguments,
            ..
        } = &actual.operation
        else {
            return Err(failure("If actual observe ABI", actual));
        };
        let [EntityId::Loan(loan)] = arguments.as_slice() else {
            return Err(failure("If actual observe Shared Map argument", actual));
        };
        let begin = g.definition(EntityId::Loan(*loan))?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = begin.operation
        else {
            return Err(failure("If actual observe shared loan birth", begin));
        };
        let root = g.definition(EntityId::Place(place))?;
        need(
            root.operation == Operation::RootPlace { owner: owner_value }
                && root.origin == Origin::Source(span(p, arg))
                && begin.origin == Origin::Source(span(p, arg))
                && actual.block == tail
                && root.block == tail
                && begin.block == tail
                && g.position(root)? < g.position(begin)?
                && g.position(begin)? < g.position(actual)?,
            "If observe borrows the actual complete joined owner",
            (root, begin, actual),
        )?;
        let end = one(
            g.instructions
                .iter()
                .copied()
                .filter(|op| op.operation == Operation::BorrowEnd { loan: *loan }),
            "If unique actual observe loan End",
        )?;
        need(
            end.block == tail
                && end.origin == Origin::Source(span(p, call))
                && g.position(end)? == g.position(actual)? + 1,
            "If actual observe returns before its adjacent shared End",
            end,
        )?;
        uses.permit(owner, root);
        for (entity, op) in [
            (EntityId::Place(place), begin),
            (EntityId::Loan(*loan), actual),
            (EntityId::Loan(*loan), end),
        ] {
            uses.protect(entity);
            uses.permit(entity, op);
        }
        ends.push(end);
    }
    for pair in ends.windows(2) {
        need(
            g.position(pair[0])? < g.position(pair[1])?,
            "If actual repeated observe source order",
            pair,
        )?;
    }
    let drop = one(
        g.instructions
            .iter()
            .copied()
            .filter(|op| op.operation == Operation::Drop { owner: owner_value }),
        "If unique actual current joined owner Drop",
    )?;
    let published = &proof.drop;
    let after_delivery = if let Some(end) = ends.last() {
        g.position(end)? < g.position(drop)?
    } else {
        c.temporary && g.position(drop)? == 1
    };
    need(drop.block==tail&&drop.origin==Origin::Source(published.value_origin())&&after_delivery&&g.ops(tail)?.last().map(|op|op.id)==Some(drop.id)&&matches!(&g.term(tail)?.kind,TerminatorKind::Return{values}if values.is_empty())&&g.instructions.iter().filter(|op|matches!(op.operation,Operation::Drop{owner}if scan.entity(EntityId::Value(owner)).is_some_and(|e|module.map_container(e.ty.semantic_type()).is_some()))).count()==1,"If branch owner survives each real observe and is uniquely dropped before Return",(drop,published))?;
    uses.permit(owner, drop);
    for &block in &g.blocks {
        for entity in g.block(block)?.parameters.iter().copied().chain(
            g.ops(block)?
                .into_iter()
                .flat_map(|op| op.results.iter().copied()),
        ) {
            if matches!(scan.entity(entity).unwrap().ty,EntityType::Value(t)if module.map_container(t).is_some())
            {
                g.anchored(entity, &births)?;
                uses.protect(entity);
            }
        }
    }
    uses.check(&g, None)
}
