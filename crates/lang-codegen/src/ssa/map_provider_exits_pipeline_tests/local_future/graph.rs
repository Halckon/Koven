//! Actual Call anchors, full incoming identities and final Pure-owner lifetime.
mod continuation;
mod provider;
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
