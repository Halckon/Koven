//! Provider, complete incoming and local Guard proof for one original scanner.
use super::*;

pub(super) struct Evidence<'a> {
    pub body: BlockId,
    pub exhausted: BlockId,
    pub ends: Vec<&'a Instruction>,
    pub terminal_owners: BTreeMap<BlockId, EntityId>,
    pub key: &'a Instruction,
    pub val: &'a Instruction,
    pub read: &'a Instruction,
    pub local_drop: &'a Instruction,
}

pub(super) fn check<'a>(
    g: &Graph<'a>,
    module: &Module,
    sources: &SourceMap,
    parsed: &ParsedFile,
    site: (Span, lang_frontend::ast::StatementId, usize),
    owners: &BTreeSet<EntityId>,
    uses: &mut Uses,
) -> Result<Evidence<'a>, Failure> {
    let (span, body, exit) = site;
    let terminal = exit >= 2;
    let broken = exit == 2;
    let continued = exit == 1;
    let op = |p: fn(&Operation) -> bool| {
        one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.origin == Origin::Source(span) && p(&i.operation)),
            "actual original For operation",
        )
    };
    let first = op(|o| matches!(o, Operation::MapIterationFirst { .. }))?;
    let guard = op(|o| matches!(o, Operation::MapIterationHasNext { .. }))?;
    let key = op(|o| matches!(o, Operation::MapIterationKeyLoan { .. }))?;
    let val = op(|o| matches!(o, Operation::MapIterationValueLoan { .. }))?;
    let Operation::MapIterationFirst { source } = first.operation else {
        unreachable!()
    };
    let birth = g.definition(EntityId::Loan(source))?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = birth.operation
    else {
        return Err(failure("provider actual source loan birth", birth));
    };
    let root = g.definition(EntityId::Place(place))?;
    let Operation::RootPlace { owner } = root.operation else {
        return Err(failure("provider actual source root place", root));
    };
    need(
        owners.contains(&EntityId::Value(owner)),
        "every First borrows original current owner",
        root,
    )?;
    uses.permit(EntityId::Value(owner), root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), birth);
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
        return Err(failure("actual provider guard branch", guard));
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
        "actual guard and K/V operands",
        (guard, key, val),
    )?;
    let entry = g.branch(first.block)?;
    need(
        entry.target == guard.block
            && g.transported(entry, EntityId::Loan(source))? == EntityId::Loan(hs)
            && g.transported(entry, result(first)?)? == EntityId::Value(hc)
            && g.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc),
        "all provider entry/header/body columns",
        (entry, when_true),
    )?;
    let header_owner = g.transported(entry, EntityId::Value(owner))?;
    let body_owner = g.transported(when_true, header_owner)?;
    let exhausted_owner = g.transported(when_false, header_owner)?;
    need(
        [header_owner, body_owner, exhausted_owner]
            .iter()
            .all(|entity| owners.contains(entity)),
        "actual owner columns retain original factory anchor",
        (header_owner, body_owner, exhausted_owner),
    )?;
    let mut terminal_owners = BTreeMap::from([(when_false.target, exhausted_owner)]);
    if terminal {
        terminal_owners.insert(key.block, body_owner);
    }
    if terminal {
        g.incoming_from(guard.block, &[first.block])?;
    } else {
        g.incoming_from(guard.block, &[first.block, key.block])?;
    }
    g.incoming_from(key.block, &[guard.block])?;
    g.incoming_from(when_false.target, &[guard.block])?;
    let body_span = parsed.ast().statements().get(body).unwrap().span();
    let transfer = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, n)| {
            (inside(n.span(), body_span)
                && matches!(
                    n.payload(),
                    Expression::Continue { .. }
                        | Expression::Break { .. }
                        | Expression::Return { .. }
                ))
            .then_some(n.span())
        })
        .collect::<Vec<_>>();
    need(
        transfer.len() == usize::from(exit != 0),
        "actual original direct transfer count",
        &transfer,
    )?;
    let next_span = if continued { transfer[0] } else { span };
    let next = g
        .instructions
        .iter()
        .copied()
        .filter(|i| {
            i.origin == Origin::Source(next_span)
                && matches!(i.operation, Operation::MapIterationNext { .. })
        })
        .collect::<Vec<_>>();
    need(
        next.len() == usize::from(!terminal),
        "actual Next exists exactly for normal/Continue",
        &next,
    )?;
    let xs = g.transported(when_false, EntityId::Loan(hs))?;
    let xc = g.transported(when_false, EntityId::Value(hc))?;
    let leases = family(g, EntityId::Loan(source))?;
    for &lease in &leases {
        uses.protect(lease);
    }
    let mut ends = vec![];
    for i in &g.instructions {
        if matches!(i.operation,Operation::BorrowEnd {loan} if leases.contains(&EntityId::Loan(loan)))
        {
            ends.push(*i);
        }
    }
    need(
        ends.len() == if terminal { 2 } else { 1 },
        "exact provider source End count",
        &ends,
    )?;
    let exhausted = one(
        ends.iter().copied().filter(|i| {
            i.operation
                == match xs {
                    EntityId::Loan(loan) => Operation::BorrowEnd { loan },
                    _ => unreachable!(),
                }
        }),
        "actual false-edge source End",
    )?;
    need(
        exhausted.block == when_false.target,
        "false source ends in actual exhausted block",
        exhausted,
    )?;
    for &end in &ends {
        if let Operation::BorrowEnd { loan } = end.operation {
            uses.permit(EntityId::Loan(loan), end);
        }
    }
    uses.permit(EntityId::Loan(source), first);
    uses.permit(EntityId::Loan(hs), guard);
    for i in [key, val] {
        uses.permit(EntityId::Loan(bs), i);
    }
    let cursors = BTreeSet::from([result(first)?, EntityId::Value(hc), EntityId::Value(bc), xc]);
    for &cursor in &cursors {
        uses.protect(cursor);
        need(g.function.entity(cursor).is_some_and(|e|matches!(e.ty,EntityType::Value(t) if module.type_kind(t)==Some(&SsaTypeKind::Integer {bits:64,signed:false}))),"actual unsigned provider cursor",cursor)?;
    }
    uses.permit(EntityId::Value(hc), guard);
    for i in [key, val] {
        uses.permit(EntityId::Value(bc), i);
    }
    if !terminal {
        let next = next[0];
        let back = g.branch(key.block)?;
        need(
            next.block == key.block
                && next.operation
                    == Operation::MapIterationNext {
                        source: bs,
                        cursor: bc,
                    }
                && back.target == guard.block
                && g.argument(back, header_owner)? == body_owner
                && g.argument(back, EntityId::Loan(hs))? == EntityId::Loan(bs)
                && g.argument(back, EntityId::Value(hc))? == result(next)?,
            "actual normal/Continue full backedge",
            (next, back),
        )?;
        uses.protect(result(next)?);
        uses.permit(EntityId::Loan(bs), next);
        uses.permit(EntityId::Value(bc), next);
    }
    let read_decl=one(parsed.roots().iter().filter_map(|id|{let n=parsed.ast().items().get(*id).ok()?;matches!(n.payload(),Item::Function{name:NameMarker::Present(name),..} if sources.slice(*name)==Ok("read")).then_some(n.span())}),"original read declaration")?;
    let reader = one(
        module
            .functions
            .iter()
            .filter(|f| f.origin == Origin::Source(read_decl)),
        "actual read callee",
    )?;
    let read=one(g.instructions.iter().copied().filter(|i|i.block==key.block && matches!(&i.operation,Operation::DirectCall {callee,receiver:None,arguments} if *callee==reader.id && arguments.as_slice()==[result(key).unwrap(),result(val).unwrap()])),"actual K/V read selected callee/arguments")?;
    let mut component_ends = vec![];
    for (index, component) in [key, val].into_iter().enumerate() {
        let entity = result(component)?;
        let ty = g.function.entity(entity).unwrap().ty;
        need(
            matches!(ty,EntityType::Loan {kind:LoanKind::Shared,target} if module.type_kind(target)==Some(&SsaTypeKind::StringOwner))
                && reader
                    .block(reader.entry_block().unwrap())
                    .unwrap()
                    .parameters
                    .get(index)
                    .and_then(|p| reader.entity(*p))
                    .is_some_and(|e| e.ty == ty),
            "K/V original Shared String reader ABI",
            component,
        )?;
        let end=one(g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::BorrowEnd {loan} if EntityId::Loan(loan)==entity)),"unique actual K/V End")?;
        need(end.block == key.block, "body ends own K/V", end)?;
        uses.protect(entity);
        uses.permit(entity, read);
        uses.permit(entity, end);
        component_ends.push(end);
    }
    need(
        g.position(read)? < g.position(component_ends[1])?
            && g.position(component_ends[1])? < g.position(component_ends[0])?,
        "actual read/VEnd/KEnd order",
        &component_ends,
    )?;
    let local_drop = guard_drop(
        g,
        module,
        sources,
        parsed,
        body,
        (read, component_ends[1]),
        uses,
    )?;
    if terminal {
        let end = one(
            ends.iter().copied().filter(|i| i.block == key.block),
            "actual Break source End",
        )?;
        need(
            g.position(component_ends[0])? < g.position(end)?,
            "Break KEnd before EndSource",
            end,
        )?;
        if broken {
            need(
                g.branch(key.block)?.target == g.branch(when_false.target)?.target,
                "actual Break/exhaustion continuation merge",
                span,
            )?;
            g.incoming_from(g.branch(key.block)?.target, &[key.block, when_false.target])?;
        }
    } else {
        need(
            g.position(component_ends[0])? < g.position(next[0])?,
            "KEnd before actual Next",
            next[0],
        )?;
    }
    Ok(Evidence {
        body: key.block,
        exhausted: when_false.target,
        ends,
        terminal_owners,
        key,
        val,
        read,
        local_drop,
    })
}

fn guard_drop<'a>(
    g: &Graph<'a>,
    module: &Module,
    sources: &SourceMap,
    parsed: &ParsedFile,
    body: lang_frontend::ast::StatementId,
    read_end: (&Instruction, &Instruction),
    uses: &mut Uses,
) -> Result<&'a Instruction, Failure> {
    let (read, value_end) = read_end;
    let span = parsed.ast().statements().get(body).unwrap().span();
    let (name, initializer) = one(
        parsed.ast().items().iter().filter_map(|(_, node)| {
            let Item::Variable {
                name: NameMarker::Present(name),
                initializer,
                ..
            } = node.payload()
            else {
                return None;
            };
            (inside(node.span(), span) && sources.slice(*name) == Ok("local"))
                .then_some((*name, *initializer))
        }),
        "original body Guard binding",
    )?;
    let origin = parsed.ast().expressions().get(initializer).unwrap().span();
    need(
        sources.slice(origin) == Ok("Guard()"),
        "original Guard construction",
        origin,
    )?;
    let allocation = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(origin)
                && matches!(i.operation, Operation::HeapAllocate { .. })
        }),
        "actual original Guard allocation",
    )?;
    let Operation::HeapAllocate { owner: ty, payload } = allocation.operation else {
        unreachable!()
    };
    let class = one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            let Item::Classifier(c) = n.payload() else {
                return None;
            };
            matches!(c.name,NameMarker::Present(name) if sources.slice(name)==Ok("Guard"))
                .then_some(c)
        }),
        "original Guard class",
    )?;
    let deinit = one(
        class
            .body
            .as_ref()
            .unwrap()
            .members
            .iter()
            .filter_map(|id| parsed.ast().items().get(*id).ok())
            .filter(|n| matches!(n.payload(), Item::Deinit { .. })),
        "original Guard deinit",
    )?;
    need(
        module
            .deinits
            .get(&ty)
            .and_then(|id| module.function(*id))
            .is_some_and(|f| f.origin == Origin::Source(deinit.span())),
        "actual Guard type/deinit identity",
        ty,
    )?;
    let payload = g.definition(EntityId::Value(payload))?;
    need(
        payload.origin == Origin::Source(origin)
            && payload.block == read.block
            && matches!(&payload.operation,Operation::AggregateConstruct {fields,..} if fields.is_empty()),
        "actual Guard empty payload",
        payload,
    )?;
    let owner = value(allocation)?;
    let drop = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::Drop { owner }),
        "exact actual local Guard Drop",
    )?;
    need(
        allocation.block == read.block
            && drop.block == read.block
            && drop.origin == Origin::Source(name)
            && g.position(allocation)? < g.position(read)?
            && g.position(read)? < g.position(drop)?
            && g.position(drop)? < g.position(value_end)?,
        "actual Guard construct/read/Drop/VEnd order",
        drop,
    )?;
    uses.protect(EntityId::Value(owner));
    uses.permit(EntityId::Value(owner), drop);
    Ok(drop)
}
