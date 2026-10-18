//! Independent SSA epoch identities, assignment ordering and finite-path cleanup.
mod provider;
mod storage;
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
                sources,
                parsed,
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
