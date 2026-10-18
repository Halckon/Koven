//! Independent future-use identity and pure-owner ASAP checks over reachable SSA.
mod continuation;
use super::Case;
use crate::ssa::{
    map_provider_exits_pipeline_tests::{
        fixture::{Failure, failure, need},
        query_graph::{Graph, Uses, result, value},
        query_inputs::one,
    },
    model::*,
};
use lang_frontend::{
    parser::{Expression, Item, NameMarker, ParsedFile, Statement},
    source::{SourceMap, Span},
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn check(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    program: &Program,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "future-use original module")?;
    let declaration=one(parsed.roots().iter().filter_map(|id|{
        let node=parsed.ast().items().get(*id).ok()?;
        matches!(node.payload(),Item::Function {name:NameMarker::Present(name),..} if sources.slice(*name)==Ok("scan")).then_some(node)
    }),"original scan AST declaration")?;
    let Item::Function { parameters, .. } = declaration.payload() else {
        unreachable!()
    };
    let [parameter] = parameters.as_slice() else {
        return Err(failure("original own parameter", parameters));
    };
    need(
        matches!(
            parameter.mode_marker,
            Some(lang_frontend::parser::ParameterModeMarker::Own(_))
        ),
        "original AST parameter is explicitly own",
        parameter,
    )?;
    let NameMarker::Present(parameter_span) = parameter.name else {
        return Err(failure("parameter binding span", parameter));
    };
    let scan = one(
        module
            .functions
            .iter()
            .filter(|f| f.origin == Origin::Source(declaration.span())),
        "actual original scan",
    )?;
    let g = Graph::new(scan)?;
    let [anchor @ EntityId::Value(_)] = g.block(scan.entry_block().unwrap())?.parameters.as_slice()
    else {
        return Err(failure("one actual own Map entry parameter", scan));
    };
    let map_type = scan.entity(*anchor).unwrap().ty.semantic_type();
    let (kind, key_type, value_type) = module
        .map_container(map_type)
        .ok_or_else(|| failure("actual own Map representation", map_type))?;
    need(
        kind == if case.mutable {
            MapContainerKind::MutableMap
        } else {
            MapContainerKind::Map
        } && module.type_kind(key_type) == Some(&SsaTypeKind::StringOwner)
            && module.type_kind(value_type) == Some(&SsaTypeKind::StringOwner),
        "original pure Map/MutableMap storage",
        (kind, key_type, value_type),
    )?;
    let owners = family(&g, *anchor)?;
    let mut uses = Uses::default();
    for &owner in &owners {
        uses.protect(owner);
    }
    let loops = parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(_, node)| {
            let Statement::For { source, body, .. } = node.payload() else {
                return None;
            };
            (inside(node.span(), declaration.span())).then_some((node.span(), *source, *body))
        })
        .collect::<Vec<_>>();
    need(
        loops.len() == if case.form == 3 { 2 } else { 1 },
        "all actual original For statements",
        &loops,
    )?;
    let mut loops = loops;
    loops.sort_by_key(|(span, _, _)| span.start());
    let mut source_ends = Vec::new();
    for (index, (span, source, body)) in loops.iter().copied().enumerate() {
        need(
            sources.slice(parsed.ast().expressions().get(source).unwrap().span()) == Ok("source"),
            "actual For original source expression",
            span,
        )?;
        let broken = index == 0 && case.label.contains("-break-");
        let continued = index == 0 && case.label.contains("-continue-");
        source_ends.push(provider(
            &g,
            module,
            sources,
            parsed,
            (span, body, broken, continued),
            &owners,
            &mut uses,
        )?);
    }
    need(
        g.instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::MapIterationFirst { .. }))
            .count()
            == loops.len(),
        "no unqueried provider",
        scan.id,
    )?;
    let drops=g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::Drop {owner} if scan.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()==map_type)).collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        return Err(failure("one actual pure Map Drop", &drops));
    };
    let Operation::Drop { owner } = drop.operation else {
        unreachable!()
    };
    transport(&g, EntityId::Value(owner), *anchor)?;
    need(
        drop.origin == Origin::Source(parameter_span),
        "Map Drop original own binding",
        drop,
    )?;
    uses.permit(EntityId::Value(owner), drop);
    need(
        !cycle(&g, drop.block),
        "Map Drop cannot be executed on any CFG cycle",
        drop,
    )?;
    let calls = borrow_calls(
        &g,
        module,
        sources,
        parsed,
        (declaration.span(), case.form),
        &owners,
        &mut uses,
    )?;
    let sizes = g
        .instructions
        .iter()
        .copied()
        .filter(|i| matches!(i.operation, Operation::MapSize { .. }))
        .collect::<Vec<_>>();
    for size in &sizes {
        let Operation::MapSize { owner } = size.operation else {
            unreachable!()
        };
        need(
            owners.contains(&owner)
                && matches!(size.origin,Origin::Source(span) if sources.slice(span)==Ok("source.size") && span.start()>loops[0].0.end()),
            "actual post-provider size on current original owner",
            size,
        )?;
        uses.permit(owner, size);
    }
    // Every RootPlace of this protected owner is accounted for by a provider or
    // actual Borrow call; Uses rejects a new consumption or hidden read.
    let marker = marker(&g, sources, parsed, declaration.span())?;
    let boundary = match case.form {
        0 => {
            need(
                calls.is_empty() && sizes.len() == 1,
                "size-only future use",
                (&calls, &sizes),
            )?;
            sizes[0]
        }
        1 => {
            need(
                calls.len() == 2 && sizes.is_empty(),
                "two actual future Borrow calls",
                (&calls, &sizes),
            )?;
            calls.last().unwrap().1
        }
        2 => {
            need(
                calls.len() == 1 && sizes.len() == 1,
                "CFG then actual future Borrow",
                (&calls, &sizes),
            )?;
            calls[0].1
        }
        _ => {
            need(
                calls.is_empty() && sizes.is_empty(),
                "second For is the final use",
                (&calls, &sizes),
            )?;
            one(
                source_ends.last().unwrap().iter().copied(),
                "second normal For exhaustion EndSource",
            )?
        }
    };
    need(
        drop.block == boundary.block && g.position(drop)? == g.position(boundary)? + 1,
        "ASAP Drop immediately follows actual final use/loan End",
        (boundary, drop),
    )?;
    need(
        drop.block == marker.block && g.position(drop)? < g.position(marker)?,
        "ASAP Drop precedes actual after-last-use println",
        (drop, marker),
    )?;
    continuation::check(
        &g,
        module,
        sources,
        parsed,
        declaration.span(),
        case.form,
        &sizes,
        &calls,
        &mut uses,
    )?;
    uses.check(&g, None)?;
    paths(&g, drop.id, marker.id, &source_ends, &calls, case.form)?;
    Ok(())
}

fn inside(span: Span, outer: Span) -> bool {
    span.source_id() == outer.source_id()
        && span.start() > outer.start()
        && span.end() < outer.end()
}

// Trace every predecessor column, including loop backedges, to one immutable
// semantic anchor. A connected unrelated producer or unanchored cycle fails.
fn transport(g: &Graph<'_>, current: EntityId, anchor: EntityId) -> Result<(), Failure> {
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
            .ok_or_else(|| failure("transport entity", entity))?;
        let Definition::BlockParameter { block, .. } = data.definition else {
            return Err(failure("unrelated future-use owner/loan", data));
        };
        need(data.ty == ty, "actual transport type", data)?;
        let incoming = g.incoming(block);
        need(
            !incoming.is_empty(),
            "complete nonempty actual incoming",
            entity,
        )?;
        let mut values = vec![];
        for (_, edge) in incoming {
            let arg = g.argument(edge, entity)?;
            pending.push(arg);
            values.push(arg);
        }
        parents.insert(entity, values);
    }
    let mut anchored = BTreeSet::from([anchor]);
    loop {
        let before = anchored.len();
        for (&entity, args) in &parents {
            if args.iter().any(|a| anchored.contains(a)) {
                anchored.insert(entity);
            }
        }
        if anchored.len() == before {
            break;
        }
    }
    need(
        parents.keys().all(|e| anchored.contains(e)),
        "all actual incoming roots reach immutable anchor",
        &parents,
    )
}
fn family(g: &Graph<'_>, anchor: EntityId) -> Result<BTreeSet<EntityId>, Failure> {
    let mut entities = BTreeSet::from([anchor]);
    loop {
        let before = entities.len();
        for (_, edge) in &g.edges {
            for (&arg, &param) in edge.arguments.iter().zip(&g.block(edge.target)?.parameters) {
                if entities.contains(&arg) {
                    entities.insert(param);
                }
            }
        }
        if entities.len() == before {
            break;
        }
    }
    for &entity in &entities {
        transport(g, entity, anchor)?;
    }
    Ok(entities)
}
fn cycle(g: &Graph<'_>, start: BlockId) -> bool {
    let mut pending = g
        .edges
        .iter()
        .filter_map(|(from, e)| (*from == start).then_some(e.target))
        .collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    while let Some(block) = pending.pop() {
        if block == start {
            return true;
        }
        if seen.insert(block) {
            pending.extend(
                g.edges
                    .iter()
                    .filter_map(|(from, e)| (*from == block).then_some(e.target)),
            );
        }
    }
    false
}

fn provider<'a>(
    g: &Graph<'a>,
    module: &Module,
    sources: &SourceMap,
    parsed: &ParsedFile,
    site: (Span, lang_frontend::ast::StatementId, bool, bool),
    owners: &BTreeSet<EntityId>,
    uses: &mut Uses,
) -> Result<Vec<&'a Instruction>, Failure> {
    let (span, body, broken, continued) = site;
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
    if broken {
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
                    Expression::Continue { .. } | Expression::Break { .. }
                ))
            .then_some(n.span())
        })
        .collect::<Vec<_>>();
    need(
        transfer.len() == usize::from(broken || continued),
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
        next.len() == usize::from(!broken),
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
        ends.len() == if broken { 2 } else { 1 },
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
    if !broken {
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
    guard_drop(
        g,
        module,
        sources,
        parsed,
        body,
        (read, component_ends[1]),
        uses,
    )?;
    if broken {
        let end = one(
            ends.iter().copied().filter(|i| i.block == key.block),
            "actual Break source End",
        )?;
        need(
            g.position(component_ends[0])? < g.position(end)?,
            "Break KEnd before EndSource",
            end,
        )?;
        need(
            g.branch(key.block)?.target == g.branch(when_false.target)?.target,
            "actual Break/exhaustion continuation merge",
            span,
        )?;
        g.incoming_from(g.branch(key.block)?.target, &[key.block, when_false.target])?;
    } else {
        need(
            g.position(component_ends[0])? < g.position(next[0])?,
            "KEnd before actual Next",
            next[0],
        )?;
    }
    Ok(ends)
}

fn guard_drop(
    g: &Graph<'_>,
    module: &Module,
    sources: &SourceMap,
    parsed: &ParsedFile,
    body: lang_frontend::ast::StatementId,
    read_end: (&Instruction, &Instruction),
    uses: &mut Uses,
) -> Result<(), Failure> {
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
    Ok(())
}

fn borrow_calls<'a>(
    g: &Graph<'a>,
    module: &Module,
    sources: &SourceMap,
    parsed: &ParsedFile,
    scan_form: (Span, usize),
    owners: &BTreeSet<EntityId>,
    uses: &mut Uses,
) -> Result<Vec<(&'a Instruction, &'a Instruction)>, Failure> {
    let (scan, form) = scan_form;
    let original_calls = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, node)| {
            let Expression::Call {
                callee, arguments, ..
            } = node.payload()
            else {
                return None;
            };
            (inside(node.span(), scan)
                && sources.slice(parsed.ast().expressions().get(*callee).unwrap().span())
                    == Ok("readMap"))
            .then_some((node, arguments))
        })
        .collect::<Vec<_>>();
    need(
        original_calls.len()
            == match form {
                1 => 2,
                2 => 1,
                _ => 0,
            },
        "form exact original future Borrow call count",
        &original_calls,
    )?;
    if original_calls.is_empty() {
        return Ok(Vec::new());
    }
    let decl=one(parsed.roots().iter().filter_map(|id|{let n=parsed.ast().items().get(*id).ok()?;matches!(n.payload(),Item::Function {name:NameMarker::Present(name),..} if sources.slice(*name)==Ok("readMap")).then_some(n.span())}),"original future Borrow declaration")?;
    let callee = one(
        module
            .functions
            .iter()
            .filter(|f| f.origin == Origin::Source(decl)),
        "actual future Borrow callee",
    )?;
    let mut calls = vec![];
    for (node, arguments) in original_calls {
        need(
            matches!(arguments.as_slice(),[a] if sources.slice(parsed.ast().expressions().get(a.value).unwrap().span())==Ok("source")),
            "original future Borrow source argument",
            node,
        )?;
        let call = one(
            g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(node.span())
                    && matches!(i.operation, Operation::DirectCall { .. })
            }),
            "actual future Borrow call",
        )?;
        let Operation::DirectCall {
            callee: actual,
            receiver: None,
            arguments,
        } = &call.operation
        else {
            return Err(failure("future Borrow call ABI", call));
        };
        let [argument @ EntityId::Loan(loan)] = arguments.as_slice() else {
            return Err(failure("future Borrow exact one loan", call));
        };
        need(
            *actual == callee.id
                && callee
                    .block(callee.entry_block().unwrap())
                    .unwrap()
                    .parameters
                    .len()
                    == 1
                && callee
                    .entity(
                        callee
                            .block(callee.entry_block().unwrap())
                            .unwrap()
                            .parameters[0],
                    )
                    .unwrap()
                    .ty
                    == g.function.entity(*argument).unwrap().ty,
            "future Borrow selected callee/parameter ABI",
            call,
        )?;
        let birth = g.definition(*argument)?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = birth.operation
        else {
            return Err(failure("future call actual scoped loan birth", birth));
        };
        let root = g.definition(EntityId::Place(place))?;
        let Operation::RootPlace { owner } = root.operation else {
            return Err(failure("future call original owner root", root));
        };
        need(
            owners.contains(&EntityId::Value(owner)),
            "future call borrows current original owner",
            root,
        )?;
        let end = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: *loan }),
            "future call exact scoped loan End",
        )?;
        need(
            birth.block == call.block
                && end.block == call.block
                && g.position(birth)? < g.position(call)?
                && g.position(call)? < g.position(end)?,
            "future call ABI loan birth/call/End",
            (birth, call, end),
        )?;
        uses.permit(EntityId::Value(owner), root);
        uses.protect(EntityId::Place(place));
        uses.permit(EntityId::Place(place), birth);
        uses.protect(*argument);
        uses.permit(*argument, call);
        uses.permit(*argument, end);
        calls.push((call, end));
    }
    calls.sort_by_key(|(call, _)| match call.origin {
        Origin::Source(span) => span.start(),
        _ => usize::MAX,
    });
    Ok(calls)
}
fn marker<'a>(
    g: &Graph<'a>,
    sources: &SourceMap,
    parsed: &ParsedFile,
    scan: Span,
) -> Result<&'a Instruction, Failure> {
    let call=one(parsed.ast().expressions().iter().filter_map(|(_,n)|{
        let Expression::Call {callee,arguments,..}=n.payload() else{return None;};
        (inside(n.span(),scan) && sources.slice(parsed.ast().expressions().get(*callee).unwrap().span())==Ok("println") && matches!(arguments.as_slice(),[a] if sources.slice(parsed.ast().expressions().get(a.value).unwrap().span())==Ok("\"after-last-use\""))).then_some(n.span())
    }),"original after-last-use println")?;
    one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(call)
                && matches!(
                    i.operation,
                    Operation::PrintString { .. } | Operation::PrintLiteral { .. }
                )
        }),
        "actual after-last-use println",
    )
}
fn paths(
    g: &Graph<'_>,
    drop: InstructionId,
    marker: InstructionId,
    ends: &[Vec<&Instruction>],
    calls: &[(&Instruction, &Instruction)],
    form: usize,
) -> Result<(), Failure> {
    let mut pending = vec![(
        g.function.entry_block().unwrap(),
        0u8,
        vec![0u8; ends.len()],
        BTreeSet::new(),
    )];
    let mut seen = BTreeSet::new();
    while let Some((block, mut dropped, mut ended, mut called)) = pending.pop() {
        if !seen.insert((block, dropped, ended.clone(), called.clone())) {
            continue;
        }
        for i in g.ops(block)? {
            for (index, group) in ends.iter().enumerate() {
                if group.iter().any(|e| e.id == i.id) {
                    ended[index] += 1;
                    need(
                        ended[index] == 1 && dropped == 0,
                        "each provider ends once while original owner remains live",
                        i,
                    )?;
                }
            }
            if calls.iter().any(|(call, _)| call.id == i.id) {
                need(
                    dropped == 0 && ended[0] == 1,
                    "actual future Borrow after provider End before owner Drop",
                    i,
                )?;
                called.insert(i.id);
            }
            if i.id == drop {
                dropped += 1;
                need(
                    dropped == 1 && ended.iter().all(|n| *n == 1) && called.len() == calls.len(),
                    "all provider/future uses completed before unique owner Drop",
                    i,
                )?;
            }
            if i.id == marker {
                need(
                    dropped == 1,
                    "normal after-last-use marker observes completed ASAP Drop",
                    i,
                )?;
            }
            if dropped != 0 && i.id != drop {
                need(
                    !matches!(
                        i.operation,
                        Operation::MapIterationFirst { .. } | Operation::MapSize { .. }
                    ),
                    "source cannot be read after final ASAP Drop",
                    i,
                )?;
            }
        }
        if matches!(g.term(block)?.kind, TerminatorKind::Return { .. }) {
            need(
                dropped == 1 && ended.iter().all(|n| *n == 1) && (form != 3 || ended.len() == 2),
                "every finite normal Return has exactly one source Drop",
                g.term(block)?,
            )?;
        }
        for (_, edge) in g.edges.iter().filter(|(from, _)| *from == block) {
            pending.push((edge.target, dropped, ended.clone(), called.clone()));
        }
    }
    Ok(())
}
