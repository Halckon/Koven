//! One actual epoch provider with complete current-owner/source/cursor columns.
use super::*;
pub(super) struct Evidence<'a> {
    pub first: &'a Instruction,
    pub end: &'a Instruction,
    pub exhausted_owner: EntityId,
}
pub(super) fn check<'a>(
    g: &Graph<'a>,
    module: &Module,
    (sources, parsed): (&SourceMap, &ParsedFile),
    l: &ast::Loop,
    anchor: EntityId,
    owners: &BTreeSet<EntityId>,
    uses: &mut Uses,
) -> Result<Evidence<'a>, Failure> {
    let op = |p: fn(&Operation) -> bool| {
        one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.origin == Origin::Source(l.span) && p(&i.operation)),
            "actual epoch For operation",
        )
    };
    let first = op(|o| matches!(o, Operation::MapIterationFirst { .. }))?;
    let has = op(|o| matches!(o, Operation::MapIterationHasNext { .. }))?;
    let key = op(|o| matches!(o, Operation::MapIterationKeyLoan { .. }))?;
    let val = op(|o| matches!(o, Operation::MapIterationValueLoan { .. }))?;
    let next = op(|o| matches!(o, Operation::MapIterationNext { .. }))?;
    let Operation::MapIterationFirst { source } = first.operation else {
        unreachable!()
    };
    let birth = g.definition(EntityId::Loan(source))?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = birth.operation
    else {
        return Err(failure("epoch actual source lease", birth));
    };
    let root = g.definition(EntityId::Place(place))?;
    need(
        root.operation
            == Operation::RootPlace {
                owner: match anchor {
                    EntityId::Value(v) => v,
                    _ => return Err(failure("epoch Value anchor", anchor)),
                },
            }
            && root.block == first.block
            && birth.block == first.block
            && g.position(root)? < g.position(birth)?
            && g.position(birth)? < g.position(first)?,
        "First borrows current complete factory RHS, never old declaration epoch",
        root,
    )?;
    uses.permit(anchor, root);
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
        return Err(failure("epoch guard CFG", has));
    };
    need(
        *condition == value(has)?
            && when_true.target == key.block
            && val.block == key.block
            && val.operation
                == Operation::MapIterationValueLoan {
                    source: bs,
                    cursor: bc,
                },
        "actual epoch guard and K/V loan operands",
        (has, key, val),
    )?;
    let entry = g.branch(first.block)?;
    need(
        entry.target == has.block
            && g.transported(entry, EntityId::Loan(source))? == EntityId::Loan(hs)
            && g.transported(entry, result(first)?)? == EntityId::Value(hc)
            && g.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc),
        "actual epoch full source/cursor incoming",
        entry,
    )?;
    let ho = g.transported(entry, anchor)?;
    let bo = g.transported(when_true, ho)?;
    let xo = g.transported(when_false, ho)?;
    need(
        [ho, bo, xo].iter().all(|e| owners.contains(e)),
        "actual epoch owner incoming columns",
        (ho, bo, xo),
    )?;
    g.incoming_from(has.block, &[first.block, key.block])?;
    g.incoming_from(key.block, &[has.block])?;
    g.incoming_from(when_false.target, &[has.block])?;
    let back = g.branch(key.block)?;
    need(
        next.block == key.block
            && next.operation
                == Operation::MapIterationNext {
                    source: bs,
                    cursor: bc,
                }
            && back.target == has.block
            && g.argument(back, ho)? == bo
            && g.argument(back, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.argument(back, EntityId::Value(hc))? == result(next)?,
        "actual epoch complete owner/source/cursor backedge",
        (next, back),
    )?;
    let xs = g.transported(when_false, EntityId::Loan(hs))?;
    let xc = g.transported(when_false, EntityId::Value(hc))?;
    let leases = family(g, EntityId::Loan(source))?;
    let ends=g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::BorrowEnd{loan} if leases.contains(&EntityId::Loan(loan)))).collect::<Vec<_>>();
    let end = one(ends.into_iter(), "only epoch exhaustion ends its source")?;
    need(
        matches!(end.operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==xs)
            && end.block == when_false.target,
        "epoch ends exact exhaustion source column",
        end,
    )?;
    for &e in &leases {
        uses.protect(e);
    }
    uses.permit(EntityId::Loan(source), first);
    uses.permit(EntityId::Loan(hs), has);
    for i in [key, val, next] {
        uses.permit(EntityId::Loan(bs), i);
    }
    uses.permit(xs, end);
    let cursors = [
        result(first)?,
        EntityId::Value(hc),
        EntityId::Value(bc),
        result(next)?,
        xc,
    ];
    for &e in &cursors {
        uses.protect(e);
        need(g.function.entity(e).is_some_and(|d|matches!(d.ty,EntityType::Value(t) if module.type_kind(t)==Some(&SsaTypeKind::Integer{bits:64,signed:false}))),"epoch exact cursor storage",e)?;
    }
    uses.permit(EntityId::Value(hc), has);
    for i in [key, val, next] {
        uses.permit(EntityId::Value(bc), i);
    }
    for (from, edge) in &g.edges {
        for e in &edge.arguments {
            if leases.contains(e) || cursors.contains(e) {
                need(
                    (*from == first.block && edge.target == has.block)
                        || (*from == has.block
                            && [key.block, when_false.target].contains(&edge.target))
                        || (*from == key.block && edge.target == has.block),
                    "epoch provider state never survives cleanup or crosses other For",
                    (from, edge, e),
                )?;
            }
        }
    }
    let reader = function(module, ast::declaration(sources, parsed, "read")?)?;
    let read=one(g.instructions.iter().copied().filter(|i|i.block==key.block && matches!(&i.operation,Operation::DirectCall{callee,receiver:None,arguments} if *callee==reader.id && arguments.as_slice()==[result(key).unwrap(),result(val).unwrap()])),"actual epoch selected reader and current K/V operands")?;
    let (_, kt, vt) = module
        .map_container(g.function.entity(anchor).unwrap().ty.semantic_type())
        .ok_or_else(|| failure("epoch map storage", anchor))?;
    let mut component_ends = Vec::new();
    for (index, (component, target)) in [(key, kt), (val, vt)].into_iter().enumerate() {
        let e = result(component)?;
        let ty = EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        };
        need(
            g.function.entity(e).unwrap().ty == ty
                && reader
                    .block(reader.entry_block().unwrap())
                    .unwrap()
                    .parameters
                    .get(index)
                    .and_then(|p| reader.entity(*p))
                    .is_some_and(|p| p.ty == ty),
            "epoch exact represented Shared component ABI",
            component,
        )?;
        let ending = one(
            g.instructions.iter().copied().filter(|i| {
                i.operation
                    == Operation::BorrowEnd {
                        loan: loan(component).unwrap(),
                    }
            }),
            "epoch unique component End",
        )?;
        need(
            ending.block == key.block,
            "epoch component End actual body",
            ending,
        )?;
        uses.protect(e);
        uses.permit(e, read);
        uses.permit(e, ending);
        component_ends.push(ending);
    }
    need(
        g.position(read)? < g.position(component_ends[1])?
            && g.position(component_ends[1])? < g.position(component_ends[0])?
            && g.position(component_ends[0])? < g.position(next)?,
        "epoch read/VEnd/KEnd/Next order",
        &component_ends,
    )?;
    guard(
        g,
        module,
        (sources, parsed),
        l,
        read,
        component_ends[1],
        uses,
    )?;
    let inner = match module.type_kind(vt) {
        Some(SsaTypeKind::NullableHandle { inner }) => *inner,
        _ => vt,
    };
    // Pure String temporaries printed as markers are unrelated to the borrowed Map
    // component; protecting exact component loans forbids owner extraction already.
    if module.type_kind(inner) != Some(&SsaTypeKind::StringOwner) {
        need(!g.instructions.iter().any(|i|matches!(i.operation,Operation::Drop{owner} if [vt,inner].contains(&g.function.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()))),"no borrowed Resource component owner Drop",g.function.id)?;
    }
    Ok(Evidence {
        first,
        end,
        exhausted_owner: xo,
    })
}
fn guard(
    g: &Graph<'_>,
    module: &Module,
    (sources, parsed): (&SourceMap, &ParsedFile),
    l: &ast::Loop,
    read: &Instruction,
    vend: &Instruction,
    uses: &mut Uses,
) -> Result<(), Failure> {
    let body = parsed.ast().statements().get(l.body).unwrap().span();
    let (name, init) = one(
        parsed
            .ast()
            .items()
            .iter()
            .filter_map(|(_, n)| match n.payload() {
                Item::Variable {
                    name: NameMarker::Present(name),
                    initializer,
                    ..
                } if ast::inside(n.span(), body) && sources.slice(*name) == Ok("local") => {
                    Some((*name, *initializer))
                }
                _ => None,
            }),
        "epoch original Guard local",
    )?;
    let origin = ast::expression(parsed, init);
    need(
        sources.slice(origin) == Ok("Guard()"),
        "epoch original Guard constructor",
        origin,
    )?;
    let allocation = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(origin)
                && matches!(i.operation, Operation::HeapAllocate { .. })
        }),
        "epoch actual Guard allocation",
    )?;
    let Operation::HeapAllocate { owner: ty, payload } = allocation.operation else {
        unreachable!()
    };
    let payload = g.definition(EntityId::Value(payload))?;
    need(
        payload.origin == Origin::Source(origin)
            && matches!(&payload.operation,Operation::AggregateConstruct{fields,..} if fields.is_empty()),
        "epoch actual empty Guard payload",
        payload,
    )?;
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
            .filter_map(|id| {
                let n = parsed.ast().items().get(*id).ok()?;
                matches!(n.payload(), Item::Deinit { .. }).then_some(n.span())
            }),
        "original Guard deinit",
    )?;
    need(
        module
            .deinits
            .get(&ty)
            .and_then(|id| module.function(*id))
            .is_some_and(|f| f.origin == Origin::Source(deinit)),
        "epoch Guard selects actual original deinit",
        ty,
    )?;
    let owner = value(allocation)?;
    let drop = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::Drop { owner }),
        "epoch unique Guard Drop",
    )?;
    need(
        allocation.block == read.block
            && drop.block == read.block
            && drop.origin == Origin::Source(name)
            && g.position(allocation)? < g.position(read)?
            && g.position(read)? < g.position(drop)?
            && g.position(drop)? < g.position(vend)?,
        "epoch Guard allocation/read/Drop before component cleanup",
        drop,
    )?;
    uses.protect(EntityId::Value(owner));
    uses.permit(EntityId::Value(owner), drop);
    Ok(())
}
