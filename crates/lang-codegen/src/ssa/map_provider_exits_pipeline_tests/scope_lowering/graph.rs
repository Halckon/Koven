//! Borrow parameter anchors are followed through every real incoming column.
mod body;
mod callers {
    //! The caller keeps each actual make result through its exact scanner CallReturn.
    use super::*;
    pub(super) fn check(
        c: &Case,
        sources: &SourceMap,
        parsed: &ParsedFile,
        i: &ast::Input,
        module: &Module,
    ) -> Result<(), Failure> {
        let entry = function(module, i.entry.span)?;
        let scanner = function(module, i.scan.span)?;
        let g = Graph::with_abort(entry)?;
        let mut uses = Uses::default();
        let mut owners = BTreeSet::new();
        for call in &i.callers {
            let invoke = at(&g, ast::span(parsed, call.id), |o| {
                matches!(o, Operation::DirectCall { .. })
            })?;
            let Operation::DirectCall {
                callee,
                receiver: None,
                arguments,
            } = &invoke.operation
            else {
                return Err(failure(
                    "scope original scanner ABI actual invocation",
                    invoke,
                ));
            };
            need(
                *callee == scanner.id
                    && arguments.len() == call.arguments.len()
                    && invoke.results.is_empty(),
                "scope caller actual original scanner invocation",
                invoke,
            )?;
            if c.gate == 2 {
                continue;
            }
            let make = function(module, i.make.as_ref().unwrap().span)?;
            let birth = at(&g, ast::span(parsed, call.arguments[0]), |o| {
                matches!(o, Operation::DirectCall { .. })
            })?;
            need(
                birth.operation
                    == Operation::DirectCall {
                        callee: make.id,
                        receiver: None,
                        arguments: vec![],
                    },
                "scope actual owned result born at exact make()",
                birth,
            )?;
            let seed = value(birth)?;
            owners.insert(seed);
            uses.protect(EntityId::Value(seed));
            for (n, &argument) in arguments.iter().enumerate() {
                let begin = g.definition(argument)?;
                let Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                } = begin.operation
                else {
                    return Err(failure("scope actual caller temporary Shared loan", begin));
                };
                let root = g.definition(EntityId::Place(place))?;
                let Operation::RootPlace { owner } = root.operation else {
                    return Err(failure("scope caller actual temporary root", root));
                };
                let producer = g.definition(EntityId::Value(owner))?;
                need(
                    begin.origin == Origin::Source(ast::span(parsed, call.arguments[n]))
                        && root.origin == begin.origin
                        && if n == 0 {
                            owner == seed && producer.id == birth.id
                        } else {
                            matches!(
                                producer.operation,
                                Operation::Constant(ScalarConstant::Boolean(value)) if value == (sources.slice(ast::span(parsed, call.arguments[n])) == Ok("true"))
                            ) && producer.origin == begin.origin
                        },
                    "scope actual ordered Map/Boolean root/loan producers",
                    (root, begin, producer),
                )?;
                uses.protect(EntityId::Value(owner));
                uses.permit(EntityId::Value(owner), root);
                uses.protect(EntityId::Place(place));
                uses.permit(EntityId::Place(place), begin);
                uses.protect(argument);
                uses.permit(argument, invoke);
                before(&g, producer, root)?;
                before(&g, root, begin)?;
                before(&g, begin, invoke)?;
                let end=one(g.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::BorrowEnd{loan}if EntityId::Loan(loan)==argument)),"scope unique exact caller argument End")?;
                need(
                    end.origin == Origin::Source(ast::span(parsed, call.id)),
                    "scope caller exact CallReturn End",
                    end,
                )?;
                before(&g, invoke, end)?;
                uses.permit(argument, end);
                if n == 0 {
                    let drop = one(
                        g.instructions
                            .iter()
                            .copied()
                            .filter(|op| op.operation == Operation::Drop { owner: seed }),
                        "scope sole original make owner cleanup",
                    )?;
                    need(
                        drop.origin == Origin::Source(ast::span(parsed, call.arguments[0])),
                        "scope actual temporary map cleanup identity",
                        drop,
                    )?;
                    before(&g, end, drop)?;
                    uses.permit(EntityId::Value(seed), drop);
                }
            }
            let map = entry
                .entity(EntityId::Value(seed))
                .unwrap()
                .ty
                .semantic_type();
            let produce = ast::function(sources, parsed, "produce")?;
            let original = super::super::super::temporary_arguments::sources().remove(0);
            super::super::super::temporary_arguments::graph::factory::check(
                &original,
                sources,
                parsed,
                module,
                function(module, produce.span)?,
                map,
            )?;
            let mg = Graph::with_abort(make)?;
            let selected=one(mg.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::DirectCall{callee,..}if callee==function(module,produce.span).unwrap().id)),"scope make returns actual populated factory result")?;
            let result = value(selected)?;
            need(
                mg.blocks
                    .iter()
                    .filter_map(|&block| match &mg.term(block).unwrap().kind {
                        TerminatorKind::Return { values } => Some(values.as_slice()),
                        _ => None,
                    })
                    .eq([&[result][..]]),
                "scope make exact current factory result normal Return",
                selected,
            )?;
        }
        need(g.instructions.iter().filter(|op|matches!(op.operation,Operation::Drop{owner}if module.map_container(entry.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())).count()==owners.len(),"scope caller exactly one Drop per actual map result",&owners)?;
        uses.check(&g, None)
    }
}
mod provider {
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
}
use super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, loan, result, value},
    query_inputs::one,
};
use super::{Case, ast};
use crate::ssa::model::*;
use lang_frontend::{
    parser::ParsedFile,
    source::{SourceMap, Span},
};
use std::collections::BTreeSet;
fn at<'a>(
    g: &Graph<'a>,
    span: Span,
    predicate: impl Fn(&Operation) -> bool,
) -> Result<&'a Instruction, Failure> {
    one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.origin == Origin::Source(span) && predicate(&i.operation)),
        "scope exact original SSA operation",
    )
}
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
fn protect(uses: &mut Uses, family: &BTreeSet<EntityId>) {
    for &e in family {
        uses.protect(e);
    }
}
/// A cross-block ordering is accepted only when all paths from entry to the later
/// instruction pass through the former instruction's block. Cycles do not imply order.
fn before(g: &Graph<'_>, a: &Instruction, b: &Instruction) -> Result<(), Failure> {
    if a.block == b.block {
        return need(
            g.position(a)? < g.position(b)?,
            "scope actual same-block instruction order",
            (a, b),
        );
    }
    let entry = g.function.entry_block().unwrap();
    let mut pending = vec![entry];
    let mut seen = BTreeSet::new();
    while let Some(block) = pending.pop() {
        if block == a.block || !seen.insert(block) {
            continue;
        }
        need(
            block != b.block,
            "scope all paths to later operation pass earlier block",
            (a, b, block),
        )?;
        for (from, e) in &g.edges {
            if *from == block {
                pending.push(e.target);
            }
        }
    }
    Ok(())
}
pub(super) fn check(
    c: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    i: &ast::Input,
    program: &Program,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "scope actual original module")?;
    let scan = function(module, i.scan.span)?;
    let g = Graph::with_abort(scan)?;
    callers::check(c, sources, parsed, i, module)?;
    if c.gate == 2 {
        need(
            g.instructions.is_empty()
                && g.blocks.len() == 1
                && g.edges.is_empty()
                && g.block(scan.entry_block().unwrap())?.parameters.is_empty()
                && matches!(&g.term(scan.entry_block().unwrap())?.kind,TerminatorKind::Return{values}if values.is_empty())
                && g.term(scan.entry_block().unwrap())?.origin
                    == Origin::Source(ast::span(parsed, i.early_return.unwrap())),
            "scope legal unacquired dead tail has no SSA construction/provider/cleanup",
            scan,
        )?;
        crate::ssa::map_provider_exits_pipeline_tests::query_inventory::check(
            &g,
            &[],
            &BTreeSet::new(),
        )?;
        return Ok(());
    }
    let entry = g.block(scan.entry_block().unwrap())?;
    need(
        entry.parameters.len() == 2 && scan.borrow_return.is_none() && scan.return_types.is_empty(),
        "scope actual original scanner Borrow ABI",
        entry,
    )?;
    let seed = entry.parameters[0];
    let ty = scan.entity(seed).unwrap().ty;
    let map = ty.semantic_type();
    need(
        matches!(
            ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                ..
            }
        ) && module.map_container(map).is_some_and(|(kind, k, v)| {
            kind == MapContainerKind::Map
                && module.type_kind(k) == Some(&SsaTypeKind::StringOwner)
                && module.type_kind(v) == Some(&SsaTypeKind::StringOwner)
        }),
        "scope original Borrow Map String/String capability",
        (seed, ty),
    )?;
    let originals = family(&g, &[seed])?;
    let mut uses = Uses::default();
    let mut expected_map = originals.clone();
    protect(&mut uses, &originals);
    let leased = if let Some(site) = &i.site {
        provider::check(c, parsed, i, module, &g, (&originals, site), &mut uses)?
    } else {
        need(
            !g.instructions.iter().any(|op| {
                matches!(
                    op.operation,
                    Operation::MapIterationFirst { .. }
                        | Operation::MapIterationHasNext { .. }
                        | Operation::MapIterationKeyLoan { .. }
                        | Operation::MapIterationValueLoan { .. }
                        | Operation::MapIterationNext { .. }
                )
            }),
            "scope original no-For has no provider operations",
            scan.id,
        )?;
        BTreeSet::new()
    };
    expected_map.extend(leased.iter().copied());
    if let Some(call) = &i.observe_call {
        let op = at(&g, ast::span(parsed, call.id), |o| {
            matches!(o, Operation::DirectCall { .. })
        })?;
        let Operation::DirectCall {
            callee,
            receiver: None,
            arguments,
        } = &op.operation
        else {
            return Err(failure("scope actual control observe ABI", op));
        };
        need(
            *callee == function(module, i.observe.as_ref().unwrap().span)?.id
                && arguments.len() == 1,
            "scope control selected original observe",
            op,
        )?;
        let argument = arguments[0];
        if originals.contains(&argument) {
            // This ABI directly borrows an already active input loan. Full actual
            // incoming columns are anchored to entry; the callee cannot end it.
            g.anchored(argument, &[seed])?;
            uses.permit(argument, op);
            need(!g.instructions.iter().any(|instruction|matches!(instruction.operation,Operation::BorrowEnd{loan}if originals.contains(&EntityId::Loan(loan)))),"scope direct Borrow call preserves caller's active source lease",op)?;
        } else {
            let begin = g.definition(argument)?;
            let Operation::SharedReborrow { source } = begin.operation else {
                return Err(failure(
                    "scope control actual independent argument reborrow",
                    begin,
                ));
            };
            need(
                originals.contains(&EntityId::Loan(source))
                    && begin.origin == Origin::Source(ast::span(parsed, call.arguments[0])),
                "scope control reborrow rooted at actual current input",
                begin,
            )?;
            uses.permit(EntityId::Loan(source), begin);
            let aliases = family(&g, &[argument])?;
            expected_map.extend(aliases.iter().copied());
            protect(&mut uses, &aliases);
            uses.permit(argument, op);
            let end=one(g.instructions.iter().copied().filter(|instruction|matches!(instruction.operation,Operation::BorrowEnd{loan}if aliases.contains(&EntityId::Loan(loan)))),"scope independent control argument unique End")?;
            need(
                end.origin == Origin::Source(ast::span(parsed, call.id)),
                "scope independent control argument CallReturn",
                end,
            )?;
            before(&g, op, end)?;
            for e in end.operation.entities() {
                uses.permit(e, end);
            }
        }
    }

    body::check(c, sources, parsed, i, module, &g, &mut uses)?;
    for op in &g.instructions {
        need(
            !matches!(op.operation,Operation::Drop{owner}if module.map_container(scan.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some()),
            "scope borrowed scanner cannot Drop caller Map",
            op,
        )?;
    }
    for (index, data) in scan.loans.iter().enumerate() {
        if module.map_container(data.ty.semantic_type()).is_some() {
            let e = EntityId::Loan(LoanId {
                function: scan.id,
                index,
            });
            need(
                originals.contains(&e)
                    || leased.contains(&e)
                    || i.observe_call.as_ref().is_some_and(|call| {
                        g.definition(e).is_ok_and(|op| {
                            op.origin == Origin::Source(ast::span(parsed, call.arguments[0]))
                        })
                    }),
                "scope every actual Map loan has original input/provider/control provenance",
                (e, data),
            )?;
        }
    }
    crate::ssa::map_provider_exits_pipeline_tests::query_inventory::check(
        &g,
        &[map],
        &expected_map,
    )?;
    uses.check(&g, None)
}
