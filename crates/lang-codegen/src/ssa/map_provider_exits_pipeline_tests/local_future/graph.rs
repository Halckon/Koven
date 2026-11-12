//! Actual Call anchors, full incoming identities and final Pure-owner lifetime.
pub(in crate::ssa) mod continuation;
mod provider {
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
}
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
    ast::StatementId,
    parser::{Expression, Item, NameMarker, ParsedFile, Statement, VariableKind},
    source::{SourceMap, Span},
};
use std::collections::{BTreeMap, BTreeSet};

struct Query<'a, 'p> {
    g: &'a Graph<'p>,
    module: &'p Module,
    sources: &'a SourceMap,
    parsed: &'a ParsedFile,
}
struct LoopSite {
    span: Span,
    body: StatementId,
    exit: usize,
}
struct Site {
    scan: Span,
    binding: Span,
    call: Span,
    factory: Span,
    loops: Vec<LoopSite>,
}

pub(super) fn check(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    program: &Program,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "local future original module")?;
    for (scanner, factory) in [("scan", "make"), ("scanEmpty", "makeEmpty")] {
        let site = site(case, sources, parsed, scanner, factory)?;
        let function = one(
            module
                .functions
                .iter()
                .filter(|f| f.origin == Origin::Source(site.scan)),
            "actual local future scanner",
        )?;
        let g = Graph::new(function)?;
        let q = Query {
            g: &g,
            module,
            sources,
            parsed,
        };
        need(
            g.block(function.entry_block().unwrap())?
                .parameters
                .is_empty(),
            "actual local scanner has no parameter anchor",
            function.id,
        )?;
        let factory = one(
            module
                .functions
                .iter()
                .filter(|f| f.origin == Origin::Source(site.factory)),
            "selected original local factory",
        )?;
        let call = one(g.instructions.iter().copied().filter(|i| i.origin == Origin::Source(site.call)
            && matches!(&i.operation,Operation::DirectCall {callee,receiver:None,arguments} if *callee==factory.id && arguments.is_empty())), "actual source formation Call")?;
        let anchor = result(call)?;
        need(
            matches!(anchor, EntityId::Value(_)),
            "factory actually delivers owned source",
            call,
        )?;
        let map_type = function.entity(anchor).unwrap().ty.semantic_type();
        let (kind, key, val) = module
            .map_container(map_type)
            .ok_or_else(|| failure("actual local future map storage", map_type))?;
        need(
            factory.return_types.as_slice() == [map_type]
                && factory
                    .block(factory.entry_block().unwrap())
                    .unwrap()
                    .parameters
                    .is_empty()
                && kind
                    == if case.mutable {
                        MapContainerKind::MutableMap
                    } else {
                        MapContainerKind::Map
                    }
                && module.type_kind(key) == Some(&SsaTypeKind::StringOwner)
                && module.type_kind(val) == Some(&SsaTypeKind::StringOwner),
            "factory/selected pure String Map ABI",
            factory,
        )?;
        let owners = family(&g, anchor)?;
        let mut uses = Uses::default();
        for &owner in &owners {
            uses.protect(owner);
        }
        let formed = print(&q, site.scan, "formed")?;
        let first = one(
            g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(site.loops[0].span)
                    && matches!(i.operation, Operation::MapIterationFirst { .. })
            }),
            "actual first provider",
        )?;
        need(
            call.block == function.entry_block().unwrap()
                && formed.block == call.block
                && first.block == call.block
                && g.position(call)? < g.position(formed)?
                && g.position(formed)? < g.position(first)?,
            "real formation precedes formed marker and first use",
            (call, formed, first),
        )?;
        let mut ends = vec![];
        let mut current = anchor;
        for loop_site in &site.loops {
            let evidence = provider::check(&q, loop_site, current, &owners, &mut uses)?;
            current = evidence.current;
            ends.push(evidence.ends);
        }
        need(
            g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapIterationFirst { .. }))
                .count()
                == site.loops.len(),
            "no unqueried local provider",
            function.id,
        )?;
        let drops = g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::Drop {owner} if function.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()==map_type)).collect::<Vec<_>>();
        let [drop] = drops.as_slice() else {
            return Err(failure("exactly one final local future Map Drop", &drops));
        };
        let Operation::Drop { owner } = drop.operation else {
            unreachable!()
        };
        transport(&g, EntityId::Value(owner), anchor)?;
        need(
            drop.origin == Origin::Source(site.binding)
                && !reaches(&g, drop.block, drop.block, false),
            "final Drop is original local binding and lies outside every cycle",
            drop,
        )?;
        uses.permit(EntityId::Value(owner), drop);
        let last = continuation::check(&q, &site, case.form, current, &owners, &mut uses)?;
        need(
            last.owner == EntityId::Value(owner),
            "final Drop consumes actual last-use current owner",
            drop,
        )?;
        let last = last.instruction;
        let after = print(&q, site.scan, "after-last-use")?;
        need(
            drop.block == last.block
                && g.position(drop)? == g.position(last)? + 1
                && after.block == drop.block
                && g.position(drop)? < g.position(after)?,
            "ASAP final Drop immediately after actual final size/ABI End before marker",
            (last, drop, after),
        )?;
        uses.check(&g, None)?;
        crate::ssa::map_provider_exits_pipeline_tests::local_exits::owner_flow::check(
            &g,
            module,
            &BTreeSet::from([anchor]),
        )?;
        paths(&g, call.id, formed.id, drop.id, after.id, &ends)?;
    }
    Ok(())
}

fn site(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    scanner: &str,
    factory: &str,
) -> Result<Site, Failure> {
    let decl = |name: &str| {
        one(parsed.roots().iter().filter_map(|id| {
        let n=parsed.ast().items().get(*id).ok()?;
        matches!(n.payload(),Item::Function {name:NameMarker::Present(span),..} if sources.slice(*span)==Ok(name)).then_some(n)
    }),"actual local future AST declaration")
    };
    let scan = decl(scanner)?;
    let Item::Function { parameters, .. } = scan.payload() else {
        unreachable!()
    };
    need(
        parameters.is_empty(),
        "actual scanner is zero-argument",
        scan,
    )?;
    let mut loops = parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(_, n)| {
            let Statement::For { source, body, .. } = n.payload() else {
                return None;
            };
            inside(n.span(), scan.span()).then_some((n.span(), *source, *body))
        })
        .collect::<Vec<_>>();
    loops.sort_by_key(|(span, _, _)| span.start());
    need(
        loops.len() == if case.form == 2 { 2 } else { 1 },
        "all original local future For statements",
        &loops,
    )?;
    for (_, source, _) in &loops {
        need(
            sources.slice(parsed.ast().expressions().get(*source).unwrap().span()) == Ok("source"),
            "each For reads actual named source",
            source,
        )?;
    }
    let find = |wanted: &str| {
        one(
            parsed.ast().items().iter().filter_map(|(_, n)| {
                let Item::Variable {
                    name: NameMarker::Present(name),
                    initializer,
                    kind,
                    ..
                } = n.payload()
                else {
                    return None;
                };
                (inside(n.span(), scan.span())
                    && n.span().end() < loops[0].0.start()
                    && sources.slice(*name) == Ok(wanted))
                .then_some((*name, *initializer, *kind))
            }),
            "original local source binding",
        )
    };
    let (binding, mut initializer, kind) = find("source")?;
    need(
        kind == if case.binding == 1 {
            VariableKind::Var
        } else {
            VariableKind::Val
        },
        "actual val/var binding form",
        kind,
    )?;
    if case.binding == 2 {
        let alias = parsed.ast().expressions().get(initializer).unwrap();
        need(
            alias.payload() == &Expression::Name && sources.slice(alias.span()) == Ok("original"),
            "actual moved alias name initializer",
            alias,
        )?;
        let (original, init, kind) = find("original")?;
        need(
            kind == VariableKind::Val && original.start() < binding.start(),
            "actual original formation precedes alias",
            original,
        )?;
        initializer = init;
    }
    let call = parsed.ast().expressions().get(initializer).unwrap();
    let Expression::Call {
        callee, arguments, ..
    } = call.payload()
    else {
        return Err(failure("actual local source producer Call", call));
    };
    need(
        arguments.is_empty()
            && sources.slice(parsed.ast().expressions().get(*callee).unwrap().span())
                == Ok(factory),
        "actual selected local factory AST",
        call,
    )?;
    Ok(Site {
        scan: scan.span(),
        binding,
        call: call.span(),
        factory: decl(factory)?.span(),
        loops: loops
            .into_iter()
            .enumerate()
            .map(|(i, (span, _, body))| LoopSite {
                span,
                body,
                exit: if i == 0 { case.exit } else { 0 },
            })
            .collect(),
    })
}
fn inside(span: Span, outer: Span) -> bool {
    span.source_id() == outer.source_id()
        && span.start() > outer.start()
        && span.end() < outer.end()
}
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
            .ok_or_else(|| failure("actual incoming entity", entity))?;
        let Definition::BlockParameter { block, .. } = data.definition else {
            return Err(failure("unrelated current owner/lease producer", data));
        };
        need(data.ty == ty, "actual incoming transport type", entity)?;
        let incoming = g.incoming(block);
        need(!incoming.is_empty(), "actual full predecessor set", entity)?;
        let mut args = vec![];
        for (_, edge) in incoming {
            let arg = g.argument(edge, entity)?;
            pending.push(arg);
            args.push(arg);
        }
        parents.insert(entity, args);
    }
    let mut anchored = BTreeSet::from([anchor]);
    loop {
        let before = anchored.len();
        for (&entity, args) in &parents {
            if args.iter().any(|arg| anchored.contains(arg)) {
                anchored.insert(entity);
            }
        }
        if before == anchored.len() {
            break;
        }
    }
    need(
        parents.keys().all(|entity| anchored.contains(entity)),
        "all incoming columns reach immutable real Call/lease anchor",
        &parents,
    )
}
fn family(g: &Graph<'_>, anchor: EntityId) -> Result<BTreeSet<EntityId>, Failure> {
    let mut values = BTreeSet::from([anchor]);
    loop {
        let before = values.len();
        for (_, edge) in &g.edges {
            for (&arg, &param) in edge.arguments.iter().zip(&g.block(edge.target)?.parameters) {
                if values.contains(&arg) {
                    values.insert(param);
                }
            }
        }
        if before == values.len() {
            break;
        }
    }
    for &entity in &values {
        transport(g, entity, anchor)?;
    }
    Ok(values)
}
fn reaches(g: &Graph<'_>, start: BlockId, target: BlockId, include_start: bool) -> bool {
    let mut pending = if include_start {
        vec![start]
    } else {
        g.edges
            .iter()
            .filter_map(|(from, edge)| (*from == start).then_some(edge.target))
            .collect()
    };
    let mut seen = BTreeSet::new();
    while let Some(block) = pending.pop() {
        if block == target {
            return true;
        }
        if seen.insert(block) {
            pending.extend(
                g.edges
                    .iter()
                    .filter_map(|(from, edge)| (*from == block).then_some(edge.target)),
            );
        }
    }
    false
}
fn print<'a, 'p>(
    q: &Query<'a, 'p>,
    within: Span,
    expected: &str,
) -> Result<&'p Instruction, Failure> {
    let (call, arg) = one(
        q.parsed.ast().expressions().iter().filter_map(|(_, n)| {
            let Expression::Call {
                callee, arguments, ..
            } = n.payload()
            else {
                return None;
            };
            let [a] = arguments.as_slice() else {
                return None;
            };
            let arg = q.parsed.ast().expressions().get(a.value).unwrap().span();
            (inside(n.span(), within)
                && q.sources
                    .slice(q.parsed.ast().expressions().get(*callee).unwrap().span())
                    == Ok("println")
                && q.sources
                    .slice(arg)
                    .ok()
                    .and_then(|s| s.strip_prefix('"'))
                    .and_then(|s| s.strip_suffix('"'))
                    == Some(expected))
            .then_some((n.span(), arg))
        }),
        "original actual scanner println",
    )?;
    let i = one(
        q.g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(call)
                && matches!(
                    i.operation,
                    Operation::PrintString { .. } | Operation::PrintLiteral { .. }
                )
        }),
        "actual selected scanner println",
    )?;
    match &i.operation {
        Operation::PrintLiteral { bytes } => {
            let mut wanted = expected.as_bytes().to_vec();
            wanted.push(b'\n');
            need(bytes == &wanted, "actual selected literal bytes", i)?;
        }
        Operation::PrintString { value: loan } => {
            let birth = q.g.definition(EntityId::Loan(*loan))?;
            let Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            } = birth.operation
            else {
                return Err(failure("actual print String view", birth));
            };
            let root = q.g.definition(EntityId::Place(place))?;
            let Operation::RootPlace { owner } = root.operation else {
                return Err(failure("actual print String root", root));
            };
            let literal = q.g.definition(EntityId::Value(owner))?;
            need(
                literal.origin == Origin::Source(arg)
                    && matches!(&literal.operation,Operation::StringLiteral {bytes,..} if bytes==expected.as_bytes()),
                "actual selected String bytes",
                literal,
            )?;
        }
        _ => unreachable!(),
    }
    Ok(i)
}
fn paths(
    g: &Graph<'_>,
    producer: InstructionId,
    formed: InstructionId,
    drop: InstructionId,
    marker: InstructionId,
    ends: &[Vec<&Instruction>],
) -> Result<(), Failure> {
    let mut pending = vec![(
        g.function.entry_block().unwrap(),
        0u8,
        false,
        0u8,
        vec![0u8; ends.len()],
    )];
    let mut seen = BTreeSet::new();
    while let Some((block, mut made, mut marked, mut dropped, mut ended)) = pending.pop() {
        if !seen.insert((block, made, marked, dropped, ended.clone())) {
            continue;
        }
        for i in g.ops(block)? {
            if i.id == producer {
                made += 1;
                need(made == 1, "real factory evaluated once per normal path", i)?;
            }
            if i.id == formed {
                need(
                    made == 1 && dropped == 0,
                    "formed marker preserves live real source",
                    i,
                )?;
                marked = true;
            }
            if matches!(
                i.operation,
                Operation::MapIterationFirst { .. } | Operation::MapSize { .. }
            ) {
                need(
                    marked && dropped == 0,
                    "actual source reads only while retained live",
                    i,
                )?;
            }
            for (index, group) in ends.iter().enumerate() {
                if group.iter().any(|end| end.id == i.id) {
                    ended[index] += 1;
                    need(
                        ended[index] == 1 && dropped == 0,
                        "each provider ends once before final source Drop",
                        i,
                    )?;
                }
            }
            if i.id == drop {
                dropped += 1;
                need(
                    dropped == 1 && ended.iter().all(|n| *n == 1),
                    "all providers finish before unique final Drop",
                    i,
                )?;
            }
            if i.id == marker {
                need(dropped == 1, "ASAP final owner Drop precedes marker", i)?;
            }
        }
        if let TerminatorKind::Return { values } = &g.term(block)?.kind {
            need(
                values.is_empty()
                    && made == 1
                    && marked
                    && dropped == 1
                    && ended.iter().all(|n| *n == 1),
                "every normal Unit Return has one actual formation and final Drop",
                g.term(block)?,
            )?;
        }
        for (_, edge) in g.edges.iter().filter(|(from, _)| *from == block) {
            pending.push((edge.target, made, marked, dropped, ended.clone()));
        }
    }
    Ok(())
}
