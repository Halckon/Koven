//! One provider's actual columns, current owner, component loans and Guard cleanup.
use super::*;

pub(super) struct Evidence<'p> {
    pub ends: Vec<&'p Instruction>,
    pub current: EntityId,
}

pub(super) fn check<'a, 'p>(
    q: &Query<'a, 'p>,
    site: &LoopSite,
    expected_seed: EntityId,
    owners: &BTreeSet<EntityId>,
    uses: &mut Uses,
) -> Result<Evidence<'p>, Failure> {
    let g = q.g;
    let op = |predicate: fn(&Operation) -> bool| {
        one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.origin == Origin::Source(site.span) && predicate(&i.operation)),
            "local future original provider operation",
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
        return Err(failure("actual provider source loan", birth));
    };
    let root = g.definition(EntityId::Place(place))?;
    let Operation::RootPlace { owner } = root.operation else {
        return Err(failure("actual provider source owner", root));
    };
    need(
        owners.contains(&EntityId::Value(owner)) && EntityId::Value(owner) == expected_seed,
        "provider RootPlace uses exact prior continuation current owner",
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
        return Err(failure("actual HasNext Conditional", guard));
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
        "actual HasNext true edge/KV operands",
        (guard, key, val),
    )?;
    let entry = g.branch(first.block)?;
    need(
        entry.target == guard.block
            && g.transported(entry, EntityId::Loan(source))? == EntityId::Loan(hs)
            && g.transported(entry, result(first)?)? == EntityId::Value(hc)
            && g.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc),
        "actual source/cursor entry and true columns",
        (entry, when_true),
    )?;
    let ho = g.transported(entry, EntityId::Value(owner))?;
    let bo = g.transported(when_true, ho)?;
    let xo = g.transported(when_false, ho)?;
    need(
        [ho, bo, xo].iter().all(|owner| owners.contains(owner)),
        "current owner exact entry/header/body/false columns",
        (ho, bo, xo),
    )?;
    let broken = site.exit == 2;
    if broken {
        g.incoming_from(guard.block, &[first.block])?;
    } else {
        g.incoming_from(guard.block, &[first.block, key.block])?;
    }
    g.incoming_from(key.block, &[guard.block])?;
    g.incoming_from(when_false.target, &[guard.block])?;
    let body = q.parsed.ast().statements().get(site.body).unwrap().span();
    let transfer = q
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, n)| {
            (inside(n.span(), body)
                && match site.exit {
                    1 => matches!(n.payload(), Expression::Continue { .. }),
                    2 => matches!(n.payload(), Expression::Break { .. }),
                    _ => matches!(
                        n.payload(),
                        Expression::Continue { .. }
                            | Expression::Break { .. }
                            | Expression::Return { .. }
                    ),
                })
            .then_some(n.span())
        })
        .collect::<Vec<_>>();
    need(
        transfer.len() == usize::from(site.exit != 0),
        "actual original normal/Continue/Break expression",
        &transfer,
    )?;
    let next_origin = if site.exit == 1 {
        transfer[0]
    } else {
        site.span
    };
    let next = g
        .instructions
        .iter()
        .copied()
        .filter(|i| {
            i.origin == Origin::Source(next_origin)
                && matches!(i.operation, Operation::MapIterationNext { .. })
        })
        .collect::<Vec<_>>();
    need(
        next.len() == usize::from(!broken),
        "actual selected exit Next count",
        &next,
    )?;
    if !broken {
        let back = g.branch(key.block)?;
        need(
            next[0].block == key.block
                && next[0].operation
                    == Operation::MapIterationNext {
                        source: bs,
                        cursor: bc,
                    }
                && back.target == guard.block
                && g.argument(back, ho)? == bo
                && g.argument(back, EntityId::Loan(hs))? == EntityId::Loan(bs)
                && g.argument(back, EntityId::Value(hc))? == result(next[0])?,
            "current owner/source/cursor complete actual backedge",
            (next[0], back),
        )?;
        uses.protect(result(next[0])?);
        uses.permit(EntityId::Loan(bs), next[0]);
        uses.permit(EntityId::Value(bc), next[0]);
    }
    let xs = g.transported(when_false, EntityId::Loan(hs))?;
    let xc = g.transported(when_false, EntityId::Value(hc))?;
    let leases = family(g, EntityId::Loan(source))?;
    for &lease in &leases {
        uses.protect(lease);
    }
    let ends=g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::BorrowEnd {loan} if leases.contains(&EntityId::Loan(loan)))).collect::<Vec<_>>();
    need(
        ends.len() == if broken { 2 } else { 1 },
        "actual unique provider source cleanup sites",
        &ends,
    )?;
    let exhausted = one(
        ends.iter().copied().filter(
            |i| matches!(i.operation,Operation::BorrowEnd {loan} if EntityId::Loan(loan)==xs),
        ),
        "actual false source End",
    )?;
    need(
        exhausted.block == when_false.target,
        "false source End uses actual exhausted state",
        exhausted,
    )?;
    let mut current = xo;
    if broken {
        let body_end = one(
            ends.iter()
                .copied()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: bs }),
            "actual Break own source End",
        )?;
        need(
            body_end.block == key.block
                && g.branch(key.block)?.target == g.branch(when_false.target)?.target,
            "actual Break/exhaustion full merge",
            body_end,
        )?;
        let merge = g.branch(key.block)?.target;
        g.incoming_from(merge, &[key.block, when_false.target])?;
        let mo = g.transported(g.branch(key.block)?, bo)?;
        need(
            g.argument(g.branch(when_false.target)?, mo)? == xo,
            "actual Break merge keeps both current-owner columns",
            mo,
        )?;
        current = mo;
    }
    for &end in &ends {
        if let Operation::BorrowEnd { loan } = end.operation {
            uses.permit(EntityId::Loan(loan), end);
        }
    }
    uses.permit(EntityId::Loan(source), first);
    uses.permit(EntityId::Loan(hs), guard);
    uses.permit(EntityId::Value(hc), guard);
    for i in [key, val] {
        uses.permit(EntityId::Loan(bs), i);
        uses.permit(EntityId::Value(bc), i);
    }
    for cursor in [result(first)?, EntityId::Value(hc), EntityId::Value(bc), xc] {
        uses.protect(cursor);
        need(g.function.entity(cursor).is_some_and(|e|matches!(e.ty,EntityType::Value(t) if q.module.type_kind(t)==Some(&SsaTypeKind::Integer {bits:64,signed:false}))),"actual unsigned cursor storage",cursor)?;
    }
    let reader=one(q.module.functions.iter().filter(|f|matches!(f.origin,Origin::Source(span) if q.parsed.roots().iter().any(|id|q.parsed.ast().items().get(*id).is_ok_and(|n|n.span()==span && matches!(n.payload(),Item::Function {name:NameMarker::Present(name),..} if q.sources.slice(*name)==Ok("read")))))),"original actual reader")?;
    let read=one(g.instructions.iter().copied().filter(|i|i.block==key.block && matches!(&i.operation,Operation::DirectCall {callee,receiver:None,arguments} if *callee==reader.id && arguments.as_slice()==[result(key).unwrap(),result(val).unwrap()])),"actual reader selected K/V arguments")?;
    let mut component_ends = vec![];
    for (index, component) in [key, val].into_iter().enumerate() {
        let entity = result(component)?;
        let ty = g.function.entity(entity).unwrap().ty;
        need(
            matches!(ty,EntityType::Loan {kind:LoanKind::Shared,target} if q.module.type_kind(target)==Some(&SsaTypeKind::StringOwner))
                && reader
                    .block(reader.entry_block().unwrap())
                    .unwrap()
                    .parameters
                    .get(index)
                    .and_then(|p| reader.entity(*p))
                    .is_some_and(|e| e.ty == ty),
            "actual component and reader Shared String ABI",
            component,
        )?;
        let end=one(g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::BorrowEnd {loan} if EntityId::Loan(loan)==entity)),"actual unique component End")?;
        need(
            end.block == key.block,
            "actual component ends in its body",
            end,
        )?;
        uses.protect(entity);
        uses.permit(entity, read);
        uses.permit(entity, end);
        component_ends.push(end);
    }
    need(
        g.position(read)? < g.position(component_ends[1])?
            && g.position(component_ends[1])? < g.position(component_ends[0])?,
        "actual read then reverse V/K cleanup",
        &component_ends,
    )?;
    local_guard(q, site, read, component_ends[1], uses)?;
    if broken {
        let end = one(
            ends.iter().copied().filter(|i| i.block == key.block),
            "actual Break EndSource",
        )?;
        need(
            g.position(component_ends[0])? < g.position(end)?,
            "K cleanup before Break EndSource",
            end,
        )?;
    } else {
        need(
            g.position(component_ends[0])? < g.position(next[0])?,
            "K cleanup before actual Next",
            next[0],
        )?;
    }
    Ok(Evidence { ends, current })
}

fn local_guard(
    q: &Query<'_, '_>,
    site: &LoopSite,
    read: &Instruction,
    value_end: &Instruction,
    uses: &mut Uses,
) -> Result<(), Failure> {
    let body = q.parsed.ast().statements().get(site.body).unwrap().span();
    let (name, init) = one(
        q.parsed.ast().items().iter().filter_map(|(_, n)| {
            let Item::Variable {
                name: NameMarker::Present(name),
                initializer,
                ..
            } = n.payload()
            else {
                return None;
            };
            (inside(n.span(), body) && q.sources.slice(*name) == Ok("local"))
                .then_some((*name, *initializer))
        }),
        "original local Guard binding",
    )?;
    let origin = q.parsed.ast().expressions().get(init).unwrap().span();
    need(
        q.sources.slice(origin) == Ok("Guard()"),
        "original Guard initializer",
        origin,
    )?;
    let alloc = one(
        q.g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(origin)
                && matches!(i.operation, Operation::HeapAllocate { .. })
        }),
        "actual local Guard allocation",
    )?;
    let Operation::HeapAllocate { owner: ty, payload } = alloc.operation else {
        unreachable!()
    };
    let class = one(
        q.parsed.roots().iter().filter_map(|id| {
            let n = q.parsed.ast().items().get(*id).ok()?;
            let Item::Classifier(c) = n.payload() else {
                return None;
            };
            matches!(c.name,NameMarker::Present(name) if q.sources.slice(name)==Ok("Guard"))
                .then_some(c)
        }),
        "original Guard class",
    )?;
    let member = one(
        class
            .body
            .as_ref()
            .unwrap()
            .members
            .iter()
            .filter_map(|id| q.parsed.ast().items().get(*id).ok())
            .filter(|n| matches!(n.payload(), Item::Deinit { .. })),
        "original Guard deinit",
    )?;
    need(
        q.module
            .deinits
            .get(&ty)
            .and_then(|id| q.module.function(*id))
            .is_some_and(|f| f.origin == Origin::Source(member.span())),
        "actual Guard type/deinit identity",
        ty,
    )?;
    let payload = q.g.definition(EntityId::Value(payload))?;
    need(
        payload.origin == Origin::Source(origin)
            && matches!(&payload.operation,Operation::AggregateConstruct {fields,..} if fields.is_empty()),
        "actual Guard empty payload",
        payload,
    )?;
    let owner = value(alloc)?;
    let drop = one(
        q.g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::Drop { owner }),
        "actual unique Guard Drop",
    )?;
    need(
        alloc.block == read.block
            && drop.block == read.block
            && drop.origin == Origin::Source(name)
            && q.g.position(alloc)? < q.g.position(read)?
            && q.g.position(read)? < q.g.position(drop)?
            && q.g.position(drop)? < q.g.position(value_end)?,
        "actual construct/read/Guard Drop/VEnd order",
        drop,
    )?;
    uses.protect(EntityId::Value(owner));
    uses.permit(EntityId::Value(owner), drop);
    Ok(())
}
