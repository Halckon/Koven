//! Per-original-For proof with represented Resource components and full current columns.
use super::*;
pub(super) struct Evidence<'p> {
    pub current: EntityId,
    pub ends: Vec<&'p Instruction>,
}
pub(super) fn check<'a, 'p>(
    q: &Query<'a, 'p>,
    site: &LoopSite,
    seed: EntityId,
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
            "original scoped provider operation",
        )
    };
    let first = op(|o| matches!(o, Operation::MapIterationFirst { .. }))?;
    let has = op(|o| matches!(o, Operation::MapIterationHasNext { .. }))?;
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
        return Err(failure("actual source Shared loan", birth));
    };
    let root = g.definition(EntityId::Place(place))?;
    need(
        matches!(root.operation,Operation::RootPlace {owner} if EntityId::Value(owner)==seed)
            && owners.contains(&seed)
            && root.block == first.block
            && birth.block == first.block
            && g.position(root)? < g.position(birth)?
            && g.position(birth)? < g.position(first)?,
        "each For first use consumes exact prior current owner",
        root,
    )?;
    uses.permit(seed, root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), birth);
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
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(has.block)?.kind
    else {
        return Err(failure("real HasNext Conditional", has));
    };
    need(
        *condition == value(has)?
            && when_true.target == key.block
            && key.block == val.block
            && val.operation
                == Operation::MapIterationValueLoan {
                    source: bs,
                    cursor: bc,
                },
        "real conditional and component source/cursor identity",
        (has, key, val),
    )?;
    let entry = g.branch(first.block)?;
    need(
        entry.target == has.block
            && g.transported(entry, EntityId::Loan(source))? == EntityId::Loan(hs)
            && g.transported(entry, result(first)?)? == EntityId::Value(hc)
            && g.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc),
        "full Entry/Header/body source and cursor columns",
        (entry, when_true),
    )?;
    let ho = g.transported(entry, seed)?;
    let bo = g.transported(when_true, ho)?;
    let xo = g.transported(when_false, ho)?;
    need(
        [ho, bo, xo].iter().all(|e| owners.contains(e)),
        "current owner across every original provider boundary",
        (ho, bo, xo),
    )?;
    let broken = site.exit == 2;
    g.incoming_from(
        has.block,
        &if broken {
            vec![first.block]
        } else {
            vec![first.block, key.block]
        },
    )?;
    g.incoming_from(key.block, &[has.block])?;
    g.incoming_from(when_false.target, &[has.block])?;
    let body = q.parsed.ast().statements().get(site.body).unwrap().span();
    let transfers = q
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, n)| {
            (anchors::inside(n.span(), body)
                && matches!(
                    n.payload(),
                    Expression::Continue { .. }
                        | Expression::Break { .. }
                        | Expression::Return { .. }
                ))
            .then_some(n)
        })
        .collect::<Vec<_>>();
    need(
        transfers.len() == usize::from(site.exit != 0)
            && transfers.iter().all(|n| {
                matches!(
                    (site.exit, n.payload()),
                    (1, Expression::Continue { .. }) | (2, Expression::Break { .. })
                )
            }),
        "actual original body transfer kind/count",
        &transfers,
    )?;
    let next_origin = if site.exit == 1 {
        transfers[0].span()
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
        "Next belongs only to this normal/Continue For",
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
                && back.target == has.block
                && g.argument(back, ho)? == bo
                && g.argument(back, EntityId::Loan(hs))? == EntityId::Loan(bs)
                && g.argument(back, EntityId::Value(hc))? == result(next[0])?,
            "all actual backedge roles use their current body columns",
            (next[0], back),
        )?;
        uses.protect(result(next[0])?);
        uses.permit(EntityId::Loan(bs), next[0]);
        uses.permit(EntityId::Value(bc), next[0]);
    }
    let xs = g.transported(when_false, EntityId::Loan(hs))?;
    let xc = g.transported(when_false, EntityId::Value(hc))?;
    let leases = anchors::family(g, EntityId::Loan(source))?;
    let ends=g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::BorrowEnd {loan} if leases.contains(&EntityId::Loan(loan)))).collect::<Vec<_>>();
    need(
        ends.len() == if broken { 2 } else { 1 },
        "all actual source End sites for this provider",
        &ends,
    )?;
    let exhausted = one(
        ends.iter().copied().filter(
            |i| matches!(i.operation,Operation::BorrowEnd {loan} if EntityId::Loan(loan)==xs),
        ),
        "original exhausted source End",
    )?;
    need(
        exhausted.block == when_false.target,
        "source End on exact false edge",
        exhausted,
    )?;
    let current = if broken {
        let end = one(
            ends.iter()
                .copied()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: bs }),
            "actual Break source End",
        )?;
        let yes = g.branch(key.block)?;
        let no = g.branch(when_false.target)?;
        need(
            end.block == key.block && yes.target == no.target,
            "Break and exhausted cleanup join",
            (end, yes, no),
        )?;
        g.incoming_from(yes.target, &[key.block, when_false.target])?;
        let merged = g.transported(yes, bo)?;
        need(
            g.argument(no, merged)? == xo,
            "both actual incoming current owners retained at Break merge",
            merged,
        )?;
        merged
    } else {
        xo
    };
    for &lease in &leases {
        uses.protect(lease);
    }
    for &end in &ends {
        if let Operation::BorrowEnd { loan } = end.operation {
            uses.permit(EntityId::Loan(loan), end);
        }
    }
    uses.permit(EntityId::Loan(source), first);
    uses.permit(EntityId::Loan(hs), has);
    uses.permit(EntityId::Value(hc), has);
    for component in [key, val] {
        uses.permit(EntityId::Loan(bs), component);
        uses.permit(EntityId::Value(bc), component);
    }
    let mut cursors =
        BTreeSet::from([result(first)?, EntityId::Value(hc), EntityId::Value(bc), xc]);
    if !broken {
        cursors.insert(result(next[0])?);
    }
    for &cursor in &cursors {
        uses.protect(cursor);
        need(g.function.entity(cursor).is_some_and(|e|matches!(e.ty,EntityType::Value(t) if q.module.type_kind(t)==Some(&SsaTypeKind::Integer {bits:64,signed:false}))),"actual unsigned cursor storage",cursor)?;
    }
    for (from, edge) in &g.edges {
        for e in &edge.arguments {
            if leases.contains(e) || cursors.contains(e) {
                need(
                    (*from == first.block && edge.target == has.block)
                        || (*from == has.block
                            && [key.block, when_false.target].contains(&edge.target))
                        || (!broken && *from == key.block && edge.target == has.block),
                    "source/cursor transport remains inside its own provider",
                    (from, edge, e),
                )?;
            }
        }
    }
    let reader = function(q.module, anchors::declaration(q.parsed, q.sources, "read")?)?;
    let read=one(g.instructions.iter().copied().filter(|i|i.block==key.block && matches!(&i.operation,Operation::DirectCall {callee,receiver:None,arguments} if *callee==reader.id && arguments.as_slice()==[result(key).unwrap(),result(val).unwrap()])),"actual read receives current K/V loans")?;
    let mut component_ends = Vec::new();
    for (component, target) in [(key, q.types.0), (val, q.types.1)] {
        let e = result(component)?;
        need(
            g.function.entity(e).unwrap().ty
                == EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                },
            "exact represented Resource component Shared type",
            component,
        )?;
        let end = one(
            g.instructions.iter().copied().filter(
                |i| matches!(i.operation,Operation::BorrowEnd {loan} if EntityId::Loan(loan)==e),
            ),
            "unique actual component End",
        )?;
        need(
            end.block == key.block,
            "component End belongs to actual body",
            end,
        )?;
        uses.protect(e);
        uses.permit(e, read);
        uses.permit(e, end);
        component_ends.push(end);
    }
    need(
        g.position(read)? < g.position(component_ends[1])?
            && g.position(component_ends[1])? < g.position(component_ends[0])?,
        "read then reverse V/K End",
        &component_ends,
    )?;
    guard(q, site, read, component_ends[1], uses)?;
    let terminal = if broken {
        one(
            ends.iter().copied().filter(|i| i.block == key.block),
            "actual Break terminal End",
        )?
    } else {
        next[0]
    };
    need(
        g.position(component_ends[0])? < g.position(terminal)?,
        "K End before Next or terminal source End",
        terminal,
    )?;
    let resource = match q.module.type_kind(q.types.1) {
        Some(SsaTypeKind::NullableHandle { inner }) => *inner,
        _ => q.types.1,
    };
    need(!g.instructions.iter().any(|i|matches!(i.operation,Operation::Drop {owner} if [q.types.1,resource].contains(&g.function.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()))),"borrowed Resource components never become dropped owners",g.function.id)?;
    Ok(Evidence { current, ends })
}

fn guard(
    q: &Query<'_, '_>,
    site: &LoopSite,
    read: &Instruction,
    value_end: &Instruction,
    uses: &mut Uses,
) -> Result<(), Failure> {
    let body = q.parsed.ast().statements().get(site.body).unwrap().span();
    let (name, init) = one(
        q.parsed
            .ast()
            .items()
            .iter()
            .filter_map(|(_, n)| match n.payload() {
                Item::Variable {
                    name: NameMarker::Present(name),
                    initializer,
                    ..
                } if anchors::inside(n.span(), body) && q.sources.slice(*name) == Ok("local") => {
                    Some((*name, *initializer))
                }
                _ => None,
            }),
        "original body Guard declaration",
    )?;
    let span = q.parsed.ast().expressions().get(init).unwrap().span();
    need(
        q.sources.slice(span) == Ok("Guard()"),
        "actual Guard initializer",
        span,
    )?;
    let alloc = one(
        q.g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(span)
                && matches!(i.operation, Operation::HeapAllocate { .. })
        }),
        "actual Guard heap allocation",
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
        "actual original Guard class",
    )?;
    let deinit = one(
        class
            .body
            .as_ref()
            .unwrap()
            .members
            .iter()
            .filter_map(|id| {
                let n = q.parsed.ast().items().get(*id).ok()?;
                matches!(n.payload(), Item::Deinit { .. }).then_some(n.span())
            }),
        "actual Guard deinit declaration",
    )?;
    need(
        q.module
            .deinits
            .get(&ty)
            .and_then(|id| q.module.function(*id))
            .is_some_and(|f| f.origin == Origin::Source(deinit)),
        "actual Guard type/deinit identity",
        ty,
    )?;
    let payload = q.g.definition(EntityId::Value(payload))?;
    need(
        payload.origin == Origin::Source(span)
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
        "actual Guard construct/read/Drop/VEnd ordering",
        drop,
    )?;
    uses.protect(EntityId::Value(owner));
    uses.permit(EntityId::Value(owner), drop);
    Ok(())
}
