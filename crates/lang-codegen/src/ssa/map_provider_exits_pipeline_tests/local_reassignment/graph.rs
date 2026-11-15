//! Independent SSA epoch identities, assignment ordering and finite-path cleanup.
mod provider {
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
}
mod storage {
    //! Actual returned map and payload ABI; empty producers are proved from SSA construction.
    use super::*;
    pub(super) fn factory(
        case: &Case,
        sources: &SourceMap,
        parsed: &ParsedFile,
        module: &Module,
        f: &Function,
    ) -> Result<(), Failure> {
        let name = one(
            parsed.roots().iter().filter_map(|id| {
                let n = parsed.ast().items().get(*id).ok()?;
                matches!(n.payload(),Item::Function{..} if f.origin==Origin::Source(n.span()))
                    .then_some(n)
            }),
            "original selected factory item",
        )?;
        let Item::Function {
            name: NameMarker::Present(name),
            ..
        } = name.payload()
        else {
            unreachable!()
        };
        let name = sources
            .slice(*name)
            .map_err(|e| failure("actual factory name", e))?;
        let empty = name.ends_with("Empty");
        let g = Graph::new(f)?;
        let constructs = g
            .instructions
            .iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::MapConstruct { .. }))
            .collect::<Vec<_>>();
        need(
            constructs.len() == 1,
            "actual factory starts one empty map",
            &constructs,
        )?;
        let puts = g
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::MapPut { .. }))
            .count();
        need(
            puts == if empty {
                0
            } else if case.payload == 2 {
                2
            } else {
                1
            },
            "factory real entry population count",
            (name, puts),
        )?;
        let heaps = g
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::HeapAllocate { .. }))
            .count();
        need(
            heaps == usize::from(!empty && case.payload != 0),
            "empty factory creates no Resource payload",
            (name, heaps),
        )?;
        if empty {
            let mut current = result(constructs[0])?;
            let consumes = g
                .instructions
                .iter()
                .copied()
                .filter(|i| matches!(i.operation, Operation::MapConsume { .. }))
                .collect::<Vec<_>>();
            need(
                consumes.len() == usize::from(!case.mutable),
                "empty Map conversion keeps same zero entries",
                &consumes,
            )?;
            if let Some(consume) = consumes.first() {
                need(
                    matches!(consume.operation,Operation::MapConsume{owner,..} if EntityId::Value(owner)==current),
                    "consume original empty MutableMap",
                    consume,
                )?;
                current = result(consume)?;
            }
            let EntityId::Value(returned) = current else {
                return Err(failure("actual empty owner Value", current));
            };
            need(g.blocks.iter().filter(|&&b|matches!(&g.term(b).unwrap().kind,TerminatorKind::Return{values} if values.as_slice()==[returned])).count()==1,"empty factory returns exact zero-entry construction",f.id)?;
        }
        need(
            f.return_types.len() == 1 && module.map_container(f.return_types[0]).is_some(),
            "factory returns actual Map storage",
            f.id,
        )
    }
    pub(super) fn representation(
        case: &Case,
        sources: &SourceMap,
        parsed: &ParsedFile,
        module: &Module,
        map: SsaTypeId,
    ) -> Result<(), Failure> {
        let (kind, key, val) = module
            .map_container(map)
            .ok_or_else(|| failure("actual epoch map representation", map))?;
        need(
            kind == if case.mutable {
                MapContainerKind::MutableMap
            } else {
                MapContainerKind::Map
            } && module.type_kind(key) == Some(&SsaTypeKind::StringOwner),
            "actual epoch container/String key storage",
            (kind, key),
        )?;
        if case.payload == 0 {
            return need(
                module.type_kind(val) == Some(&SsaTypeKind::StringOwner),
                "Pure payload remains String owner",
                val,
            );
        }
        let resource = if case.payload == 2 {
            let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(val) else {
                return Err(failure("nullable keeps real handle ABI", val));
            };
            *inner
        } else {
            val
        };
        let Some(SsaTypeKind::HeapOwner {
            payload: Some(payload),
            ..
        }) = module.type_kind(resource)
        else {
            return Err(failure("actual Resource heap payload", resource));
        };
        fn strings(m: &Module, t: SsaTypeId) -> usize {
            match m.type_kind(t) {
                Some(SsaTypeKind::StringOwner) => 1,
                Some(SsaTypeKind::Aggregate { fields, .. }) => {
                    fields.iter().map(|&t| strings(m, t)).sum()
                }
                _ => 0,
            }
        }
        need(
            strings(module, *payload) == 1,
            "Resource still owns its String tag",
            resource,
        )?;
        let class = one(
            parsed.roots().iter().filter_map(|id| {
                let n = parsed.ast().items().get(*id).ok()?;
                let Item::Classifier(c) = n.payload() else {
                    return None;
                };
                matches!(c.name,NameMarker::Present(name) if sources.slice(name)==Ok("Resource"))
                    .then_some(c)
            }),
            "original Resource declaration",
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
            "original Resource deinit",
        )?;
        need(
            module
                .deinits
                .get(&resource)
                .and_then(|id| module.function(*id))
                .is_some_and(|f| f.origin == Origin::Source(deinit)),
            "actual Resource representation selects original deinit",
            resource,
        )
    }
}
use super::{Case, facts::ast};
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
    source::{SourceMap, Span},
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn check(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    program: &Program,
    unit: bool,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "reassignment actual module")?;
    for site in ast::sites(case, sources, parsed)? {
        let scan = function(module, site.scan)?;
        let g = Graph::new(scan)?;
        need(
            g.block(scan.entry_block().unwrap())?.parameters.is_empty()
                && scan.return_types.is_empty(),
            "actual local zero-argument Unit scanner",
            scan.id,
        )?;
        let calls=[site.birth,site.rhs].map(|id|{
            let epoch=usize::from(id==site.rhs);let factory=function(module,site.factories[epoch])?;
            let call=one(g.instructions.iter().copied().filter(|i|i.origin==Origin::Source(ast::expression(parsed,id)) && matches!(&i.operation,Operation::DirectCall{callee,receiver:None,arguments} if *callee==factory.id && arguments.is_empty())),"actual distinct old/new factory Call")?;
            let entity=result(call)?;need(matches!(entity,EntityId::Value(_)) && factory.return_types.as_slice()==[scan.entity(entity).unwrap().ty.semantic_type()] && factory.block(factory.entry_block().unwrap()).unwrap().parameters.is_empty(),"actual factory owned Value ABI",call)?;
            storage::factory(case,sources,parsed,module,factory)?;Ok::<_, Failure>(call)
        });
        let [old, new] = calls;
        let calls = [old?, new?];
        let anchors = [result(calls[0])?, result(calls[1])?];
        need(
            anchors[0] != anchors[1],
            "replacement has a fresh immutable Call identity",
            anchors,
        )?;
        let types = [
            scan.entity(anchors[0]).unwrap().ty,
            scan.entity(anchors[1]).unwrap().ty,
        ];
        need(
            types[0] == types[1],
            "replacement preserves actual Map storage",
            types,
        )?;
        storage::representation(case, sources, parsed, module, types[0].semantic_type())?;
        let families = [family(&g, anchors[0])?, family(&g, anchors[1])?];
        need(
            families[0].is_disjoint(&families[1]),
            "old/new owner transport never aliases across epochs",
            &families,
        )?;
        let mut uses = Uses::default();
        for f in &families {
            for &e in f {
                uses.protect(e);
            }
        }
        let mut providers = Vec::new();
        for l in &site.loops {
            providers.push(provider::check(
                &g,
                module,
                (sources, parsed),
                l,
                anchors[l.epoch],
                &families[l.epoch],
                &mut uses,
            )?);
        }
        need(
            g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapIterationFirst { .. }))
                .count()
                == providers.len(),
            "every actual For queried exactly once",
            scan.id,
        )?;
        let all_drops=g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::Drop{owner} if scan.entity(EntityId::Value(owner)).unwrap().ty==types[0])).collect::<Vec<_>>();
        need(
            all_drops.len() == 2,
            "one Drop site per old/new epoch",
            &all_drops,
        )?;
        let mut drops = Vec::new();
        for epoch in 0..2 {
            let drop=one(all_drops.iter().copied().filter(|i|matches!(i.operation,Operation::Drop{owner} if families[epoch].contains(&EntityId::Value(owner)))),"Drop belongs to one immutable epoch only")?;
            let Operation::Drop { owner } = drop.operation else {
                unreachable!()
            };
            let p = site
                .loops
                .iter()
                .zip(&providers)
                .find_map(|(l, p)| (l.epoch == epoch).then_some(p));
            let current = p.map_or(anchors[epoch], |p| p.exhausted_owner);
            need(
                EntityId::Value(owner) == current,
                "Drop is exact current terminal column, not merely related history",
                (drop, current),
            )?;
            let marker = if epoch == 0 {
                site.binding
            } else if let Some((alias, _)) = site.alias {
                alias
            } else if unit {
                ast::expression(parsed, site.assignment)
            } else {
                site.binding
            };
            need(
                drop.origin == Origin::Source(marker) && !cyclic(&g, drop.block),
                "Drop exact epoch binding origin and no cleanup cycle",
                drop,
            )?;
            if epoch == 0 && case.payload != 0 {
                need(
                    drop.block == calls[1].block && g.position(drop)? == g.position(calls[1])? + 1,
                    "complete replacement RHS first, old Resource Drop next",
                    (calls[1], drop),
                )?;
            } else if case.payload == 0 {
                let last = p.map_or(calls[epoch], |p| p.end);
                need(
                    drop.block == last.block && g.position(drop)? == g.position(last)? + 1,
                    "Pure ASAP follows actual FE final formation/End point",
                    (last, drop),
                )?;
            } else {
                need(
                    matches!(g.term(drop.block)?.kind,TerminatorKind::Return{ref values} if values.is_empty()),
                    "new Resource retained to lexical Unit return cleanup",
                    drop,
                )?;
            }
            uses.permit(EntityId::Value(owner), drop);
            drops.push(drop);
        }
        let formed = print(&g, sources, parsed, site.scan, "formed")?;
        let replaced = print(&g, sources, parsed, site.scan, "replaced")?;
        let tail = print(&g, sources, parsed, site.scan, "tail")?;
        need(
            calls[0].block == scan.entry_block().unwrap()
                && formed.block == calls[0].block
                && g.position(calls[0])? < g.position(formed)?
                && replaced.block == calls[1].block
                && g.position(calls[1])? < g.position(replaced)?,
            "actual producer and assignment commit markers",
            (calls, formed, replaced),
        )?;
        need(
            drops[0].block == replaced.block && g.position(drops[0])? < g.position(replaced)?,
            "old owner gone before replacement commit marker",
            drops[0],
        )?;
        let last = providers.last().unwrap();
        if case.payload == 0 {
            need(
                tail.block == drops[1].block && g.position(drops[1])? < g.position(tail)?,
                "new Pure drop precedes tail marker",
                tail,
            )?;
        } else {
            need(
                tail.block == drops[1].block && g.position(tail)? < g.position(drops[1])?,
                "new Resource remains live through tail marker",
                tail,
            )?;
        }
        if case.history == 1 {
            let between = print(&g, sources, parsed, site.scan, "between")?;
            need(
                between.block == providers[0].end.block
                    && calls[1].block == between.block
                    && g.position(providers[0].end)? < g.position(between)?
                    && g.position(between)? < g.position(calls[1])?,
                "old loop exhaustion precedes RHS of second epoch",
                between,
            )?;
        }
        if case.history == 2 {
            let moved = print(&g, sources, parsed, site.scan, "moved")?;
            need(
                moved.block == last.first.block
                    && g.position(replaced)? < g.position(moved)?
                    && g.position(moved)? < g.position(last.first)?,
                "replacement committed before alias move and provider",
                moved,
            )?;
        }
        uses.check(&g, None)?;
        crate::ssa::map_provider_exits_pipeline_tests::local_exits::owner_flow::check(
            &g,
            module,
            &anchors.iter().copied().collect(),
        )?;
        paths(
            &g,
            calls,
            [drops[0], drops[1]],
            formed,
            replaced,
            tail,
            &site.loops,
            &providers,
            case,
        )?;
    }
    Ok(())
}
fn function(module: &Module, span: Span) -> Result<&Function, Failure> {
    one(
        module
            .functions
            .iter()
            .filter(|f| f.origin == Origin::Source(span)),
        "actual original callable identity",
    )
}
fn lineage(g: &Graph<'_>, current: EntityId, anchor: EntityId) -> Result<(), Failure> {
    let ty = g.function.entity(anchor).unwrap().ty;
    let mut pending = vec![current];
    let mut dependencies = BTreeMap::new();
    while let Some(e) = pending.pop() {
        if e == anchor || dependencies.contains_key(&e) {
            continue;
        }
        let d = g
            .function
            .entity(e)
            .ok_or_else(|| failure("actual transport entity", e))?;
        let Definition::BlockParameter { block, .. } = d.definition else {
            return Err(failure("foreign epoch producer", d));
        };
        need(d.ty == ty, "actual epoch transport type", d)?;
        let incoming = g.incoming(block);
        need(!incoming.is_empty(), "actual full predecessor set", e)?;
        let values = incoming
            .iter()
            .map(|(_, edge)| g.argument(edge, e))
            .collect::<Result<Vec<_>, _>>()?;
        pending.extend(values.iter().copied());
        dependencies.insert(e, values);
    }
    let mut anchored = BTreeSet::from([anchor]);
    loop {
        let before = anchored.len();
        for (&e, args) in &dependencies {
            if args.iter().any(|a| anchored.contains(a)) {
                anchored.insert(e);
            }
        }
        if before == anchored.len() {
            break;
        }
    }
    need(
        dependencies.keys().all(|e| anchored.contains(e)),
        "all epoch incoming reach exact original Call anchor",
        dependencies,
    )
}
fn family(g: &Graph<'_>, anchor: EntityId) -> Result<BTreeSet<EntityId>, Failure> {
    let mut members = BTreeSet::from([anchor]);
    loop {
        let before = members.len();
        for (_, edge) in &g.edges {
            for (&a, &p) in edge.arguments.iter().zip(&g.block(edge.target)?.parameters) {
                if members.contains(&a) {
                    members.insert(p);
                }
            }
        }
        if before == members.len() {
            break;
        }
    }
    for &e in &members {
        lineage(g, e, anchor)?;
    }
    Ok(members)
}
fn cyclic(g: &Graph<'_>, start: BlockId) -> bool {
    let mut pending = g
        .edges
        .iter()
        .filter_map(|(from, e)| (*from == start).then_some(e.target))
        .collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    while let Some(b) = pending.pop() {
        if b == start {
            return true;
        }
        if seen.insert(b) {
            pending.extend(
                g.edges
                    .iter()
                    .filter_map(|(from, e)| (*from == b).then_some(e.target)),
            );
        }
    }
    false
}
fn print<'a>(
    g: &Graph<'a>,
    sources: &SourceMap,
    parsed: &ParsedFile,
    scan: Span,
    wanted: &str,
) -> Result<&'a Instruction, Failure> {
    let span = one(
        parsed.ast().expressions().iter().filter_map(|(_, n)| {
            let Expression::Call {
                callee, arguments, ..
            } = n.payload()
            else {
                return None;
            };
            let [arg] = arguments.as_slice() else {
                return None;
            };
            (ast::inside(n.span(), scan)
                && sources.slice(ast::expression(parsed, *callee)) == Ok("println")
                && sources.slice(ast::expression(parsed, arg.value))
                    == Ok(format!("\"{wanted}\"").as_str()))
            .then_some(n.span())
        }),
        "original scanner marker AST",
    )?;
    one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(span)
                && matches!(
                    i.operation,
                    Operation::PrintString { .. } | Operation::PrintLiteral { .. }
                )
        }),
        "actual scanner marker instruction",
    )
}
#[allow(clippy::too_many_arguments)]
fn paths(
    g: &Graph<'_>,
    calls: [&Instruction; 2],
    drops: [&Instruction; 2],
    formed: &Instruction,
    replaced: &Instruction,
    tail: &Instruction,
    loops: &[ast::Loop],
    providers: &[provider::Evidence<'_>],
    case: &Case,
) -> Result<(), Failure> {
    let mut pending = vec![(
        g.function.entry_block().unwrap(),
        [0u8; 2],
        [0u8; 2],
        [0u8; 2],
    )];
    let mut seen = BTreeSet::new();
    while let Some((block, mut born, mut dead, mut ended)) = pending.pop() {
        if !seen.insert((block, born, dead, ended)) {
            continue;
        }
        for i in g.ops(block)? {
            for epoch in 0..2 {
                if i.id == calls[epoch].id {
                    born[epoch] += 1;
                    need(
                        born[epoch] == 1 && dead[epoch] == 0,
                        "epoch factory once and never reuse dead owner",
                        i,
                    )?;
                }
                if i.id == drops[epoch].id {
                    dead[epoch] += 1;
                    need(
                        born[epoch] == 1 && dead[epoch] == 1,
                        "epoch owner Drop once after birth",
                        i,
                    )?;
                }
            }
            if i.id == formed.id {
                need(
                    born == [1, 0] && dead[0] == u8::from(case.payload == 0 && case.history != 1),
                    "formed marker obeys true Pure/Resource ASAP",
                    i,
                )?;
            }
            if i.id == replaced.id {
                need(
                    born == [1, 1] && dead == [1, 0],
                    "assignment commits only after complete RHS and old cleanup",
                    i,
                )?;
            }
            for (l, p) in loops.iter().zip(providers) {
                if i.id == p.first.id {
                    need(
                        born[l.epoch] == 1 && dead[l.epoch] == 0,
                        "provider uses live current epoch only",
                        i,
                    )?;
                }
                if i.id == p.end.id {
                    ended[l.epoch] += 1;
                    need(
                        ended[l.epoch] == 1 && dead[l.epoch] == 0,
                        "each provider ends before consuming its epoch",
                        i,
                    )?;
                }
            }
            if i.id == tail.id {
                need(
                    dead[0] == 1 && dead[1] == u8::from(case.payload == 0),
                    "tail observes final Pure/Resource lifetime",
                    i,
                )?;
            }
        }
        if let TerminatorKind::Return { values } = &g.term(block)?.kind {
            need(
                values.is_empty()
                    && born == [1, 1]
                    && dead == [1, 1]
                    && ended == [u8::from(case.history == 1), 1],
                "every finite Unit Return drops both distinct epochs exactly once",
                g.term(block)?,
            )?;
        }
        for (_, edge) in g.edges.iter().filter(|(from, _)| *from == block) {
            pending.push((edge.target, born, dead, ended));
        }
    }
    Ok(())
}
