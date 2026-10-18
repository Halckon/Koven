//! Read-only Break proof over original declarations, complete incoming columns and operands.
use super::super::{
    continuation::graph::{check_local_guard, transport},
    identity::Input,
};
use crate::ssa::{
    map_provider_exits_pipeline_tests::{
        fixture::{Failure, failure, need},
        query_graph::{Graph, Uses, loan, result, value},
        query_inputs::one,
    },
    model::*,
};
use lang_frontend::{
    parser::{Expression, Item, NameMarker, ParsedFile},
    source::SourceMap,
};
use std::collections::BTreeSet;

pub(super) fn check(
    program: &Program,
    input: &Input,
    sources: &SourceMap,
    parsed: &ParsedFile,
    unit: bool,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "one original module")?;
    let scan = one(
        module
            .functions
            .iter()
            .filter(|f| f.origin == Origin::Source(input.scan)),
        "original Break scan",
    )?;
    let g = Graph::new(scan)?;
    let [anchor] = g.block(scan.entry_block().unwrap())?.parameters.as_slice() else {
        return Err(failure("one original scan parameter", scan));
    };
    need(
        matches!(
            (input.owned, scan.entity(*anchor).unwrap().ty),
            (true, EntityType::Value(_))
                | (
                    false,
                    EntityType::Loan {
                        kind: LoanKind::Shared,
                        ..
                    }
                )
        ),
        "original Value/Borrow ABI",
        anchor,
    )?;
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
    need(
        !g.instructions
            .iter()
            .any(|i| matches!(i.operation, Operation::MapIterationNext { .. })),
        "Break has no reachable Next",
        scan.id,
    )?;
    need(
        [first, guard, key, val]
            .iter()
            .all(|i| i.origin == Origin::Source(input.for_span)),
        "actual original For origins",
        input.for_span,
    )?;
    let body_span = parsed.ast().statements().get(input.body).unwrap().span();
    let broken = one(
        parsed.ast().expressions().iter().filter_map(|(_, node)| {
            (matches!(node.payload(), Expression::Break { .. })
                && node.span().start() > body_span.start()
                && node.span().end() < body_span.end())
            .then_some(node.span())
        }),
        "original body Break",
    )?;
    need(
        g.term(key.block)?.origin == Origin::Source(input.for_span),
        "actual For merge branch origin",
        g.term(key.block)?,
    )?;
    let Operation::MapIterationFirst { source } = first.operation else {
        unreachable!()
    };
    let birth = g.definition(EntityId::Loan(source))?;
    let mut uses = Uses::default();
    let mut originals = BTreeSet::from([*anchor]);
    if input.owned {
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = birth.operation
        else {
            return Err(failure("owned provider source loan", birth));
        };
        let root = g.definition(EntityId::Place(place))?;
        need(
            matches!(root.operation, Operation::RootPlace { owner } if EntityId::Value(owner) == *anchor),
            "provider roots original owned parameter",
            root,
        )?;
        uses.protect(EntityId::Place(place));
        uses.permit(EntityId::Place(place), birth);
        uses.permit(*anchor, root);
    } else {
        let Operation::SharedReborrow { source: caller } = birth.operation else {
            return Err(failure("callee provider reborrow", birth));
        };
        originals.extend(transport(&g, EntityId::Loan(caller), *anchor)?);
        uses.permit(EntityId::Loan(caller), birth);
    }
    // Forward discovery finds every carried original; backward proof checks ALL its incoming.
    loop {
        let before = originals.len();
        for (_, edge) in &g.edges {
            for (&argument, &parameter) in
                edge.arguments.iter().zip(&g.block(edge.target)?.parameters)
            {
                if originals.contains(&argument) {
                    originals.insert(parameter);
                }
            }
        }
        if originals.len() == before {
            break;
        }
    }
    for &entity in &originals {
        transport(&g, entity, *anchor)?;
        uses.protect(entity);
    }
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
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(guard.block)?.kind
    else {
        return Err(failure("actual HasNext branch", guard));
    };
    need(
        *condition == value(guard)?
            && when_true.target == key.block
            && val.block == key.block
            && val.operation
                == Operation::MapIterationValueLoan {
                    source: bs,
                    cursor: bc,
                },
        "guard and current K/V operands",
        (guard, key, val),
    )?;
    need(
        first.block == scan.entry_block().unwrap(),
        "actual provider preheader",
        first,
    )?;
    g.incoming_from(first.block, &[])?;
    g.incoming_from(guard.block, &[first.block])?;
    g.incoming_from(key.block, &[guard.block])?;
    g.incoming_from(when_false.target, &[guard.block])?;
    let entry = g.branch(first.block)?;
    need(
        entry.target == guard.block
            && g.transported(entry, EntityId::Loan(source))? == EntityId::Loan(hs)
            && g.transported(entry, result(first)?)? == EntityId::Value(hc)
            && g.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc),
        "every provider incoming source/cursor column",
        (entry, when_true),
    )?;
    let xs = g.transported(when_false, EntityId::Loan(hs))?;
    let xc = g.transported(when_false, EntityId::Value(hc))?;
    let body_end = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::BorrowEnd { loan: bs }),
        "exact Break EndSource",
    )?;
    let false_end = one(
        g.instructions.iter().copied().filter(
            |i| matches!(i.operation, Operation::BorrowEnd { loan } if EntityId::Loan(loan)==xs),
        ),
        "exact exhaustion EndSource",
    )?;
    need(
        body_end.block == key.block && false_end.block == when_false.target,
        "both actual exits end their own source",
        (body_end, false_end),
    )?;
    let mut leases = BTreeSet::new();
    for entity in [EntityId::Loan(hs), EntityId::Loan(bs), xs] {
        leases.extend(transport(&g, entity, EntityId::Loan(source))?);
    }
    need(
        leases.is_disjoint(&originals),
        "provider lease and original caller/owner remain distinct",
        (&leases, &originals),
    )?;
    for &entity in &leases {
        uses.protect(entity);
    }
    for cursor in [result(first)?, EntityId::Value(hc), EntityId::Value(bc), xc] {
        uses.protect(cursor);
    }
    uses.permit(EntityId::Loan(source), first);
    uses.permit(EntityId::Loan(hs), guard);
    uses.permit(EntityId::Value(hc), guard);
    for instruction in [key, val] {
        uses.permit(EntityId::Loan(bs), instruction);
        uses.permit(EntityId::Value(bc), instruction);
    }
    uses.permit(EntityId::Loan(bs), body_end);
    uses.permit(xs, false_end);
    // No provider state may leak past either terminal cleanup or gain an extra edge.
    for (from, edge) in &g.edges {
        for &entity in &edge.arguments {
            if leases.contains(&entity)
                || [result(first)?, EntityId::Value(hc), EntityId::Value(bc), xc].contains(&entity)
            {
                need(
                    (*from == first.block && edge.target == guard.block)
                        || (*from == guard.block
                            && [key.block, when_false.target].contains(&edge.target)),
                    "only proved provider state transport edges",
                    (from, edge, entity),
                )?;
            }
        }
    }
    let read = one(g.instructions.iter().copied().filter(|i| matches!(&i.operation, Operation::DirectCall { receiver: None, arguments, .. } if arguments.as_slice()==[result(key).unwrap(), result(val).unwrap()])), "actual K/V reader")?;
    let Operation::DirectCall { callee, .. } = read.operation else {
        unreachable!()
    };
    let reader = module
        .function(callee)
        .ok_or_else(|| failure("actual reader callee", callee))?;
    need(
        reader.origin == Origin::Source(input.read),
        "selected original reader",
        reader.id,
    )?;
    let map_type = scan.entity(*anchor).unwrap().ty.semantic_type();
    let (_, kt, vt) = module
        .map_container(map_type)
        .ok_or_else(|| failure("original Map storage", map_type))?;
    need(
        module.type_kind(kt) == Some(&SsaTypeKind::StringOwner),
        "original String key storage",
        kt,
    )?;
    for cursor in [result(first)?, EntityId::Value(hc), EntityId::Value(bc), xc] {
        need(scan.entity(cursor).is_some_and(|e| matches!(e.ty, EntityType::Value(t) if module.type_kind(t)==Some(&SsaTypeKind::Integer { bits:64, signed:false }))), "every actual provider cursor has unsigned index storage", cursor)?;
    }
    for (component, target, parameter) in [(key, kt, 0), (val, vt, 1)] {
        let ty = EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        };
        need(
            scan.entity(result(component)?).unwrap().ty == ty
                && reader
                    .block(reader.entry_block().unwrap())
                    .unwrap()
                    .parameters
                    .get(parameter)
                    .and_then(|p| reader.entity(*p))
                    .is_some_and(|p| p.ty == ty),
            "component retains exact storage and reader ABI",
            (component, reader),
        )?;
    }
    let mut ends = vec![];
    for component in [key, val] {
        let entity = result(component)?;
        let end = one(
            g.instructions.iter().copied().filter(|i| {
                i.operation
                    == Operation::BorrowEnd {
                        loan: loan(component).unwrap(),
                    }
            }),
            "unique actual component End",
        )?;
        uses.protect(entity);
        uses.permit(entity, read);
        uses.permit(entity, end);
        ends.push(end);
    }
    need(
        [ends[0], ends[1], body_end]
            .iter()
            .all(|i| i.origin == Origin::Source(if unit { input.for_span } else { broken })),
        "actual K/V and source cleanup retain selected entry origin contract",
        (&ends, body_end, broken),
    )?;
    need(
        read.block == key.block
            && ends.iter().all(|i| i.block == key.block)
            && g.position(read)? < g.position(ends[1])?
            && g.position(ends[1])? < g.position(ends[0])?
            && g.position(ends[0])? < g.position(body_end)?,
        "actual read/value End/key End/EndSource",
        (&ends, body_end),
    )?;
    check_local_guard(
        &g,
        module,
        input,
        sources,
        parsed,
        (read, ends[1]),
        &mut uses,
    )?;
    let drops = g.instructions.iter().copied().filter(|i| matches!(i.operation, Operation::Drop { owner } if scan.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()==map_type)).collect::<Vec<_>>();
    if input.owned {
        let [drop] = drops.as_slice() else {
            return Err(failure(
                "one lexical Map Drop after Break/exhaustion merge",
                &drops,
            ));
        };
        let Operation::Drop { owner } = drop.operation else {
            unreachable!()
        };
        need(
            drop.origin == Origin::Source(input.parameter),
            "actual lexical Drop parameter identity",
            drop,
        )?;
        originals.extend(transport(&g, EntityId::Value(owner), *anchor)?);
        for &entity in &originals {
            uses.protect(entity);
        }
        uses.permit(EntityId::Value(owner), drop);
        let merge = g.branch(key.block)?.target;
        need(
            g.branch(when_false.target)?.target == merge,
            "Break and exhaustion reach same owner merge",
            merge,
        )?;
        g.incoming_from(merge, &[key.block, when_false.target])?;
        let merged_owner = g.transported(
            g.branch(key.block)?,
            g.transported(when_true, g.transported(entry, *anchor)?)?,
        )?;
        transport(&g, merged_owner, *anchor)?;
    } else {
        need(drops.is_empty() && !g.instructions.iter().any(|i| matches!(i.operation, Operation::BorrowEnd { loan } if originals.contains(&EntityId::Loan(loan)))), "callee never drops caller Map or ends original caller loan", scan.id)?;
    }
    let post = post_uses(&g, module, input, sources, parsed, &originals, &mut uses)?;
    uses.check(&g, None)?;
    paths(&g, body_end.id, false_end.id, &drops, &post, input.owned)?;
    if !input.owned {
        caller(module, scan)?;
    }
    Ok(())
}

fn post_uses(
    g: &Graph<'_>,
    module: &Module,
    input: &Input,
    sources: &SourceMap,
    parsed: &ParsedFile,
    originals: &BTreeSet<EntityId>,
    uses: &mut Uses,
) -> Result<BTreeSet<InstructionId>, Failure> {
    let mut post = BTreeSet::new();
    for instruction in &g.instructions {
        if let Operation::MapSize { owner } = instruction.operation {
            need(
                originals.contains(&owner)
                    && matches!(instruction.origin, Origin::Source(span) if sources.slice(span)==Ok("source.size") && span.start()>input.for_span.end() && span.end()<input.scan.end()),
                "post-loop size uses original transported source",
                instruction,
            )?;
            uses.permit(owner, instruction);
            post.insert(instruction.id);
        }
    }
    for (_, node) in parsed.ast().expressions().iter() {
        if node.span().start() <= input.for_span.end() || node.span().end() >= input.scan.end() {
            continue;
        }
        let Expression::Call {
            callee, arguments, ..
        } = node.payload()
        else {
            continue;
        };
        if sources.slice(parsed.ast().expressions().get(*callee).unwrap().span()) != Ok("readMap") {
            continue;
        }
        need(
            matches!(arguments.as_slice(), [a] if sources.slice(parsed.ast().expressions().get(a.value).unwrap().span())==Ok("source")),
            "original post-loop readMap source",
            node,
        )?;
        let decl = one(parsed.roots().iter().filter_map(|id| { let n=parsed.ast().items().get(*id).ok()?; matches!(n.payload(), Item::Function { name: NameMarker::Present(name), .. } if sources.slice(*name)==Ok("readMap")).then_some(n.span()) }), "original readMap declaration")?;
        let callee = one(
            module
                .functions
                .iter()
                .filter(|f| f.origin == Origin::Source(decl)),
            "actual readMap callee",
        )?;
        let call = one(
            g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(node.span())
                    && matches!(i.operation, Operation::DirectCall { .. })
            }),
            "actual later readMap call",
        )?;
        let Operation::DirectCall {
            callee: actual,
            receiver: None,
            arguments,
        } = &call.operation
        else {
            return Err(failure("actual readMap call ABI", call));
        };
        need(*actual == callee.id, "selected original readMap", call)?;
        let [argument @ EntityId::Loan(_)] = arguments.as_slice() else {
            return Err(failure("readMap exact Borrow argument", call));
        };
        if !input.owned {
            // Borrow-to-Borrow forwarding uses the existing caller loan. Unit
            // lowering may carry it through a phi; originals already proves
            // every incoming to the original entry parameter. It gains no End.
            need(
                originals.contains(argument),
                "later Borrow forwards proven original caller loan",
                argument,
            )?;
            uses.permit(*argument, call);
            post.insert(call.id);
            continue;
        }
        let birth = g.definition(*argument)?;
        {
            let Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            } = birth.operation
            else {
                return Err(failure("later own source Borrow", birth));
            };
            let root = g.definition(EntityId::Place(place))?;
            let Operation::RootPlace { owner } = root.operation else {
                return Err(failure("later source RootPlace", root));
            };
            need(
                originals.contains(&EntityId::Value(owner)),
                "later Borrow original owner transport",
                root,
            )?;
            uses.permit(EntityId::Value(owner), root);
            uses.protect(EntityId::Place(place));
            uses.permit(EntityId::Place(place), birth);
        }
        let EntityId::Loan(argument_loan) = argument else {
            unreachable!()
        };
        let end = one(
            g.instructions.iter().copied().filter(|i| {
                i.operation
                    == Operation::BorrowEnd {
                        loan: *argument_loan,
                    }
            }),
            "later scoped Borrow End",
        )?;
        need(
            birth.block == call.block
                && end.block == call.block
                && g.position(birth)? < g.position(call)?
                && g.position(call)? < g.position(end)?,
            "later Borrow birth/call/end",
            (birth, call, end),
        )?;
        uses.protect(*argument);
        uses.permit(*argument, call);
        uses.permit(*argument, end);
        post.extend([birth.id, call.id, end.id]);
    }
    Ok(post)
}

fn paths(
    g: &Graph<'_>,
    body_end: InstructionId,
    false_end: InstructionId,
    drops: &[&Instruction],
    post: &BTreeSet<InstructionId>,
    owned: bool,
) -> Result<(), Failure> {
    let mut pending = vec![(g.function.entry_block().unwrap(), 0u8, 0u8)];
    let mut seen = BTreeSet::new();
    while let Some((block, mut ended, mut dropped)) = pending.pop() {
        if !seen.insert((block, ended, dropped)) {
            continue;
        }
        for i in g.ops(block)? {
            if [body_end, false_end].contains(&i.id) {
                ended += 1;
                need(ended == 1, "every path ends source exactly once", i)?;
            }
            if post.contains(&i.id) {
                need(
                    ended == 1 && dropped == 0,
                    "later size/Borrow occurs after source End before owner Drop",
                    i,
                )?;
            }
            if drops.iter().any(|d| d.id == i.id) {
                dropped += 1;
                need(
                    ended == 1 && dropped == 1,
                    "owner Drop once after EndSource",
                    i,
                )?;
            }
        }
        if matches!(g.term(block)?.kind, TerminatorKind::Return { .. }) {
            need(
                ended == 1 && dropped == u8::from(owned),
                "all normal Return paths retain exact cleanup",
                g.term(block)?,
            )?;
        }
        for (_, edge) in g.edges.iter().filter(|(from, _)| *from == block) {
            pending.push((edge.target, ended, dropped));
        }
    }
    Ok(())
}

fn caller(module: &Module, scan: &Function) -> Result<(), Failure> {
    let f = one(module.functions.iter().filter(|f| f.instructions.iter().filter(|i| matches!(i.operation,Operation::DirectCall { callee,.. } if callee==scan.id)).count()==2), "original caller populated and empty calls")?;
    let g = Graph::with_abort(f)?;
    let calls = g
        .instructions
        .iter()
        .copied()
        .filter(|i| matches!(i.operation,Operation::DirectCall { callee,.. } if callee==scan.id))
        .collect::<Vec<_>>();
    let mut populated = None;
    for call in calls {
        let Operation::DirectCall {
            receiver: None,
            arguments,
            ..
        } = &call.operation
        else {
            return Err(failure("caller Borrow call", call));
        };
        let [EntityId::Loan(argument)] = arguments.as_slice() else {
            return Err(failure("caller Borrow delivery", call));
        };
        let birth = g.definition(EntityId::Loan(*argument))?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = birth.operation
        else {
            return Err(failure("caller original scoped loan", birth));
        };
        let root = g.definition(EntityId::Place(place))?;
        let Operation::RootPlace { owner } = root.operation else {
            return Err(failure("caller original Map root", root));
        };
        let end = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: *argument }),
            "caller actual loan End",
        )?;
        need(
            birth.block == call.block
                && end.block == call.block
                && g.position(birth)? < g.position(call)?
                && g.position(call)? < g.position(end)?,
            "caller owns loan through whole scan",
            (birth, call, end),
        )?;
        if populated.is_none() {
            populated = Some((owner, end));
        }
    }
    let (owner, end) = populated.unwrap();
    let mut current = owner;
    for put in g
        .instructions
        .iter()
        .copied()
        .filter(|i| matches!(i.operation, Operation::MapPut { .. }))
    {
        need(
            matches!(put.operation,Operation::MapPut {owner:actual,..} if actual==owner)
                && put.block == end.block
                && g.position(end)? < g.position(put)?,
            "postscan mutation after caller loan End on same owner",
            put,
        )?;
        current = value(put)?;
    }
    let size = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::MapSize { .. })),
        "caller actual postscan size",
    )?;
    need(
        size.operation
            == Operation::MapSize {
                owner: EntityId::Value(current),
            }
            && size.block == end.block
            && g.position(end)? < g.position(size)?,
        "caller size uses latest MapPut owner after original loan End",
        size,
    )?;
    Ok(())
}
