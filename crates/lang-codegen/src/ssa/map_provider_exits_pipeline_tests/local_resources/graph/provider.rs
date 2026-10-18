//! Actual provider graph and Guard cleanup; no production proof helpers are consulted.
use super::*;
pub(super) struct Evidence<'a> {
    pub first: &'a Instruction,
    pub body: BlockId,
    pub body_owner: EntityId,
    pub normal_owner: EntityId,
    pub ends: Vec<&'a Instruction>,
    pub key: &'a Instruction,
    pub val: &'a Instruction,
    pub read: &'a Instruction,
    pub guard_drop: &'a Instruction,
}
#[allow(clippy::too_many_arguments)]
pub(super) fn check<'a>(
    case: &Case,
    g: &Graph<'a>,
    module: &Module,
    sources: &SourceMap,
    parsed: &ParsedFile,
    site: &Site,
    owners: &BTreeSet<EntityId>,
    types: (SsaTypeId, SsaTypeId),
    uses: &mut Uses,
    unit: bool,
) -> Result<Evidence<'a>, Failure> {
    let op = |p: fn(&Operation) -> bool| {
        one(
            g.instructions.iter().copied().filter(|i| p(&i.operation)),
            "actual unique provider operation",
        )
    };
    let first = op(|o| matches!(o, Operation::MapIterationFirst { .. }))?;
    let has = op(|o| matches!(o, Operation::MapIterationHasNext { .. }))?;
    let key = op(|o| matches!(o, Operation::MapIterationKeyLoan { .. }))?;
    let val = op(|o| matches!(o, Operation::MapIterationValueLoan { .. }))?;
    need(
        [first, has, key, val]
            .iter()
            .all(|i| i.origin == Origin::Source(site.for_span)),
        "actual For provider origins",
        site.for_span,
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
        return Err(failure("owned source lease birth", birth));
    };
    let root = g.definition(EntityId::Place(place))?;
    let Operation::RootPlace { owner } = root.operation else {
        return Err(failure("source actual RootPlace", root));
    };
    need(
        owners.contains(&EntityId::Value(owner))
            && root.block == first.block
            && birth.block == first.block
            && g.position(root)? < g.position(birth)?
            && g.position(birth)? < g.position(first)?,
        "source borrows actual current factory owner",
        root,
    )?;
    uses.permit(EntityId::Value(owner), root);
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
        return Err(failure("actual HasNext condition", has));
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
        "actual guard and K/V provider operands",
        (has, key, val),
    )?;
    let entry = g.branch(first.block)?;
    need(
        entry.target == has.block
            && g.transported(entry, EntityId::Loan(source))? == EntityId::Loan(hs)
            && g.transported(entry, result(first)?)? == EntityId::Value(hc)
            && g.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc),
        "actual provider incoming columns",
        (entry, when_true),
    )?;
    let ho = g.transported(entry, EntityId::Value(owner))?;
    let bo = g.transported(when_true, ho)?;
    let xo = g.transported(when_false, ho)?;
    need(
        [ho, bo, xo].iter().all(|v| owners.contains(v)),
        "every current owner column retains immutable factory origin",
        (ho, bo, xo),
    )?;
    g.incoming_from(first.block, &[])?;
    let header_inputs = if case.exit < 2 {
        vec![first.block, key.block]
    } else {
        vec![first.block]
    };
    g.incoming_from(has.block, &header_inputs)?;
    g.incoming_from(key.block, &[has.block])?;
    g.incoming_from(when_false.target, &[has.block])?;
    let body_span = parsed.ast().statements().get(site.body).unwrap().span();
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
            .then_some(n)
        })
        .collect::<Vec<_>>();
    need(
        transfer.len() == usize::from(case.exit != 0),
        "actual original body transfer count",
        &transfer,
    )?;
    let body_origin = if case.exit == 0 {
        site.for_span
    } else {
        transfer[0].span()
    };
    if case.exit != 0 {
        need(
            matches!(
                (case.exit, transfer[0].payload()),
                (1, Expression::Continue { .. })
                    | (2, Expression::Break { .. })
                    | (3, Expression::Return { .. })
            ),
            "actual transfer kind",
            transfer[0],
        )?;
    }
    // Unit cleanup is emitted by the original For action dispatcher; single
    // cleanup uses the original transfer expression. Next retains its transfer span.
    let cleanup_origin = if unit { site.for_span } else { body_origin };
    let mut normal_owner = xo;
    let nexts = g
        .instructions
        .iter()
        .copied()
        .filter(|i| matches!(i.operation, Operation::MapIterationNext { .. }))
        .collect::<Vec<_>>();
    need(
        nexts.len() == usize::from(case.exit < 2),
        "Next only for normal and Continue",
        &nexts,
    )?;
    let xs = g.transported(when_false, EntityId::Loan(hs))?;
    let xc = g.transported(when_false, EntityId::Value(hc))?;
    let leases = family(g, EntityId::Loan(source))?;
    let ends=g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::BorrowEnd{loan} if leases.contains(&EntityId::Loan(loan)))).collect::<Vec<_>>();
    need(
        ends.len() == if case.exit >= 2 { 2 } else { 1 },
        "one source End per actual terminal edge",
        &ends,
    )?;
    let exhausted = one(
        ends.iter().copied().filter(
            |i| matches!(i.operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==xs),
        ),
        "actual false-edge source End",
    )?;
    need(
        exhausted.block == when_false.target,
        "false edge ends only its current source",
        exhausted,
    )?;
    for &lease in &leases {
        uses.protect(lease);
    }
    uses.permit(EntityId::Loan(source), first);
    uses.permit(EntityId::Loan(hs), has);
    for component in [key, val] {
        uses.permit(EntityId::Loan(bs), component);
    }
    for end in &ends {
        if let Operation::BorrowEnd { loan } = end.operation {
            uses.permit(EntityId::Loan(loan), end);
        }
    }
    let mut cursors =
        BTreeSet::from([result(first)?, EntityId::Value(hc), EntityId::Value(bc), xc]);
    for &cursor in &cursors {
        uses.protect(cursor);
        need(g.function.entity(cursor).is_some_and(|e|matches!(e.ty,EntityType::Value(t) if module.type_kind(t)==Some(&SsaTypeKind::Integer{bits:64,signed:false}))),"actual provider cursor storage",cursor)?;
    }
    uses.permit(EntityId::Value(hc), has);
    for component in [key, val] {
        uses.permit(EntityId::Value(bc), component);
    }
    if case.exit < 2 {
        let next = nexts[0];
        let back = g.branch(key.block)?;
        need(
            next.block == key.block
                && next.origin == Origin::Source(body_origin)
                && next.operation
                    == Operation::MapIterationNext {
                        source: bs,
                        cursor: bc,
                    }
                && back.target == has.block
                && g.argument(back, ho)? == bo
                && g.argument(back, EntityId::Loan(hs))? == EntityId::Loan(bs)
                && g.argument(back, EntityId::Value(hc))? == result(next)?,
            "normal/Continue exact all-role backedge",
            (next, back),
        )?;
        uses.protect(result(next)?);
        cursors.insert(result(next)?);
        uses.permit(EntityId::Loan(bs), next);
        uses.permit(EntityId::Value(bc), next);
    } else if case.exit == 2 {
        let merge = g.branch(key.block)?.target;
        need(
            g.branch(when_false.target)?.target == merge,
            "Break and exhaustion share continuation",
            merge,
        )?;
        g.incoming_from(merge, &[key.block, when_false.target])?;
        normal_owner = g.transported(g.branch(key.block)?, bo)?;
        need(
            g.argument(g.branch(when_false.target)?, normal_owner)? == xo,
            "Break merge column receives exact body and exhaustion current owners",
            (normal_owner, bo, xo),
        )?;
    }
    for (from, edge) in &g.edges {
        for entity in &edge.arguments {
            if leases.contains(entity) || cursors.contains(entity) {
                need(
                    (*from == first.block && edge.target == has.block)
                        || (*from == has.block
                            && [key.block, when_false.target].contains(&edge.target))
                        || (case.exit < 2 && *from == key.block && edge.target == has.block),
                    "provider state only crosses its exact live edges",
                    (from, edge, entity),
                )?;
            }
        }
    }
    let reader = function(module, declaration(parsed, sources, "read")?)?;
    let read=one(g.instructions.iter().copied().filter(|i|i.block==key.block && matches!(&i.operation,Operation::DirectCall{callee,receiver:None,arguments} if *callee==reader.id && arguments.as_slice()==[result(key).unwrap(),result(val).unwrap()])),"actual selected read K/V arguments")?;
    let mut component_ends = Vec::new();
    for (index, (component, target)) in [(key, types.0), (val, types.1)].into_iter().enumerate() {
        let entity = result(component)?;
        let ty = EntityType::Loan {
            kind: LoanKind::Shared,
            target,
        };
        need(
            g.function.entity(entity).unwrap().ty == ty
                && reader
                    .block(reader.entry_block().unwrap())
                    .unwrap()
                    .parameters
                    .get(index)
                    .and_then(|e| reader.entity(*e))
                    .is_some_and(|e| e.ty == ty),
            "reader uses exact Shared represented K/V ABI",
            component,
        )?;
        let end = one(
            g.instructions.iter().copied().filter(|i| {
                i.operation
                    == Operation::BorrowEnd {
                        loan: loan(component).unwrap(),
                    }
            }),
            "unique actual component End",
        )?;
        need(end.block == key.block, "component ends in actual body", end)?;
        if case.exit != 0 {
            need(
                end.origin == Origin::Source(cleanup_origin),
                "component cleanup exact entry-specific source span",
                end,
            )?;
        }
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
    let local = guard(
        g,
        module,
        sources,
        parsed,
        site.body,
        read,
        component_ends[1],
        uses,
    )?;
    if case.exit >= 2 {
        let end = one(
            ends.iter().copied().filter(|i| i.block == key.block),
            "actual body terminal EndSource",
        )?;
        need(
            g.position(component_ends[0])? < g.position(end)?
                && end.origin == Origin::Source(cleanup_origin),
            "KEnd before exact entry-specific terminal EndSource",
            end,
        )?;
    } else {
        need(
            g.position(component_ends[0])? < g.position(nexts[0])?,
            "KEnd before Next",
            nexts[0],
        )?;
    }
    let resource = match module.type_kind(types.1) {
        Some(SsaTypeKind::NullableHandle { inner }) => *inner,
        _ => types.1,
    };
    need(!g.instructions.iter().any(|i|matches!(i.operation,Operation::Drop{owner} if [types.1, resource].contains(&g.function.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()))),"provider never materializes/drops component owner",g.function.id)?;
    Ok(Evidence {
        first,
        body: key.block,
        body_owner: bo,
        normal_owner,
        ends,
        key,
        val,
        read,
        guard_drop: local,
    })
}
fn guard<'a>(
    g: &Graph<'a>,
    module: &Module,
    sources: &SourceMap,
    parsed: &ParsedFile,
    body: StatementId,
    read: &Instruction,
    value_end: &Instruction,
    uses: &mut Uses,
) -> Result<&'a Instruction, Failure> {
    let span = parsed.ast().statements().get(body).unwrap().span();
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
                } if inside(n.span(), span) && sources.slice(*name) == Ok("local") => {
                    Some((*name, *initializer))
                }
                _ => None,
            }),
        "original local Guard",
    )?;
    let ast = parsed.ast().expressions().get(init).unwrap();
    need(
        sources.slice(ast.span()) == Ok("Guard()"),
        "actual Guard constructor AST",
        ast,
    )?;
    let allocated = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(ast.span())
                && matches!(i.operation, Operation::HeapAllocate { .. })
        }),
        "actual Guard allocation",
    )?;
    let Operation::HeapAllocate { owner: ty, payload } = allocated.operation else {
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
        "Guard actual type deinit",
        ty,
    )?;
    let payload = g.definition(EntityId::Value(payload))?;
    need(
        payload.block == read.block
            && payload.origin == Origin::Source(ast.span())
            && matches!(&payload.operation,Operation::AggregateConstruct{fields,..} if fields.is_empty()),
        "actual empty Guard payload",
        payload,
    )?;
    let owner = value(allocated)?;
    let dropped = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::Drop { owner }),
        "Guard unique body Drop",
    )?;
    need(
        allocated.block == read.block
            && dropped.block == read.block
            && dropped.origin == Origin::Source(name)
            && g.position(allocated)? < g.position(read)?
            && g.position(read)? < g.position(dropped)?
            && g.position(dropped)? < g.position(value_end)?,
        "Guard allocation/read/Drop/VEnd order",
        dropped,
    )?;
    uses.protect(EntityId::Value(owner));
    uses.permit(EntityId::Value(owner), dropped);
    Ok(dropped)
}
