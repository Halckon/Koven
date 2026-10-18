//! Independent actual CFG/operand query: no production qualification or carrier helpers.
use super::super::identity::Input;
use crate::ssa::{
    map_provider_exits_pipeline_tests::{
        fixture::{Failure, failure, need},
        query_graph::{Graph, Uses, loan, result, value},
        query_inputs::one,
    },
    model::*,
};
use std::collections::{BTreeMap, BTreeSet};

fn transport(
    g: &Graph<'_>,
    current: EntityId,
    anchor: EntityId,
) -> Result<BTreeSet<EntityId>, Failure> {
    let ty = g.function.entity(anchor).unwrap().ty;
    let mut parents = BTreeMap::new();
    let mut pending = vec![current];
    while let Some(entity) = pending.pop() {
        if entity == anchor || parents.contains_key(&entity) {
            continue;
        }
        let data = g
            .function
            .entity(entity)
            .ok_or_else(|| failure("actual entity", entity))?;
        let Definition::BlockParameter { block, .. } = data.definition else {
            return Err(failure("unrelated owner/lease", data));
        };
        need(data.ty == ty, "transport type", entity)?;
        let incoming = g.incoming(block);
        need(!incoming.is_empty(), "actual predecessor set", entity)?;
        let mut sources = vec![];
        for (_, edge) in incoming {
            let source = g.argument(edge, entity)?;
            sources.push(source);
            pending.push(source);
        }
        parents.insert(entity, sources);
    }
    let mut anchored = BTreeSet::from([anchor]);
    loop {
        let before = anchored.len();
        for (&entity, values) in &parents {
            if values.iter().any(|v| anchored.contains(v)) {
                anchored.insert(entity);
            }
        }
        if before == anchored.len() {
            break;
        }
    }
    need(
        parents.keys().all(|v| anchored.contains(v)),
        "all incoming owner/lease roots reach original anchor",
        &parents,
    )?;
    Ok(anchored)
}

pub(super) fn check(
    program: &Program,
    input: &Input,
    sources: &lang_frontend::source::SourceMap,
    parsed: &lang_frontend::parser::ParsedFile,
) -> Result<(), Failure> {
    let f = one(
        program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .filter(|f| f.origin == Origin::Source(input.scan)),
        "original scan function",
    )?;
    let g = Graph::new(f)?;
    let [anchor] = g.block(f.entry_block().unwrap())?.parameters.as_slice() else {
        return Err(failure("one original own parameter", f));
    };
    let EntityId::Value(anchor_value) = anchor else {
        return Err(failure("own Map ABI", anchor));
    };
    let op = |predicate: fn(&Operation) -> bool| {
        one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| predicate(&i.operation)),
            "unique actual provider operation",
        )
    };
    let first = op(|o| matches!(o, Operation::MapIterationFirst { .. }))?;
    let guard = op(|o| matches!(o, Operation::MapIterationHasNext { .. }))?;
    let key = op(|o| matches!(o, Operation::MapIterationKeyLoan { .. }))?;
    let val = op(|o| matches!(o, Operation::MapIterationValueLoan { .. }))?;
    let next = op(|o| matches!(o, Operation::MapIterationNext { .. }))?;
    need(
        [first, guard, key, val, next]
            .iter()
            .all(|i| i.origin == Origin::Source(input.for_span)),
        "original AST For origins",
        input.for_span,
    )?;
    let Operation::MapIterationFirst { source } = first.operation else {
        unreachable!()
    };
    let birth = g.definition(EntityId::Loan(source))?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = birth.operation
    else {
        return Err(failure("actual source lease", birth));
    };
    need(
        g.definition(EntityId::Place(place))?.operation
            == Operation::RootPlace {
                owner: *anchor_value,
            },
        "original source parameter root",
        birth,
    )?;
    let Operation::MapIterationHasNext {
        source: hs,
        cursor: hc,
    } = guard.operation
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
    need(
        val.block == key.block
            && val.operation
                == Operation::MapIterationValueLoan {
                    source: bs,
                    cursor: bc,
                }
            && next.block == key.block
            && next.operation
                == Operation::MapIterationNext {
                    source: bs,
                    cursor: bc,
                },
        "current K/V/Next provider operands",
        (key, val, next),
    )?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(guard.block)?.kind
    else {
        return Err(failure("actual provider condition", guard));
    };
    need(
        *condition == value(guard)? && when_true.target == key.block,
        "actual HasNext result",
        guard,
    )?;
    g.incoming_from(guard.block, &[first.block, key.block])?;
    g.incoming_from(key.block, &[guard.block])?;
    g.incoming_from(when_false.target, &[guard.block])?;
    let entry = g.branch(first.block)?;
    let back = g.branch(key.block)?;
    need(
        g.transported(entry, EntityId::Loan(source))? == EntityId::Loan(hs)
            && g.transported(entry, result(first)?)? == EntityId::Value(hc)
            && g.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc)
            && back.target == guard.block
            && g.argument(back, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.argument(back, EntityId::Value(hc))? == result(next)?,
        "every provider edge actual transport",
        (entry, back),
    )?;
    let xs = g.transported(when_false, EntityId::Loan(hs))?;
    let source_end = one(
        g.instructions.iter().copied().filter(
            |i| matches!(i.operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==xs),
        ),
        "unique actual source end",
    )?;
    need(
        source_end.block == when_false.target,
        "source ends on exhaustion",
        source_end,
    )?;
    let drops=g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::Drop{owner} if f.entity(EntityId::Value(owner)).unwrap().ty==f.entity(*anchor).unwrap().ty)).collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        return Err(failure("exactly one actual source owner Drop", drops));
    };
    let Operation::Drop { owner } = drop.operation else {
        unreachable!()
    };
    need(
        drop.origin == Origin::Source(input.parameter),
        "Drop original parameter identity",
        drop,
    )?;
    let mut owners = transport(&g, EntityId::Value(owner), *anchor)?;
    for &block in &g.blocks {
        for &entity in &g.block(block)?.parameters {
            if matches!(entity, EntityId::Value(_))
                && f.entity(entity).unwrap().ty == f.entity(*anchor).unwrap().ty
            {
                owners.extend(transport(&g, entity, *anchor)?);
            }
        }
    }
    let mut uses = Uses::default();
    for owner in &owners {
        uses.protect(*owner);
    }
    for i in &g.instructions {
        match i.operation {
            Operation::RootPlace { owner } if owners.contains(&EntityId::Value(owner)) => {
                uses.permit(EntityId::Value(owner), i)
            }
            Operation::Drop { owner } if i.id == drop.id => uses.permit(EntityId::Value(owner), i),
            Operation::MapSize { owner } if owners.contains(&owner) => {
                need(
                    matches!(i.origin,Origin::Source(span) if sources.slice(span)==Ok("source.size") && span.start()>input.for_span.end() && span.end()<input.scan.end()),
                    "actual original continuation size access",
                    i,
                )?;
                uses.permit(owner, i);
            }
            _ => {}
        }
    }
    let mut leases = BTreeSet::new();
    for e in [EntityId::Loan(hs), EntityId::Loan(bs), xs] {
        leases.extend(transport(&g, e, EntityId::Loan(source))?);
    }
    for e in &leases {
        uses.protect(*e);
    }
    uses.permit(EntityId::Loan(source), first);
    uses.permit(EntityId::Loan(hs), guard);
    for i in [key, val, next] {
        uses.permit(EntityId::Loan(bs), i);
    }
    uses.permit(xs, source_end);
    let read=one(g.instructions.iter().copied().filter(|i|matches!(&i.operation,Operation::DirectCall{receiver:None,arguments,..} if arguments.as_slice()==[result(key).unwrap(),result(val).unwrap()])),"actual K/V read call")?;
    let module = program.module(f.id.module()).unwrap();
    let Operation::DirectCall { callee, .. } = read.operation else {
        unreachable!()
    };
    need(
        module
            .function(callee)
            .is_some_and(|f| f.origin == Origin::Source(input.read)),
        "selected original read callee",
        read,
    )?;
    let mut ends = vec![];
    for component in [key, val] {
        let entity = result(component)?;
        uses.protect(entity);
        uses.permit(entity, read);
        let end = one(
            g.instructions.iter().copied().filter(|i| {
                i.operation
                    == Operation::BorrowEnd {
                        loan: loan(component).unwrap(),
                    }
            }),
            "unique component end",
        )?;
        uses.permit(entity, end);
        ends.push(end);
    }
    need(
        ends.iter().all(|i| i.block == next.block)
            && g.position(read)? < g.position(ends[1])?
            && g.position(ends[1])? < g.position(ends[0])?
            && g.position(ends[0])? < g.position(next)?,
        "read/value End/key End/Next",
        &ends,
    )?;
    uses.check(&g, None)?;
    let mut post_calls = BTreeSet::new();
    for (_, node) in parsed.ast().expressions().iter() {
        use lang_frontend::parser::{Expression, Item, NameMarker};
        if node.span().start() <= input.for_span.end() || node.span().end() >= input.scan.end() {
            continue;
        }
        let Expression::Call {
            callee: expr,
            arguments,
            ..
        } = node.payload()
        else {
            continue;
        };
        let callee_node = parsed.ast().expressions().get(*expr).unwrap();
        if sources.slice(callee_node.span()) != Ok("readMap") {
            continue;
        }
        need(
            arguments.len() == 1
                && sources.slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(arguments[0].value)
                        .unwrap()
                        .span(),
                ) == Ok("source"),
            "original continuation Borrow argument",
            node,
        )?;
        let decl=one(parsed.roots().iter().filter_map(|id|{let n=parsed.ast().items().get(*id).ok()?;matches!(n.payload(),Item::Function{name:NameMarker::Present(name),..} if sources.slice(*name)==Ok("readMap")).then_some(n.span())}),"original continuation declaration")?;
        let callee = one(
            module
                .functions
                .iter()
                .filter(|f| f.origin == Origin::Source(decl)),
            "actual continuation callee",
        )?;
        let call = one(
            g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(node.span())
                    && matches!(i.operation, Operation::DirectCall { .. })
            }),
            "actual continuation call",
        )?;
        let Operation::DirectCall {
            callee: actual,
            receiver: None,
            arguments,
        } = &call.operation
        else {
            return Err(failure("continuation signature", call));
        };
        let [EntityId::Loan(argument)] = arguments.as_slice() else {
            return Err(failure("continuation Borrow ABI", call));
        };
        need(*actual == callee.id, "selected actual readMap callee", call)?;
        let birth = g.definition(EntityId::Loan(*argument))?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = birth.operation
        else {
            return Err(failure("continuation argument source", birth));
        };
        let root = g.definition(EntityId::Place(place))?;
        need(
            matches!(root.operation,Operation::RootPlace{owner} if owners.contains(&EntityId::Value(owner))),
            "continuation borrows transported original owner",
            root,
        )?;
        let end = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: *argument }),
            "unique continuation loan End",
        )?;
        need(
            birth.block == call.block
                && end.block == call.block
                && drop.block == call.block
                && g.position(birth)? < g.position(call)?
                && g.position(call)? < g.position(end)?
                && g.position(end)? < g.position(drop)?,
            "continuation Borrow birth/call/end before lexical Drop",
            (birth, call, end, drop),
        )?;
        post_calls.insert(call.id);
    }
    // Every normal Return must consume this exact owner once, after EndSource
    // and any actual post-provider Borrow calls. Loops cannot hide a second Drop.
    let mut pending = vec![(f.entry_block().unwrap(), false, 0u8)];
    let mut seen = BTreeSet::new();
    while let Some((block, mut ended, mut dropped)) = pending.pop() {
        if !seen.insert((block, ended, dropped)) {
            continue;
        }
        for i in g.ops(block)? {
            if i.id == source_end.id {
                ended = true;
            }
            if post_calls.contains(&i.id) {
                need(
                    ended && dropped == 0,
                    "continuation uses source only after End and before Drop",
                    i,
                )?;
            }
            if i.id == drop.id {
                dropped += 1;
                need(
                    ended && dropped == 1,
                    "exactly one source Drop after End on every path",
                    drop,
                )?;
            }
        }
        if matches!(g.term(block)?.kind, TerminatorKind::Return { .. }) {
            need(
                dropped == 1,
                "every normal Return passed exact source Drop",
                g.term(block)?,
            )?;
        }
        for (_, edge) in g.edges.iter().filter(|(from, _)| *from == block) {
            pending.push((edge.target, ended, dropped));
        }
    }
    Ok(())
}
