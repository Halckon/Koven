//! Independent SSA proof for factory-formed Resource locals and their lexical cleanup.
mod provider;
pub(in crate::ssa::map_provider_exits_pipeline_tests) mod storage;
use super::Case;
use crate::ssa::{
    map_provider_exits_pipeline_tests::{
        fixture::{Failure, failure, need},
        query_graph::{Graph, Uses, loan, result, value},
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

pub(super) fn check(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    program: &Program,
    unit: bool,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "local Resource module")?;
    for (scanner, maker) in [("scan", "make"), ("scanEmpty", "makeEmpty")] {
        let site = site(case, sources, parsed, scanner, maker)?;
        let scan = function(module, site.scan)?;
        let factory = function(module, site.factory)?;
        let g = Graph::new(scan)?;
        need(
            g.block(scan.entry_block().unwrap())?.parameters.is_empty(),
            "local scanner has no source parameter",
            scan.id,
        )?;
        let producer = one(g.instructions.iter().copied().filter(|i| i.origin==Origin::Source(site.call)
            && matches!(&i.operation,Operation::DirectCall{callee,receiver:None,arguments} if *callee==factory.id && arguments.is_empty())), "original factory call formation")?;
        let anchor = result(producer)?;
        need(
            matches!(anchor, EntityId::Value(_)),
            "factory delivers Value owner",
            producer,
        )?;
        let map_ty = scan.entity(anchor).unwrap().ty.semantic_type();
        need(
            factory.return_types.as_slice() == [map_ty]
                && factory
                    .block(factory.entry_block().unwrap())
                    .unwrap()
                    .parameters
                    .is_empty(),
            "actual factory Value return ABI",
            factory,
        )?;
        if maker == "makeEmpty" {
            let fg = Graph::new(factory)?;
            let empty = one(
                fg.instructions
                    .iter()
                    .copied()
                    .filter(|i| i.operation == Operation::MapConstruct { map_type: map_ty }),
                "actual empty factory MapConstruct",
            )?;
            need(!fg.instructions.iter().any(|i| matches!(i.operation, Operation::MapPut { .. } | Operation::MapConsume { .. } | Operation::HeapAllocate { .. } | Operation::DirectCall { .. }))
                && fg.blocks.iter().filter(|&&b| matches!(&fg.term(b).unwrap().kind, TerminatorKind::Return { values } if matches!(result(empty), Ok(EntityId::Value(v)) if values.as_slice()==[v]))).count()==1,
                "empty factory returns its original zero-entry owner without payload insertion", factory.id)?;
        }
        let (kt, vt) = storage::check(
            (case.mutable, case.nullable),
            sources,
            parsed,
            module,
            map_ty,
        )?;
        let owners = family(&g, anchor)?;
        let mut uses = Uses::default();
        for owner in &owners {
            uses.protect(*owner);
        }
        let formed = marker(&g, sources, parsed, site.scan, "formed")?;
        need(
            producer.block == scan.entry_block().unwrap()
                && formed.block == producer.block
                && g.position(producer)? < g.position(formed)?,
            "one real factory before formed marker",
            producer,
        )?;
        let proof = provider::check(
            case,
            &g,
            module,
            sources,
            parsed,
            &site,
            &owners,
            (kt, vt),
            &mut uses,
            unit,
        )?;
        need(
            proof.first.block == producer.block && g.position(formed)? < g.position(proof.first)?,
            "formed owner before First",
            proof.first,
        )?;
        let drops=g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::Drop{owner} if scan.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()==map_ty)).collect::<Vec<_>>();
        need(
            drops.len() == if case.exit == 3 { 2 } else { 1 },
            "Resource owner has only its actual lexical Return Drop sites",
            &drops,
        )?;
        for drop in &drops {
            let Operation::Drop { owner } = drop.operation else {
                unreachable!()
            };
            need(
                owners.contains(&EntityId::Value(owner))
                    && drop.origin == Origin::Source(site.binding)
                    && !cyclic(&g, drop.block),
                "exact original local owner Drop outside cycles",
                drop,
            )?;
            lineage(&g, EntityId::Value(owner), anchor)?;
            let expected = if case.exit == 3 && drop.block == proof.body {
                proof.body_owner
            } else {
                proof.normal_owner
            };
            need(
                EntityId::Value(owner) == expected,
                "Drop consumes exact current terminal owner column",
                (drop, expected),
            )?;
            need(
                matches!(g.term(drop.block)?.kind, TerminatorKind::Return { .. }),
                "Resource Drop belongs to a real callable Return cleanup block",
                drop,
            )?;
            uses.permit(EntityId::Value(owner), drop);
        }
        let after = marker(&g, sources, parsed, site.scan, "after")?;
        let returned = returns(
            case,
            &g,
            module,
            (sources, parsed),
            &site,
            &proof,
            &mut uses,
        )?;
        uses.check(&g, returned)?;
        crate::ssa::map_provider_exits_pipeline_tests::local_exits::owner_flow::check(
            &g,
            module,
            &BTreeSet::from([anchor]),
        )?;
        paths(&g, producer, formed, after, &proof.ends, &drops)?;
    }
    Ok(())
}

struct Site {
    scan: Span,
    factory: Span,
    call: Span,
    binding: Span,
    for_span: Span,
    body: StatementId,
}
fn inside(span: Span, outer: Span) -> bool {
    span.source_id() == outer.source_id()
        && span.start() > outer.start()
        && span.end() < outer.end()
}
fn declaration(parsed: &ParsedFile, sources: &SourceMap, name: &str) -> Result<Span, Failure> {
    one(parsed.roots().iter().filter_map(|id|{let n=parsed.ast().items().get(*id).ok()?;matches!(n.payload(),Item::Function{name:NameMarker::Present(s),..} if sources.slice(*s)==Ok(name)).then_some(n.span())}),"original callable declaration")
}
fn function(module: &Module, span: Span) -> Result<&Function, Failure> {
    one(
        module
            .functions
            .iter()
            .filter(|f| f.origin == Origin::Source(span)),
        "actual original callable",
    )
}
fn site(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    scanner: &str,
    maker: &str,
) -> Result<Site, Failure> {
    let scan = declaration(parsed, sources, scanner)?;
    let (for_span, source, body) = one(
        parsed
            .ast()
            .statements()
            .iter()
            .filter_map(|(_, n)| match n.payload() {
                Statement::For { source, body, .. } if inside(n.span(), scan) => {
                    Some((n.span(), *source, *body))
                }
                _ => None,
            }),
        "original local For",
    )?;
    let source = parsed.ast().expressions().get(source).unwrap();
    need(
        source.payload() == &Expression::Name && sources.slice(source.span()) == Ok("source"),
        "original local source reference",
        source,
    )?;
    let binding = |wanted: &str| {
        one(
            parsed
                .ast()
                .items()
                .iter()
                .filter_map(|(_, n)| match n.payload() {
                    Item::Variable {
                        name: NameMarker::Present(name),
                        initializer,
                        kind,
                        ..
                    } if inside(n.span(), scan)
                        && n.span().end() < for_span.start()
                        && sources.slice(*name) == Ok(wanted) =>
                    {
                        Some((*name, *initializer, *kind))
                    }
                    _ => None,
                }),
            "original formation binding",
        )
    };
    let (name, mut init, kind) = binding("source")?;
    need(
        kind == if case.form == 1 {
            VariableKind::Var
        } else {
            VariableKind::Val
        },
        "actual val/var source",
        kind,
    )?;
    if case.form == 2 {
        let alias = parsed.ast().expressions().get(init).unwrap();
        need(
            alias.payload() == &Expression::Name && sources.slice(alias.span()) == Ok("original"),
            "actual moved alias",
            alias,
        )?;
        let (original, producer, original_kind) = binding("original")?;
        need(
            original_kind == VariableKind::Val && original.start() < name.start(),
            "actual source follows original formation",
            original,
        )?;
        init = producer;
    }
    let call = parsed.ast().expressions().get(init).unwrap();
    let Expression::Call {
        callee, arguments, ..
    } = call.payload()
    else {
        return Err(failure("factory AST Call", call));
    };
    need(
        arguments.is_empty()
            && sources.slice(parsed.ast().expressions().get(*callee).unwrap().span()) == Ok(maker),
        "actual zero-argument factory expression",
        call,
    )?;
    Ok(Site {
        scan,
        factory: declaration(parsed, sources, maker)?,
        call: call.span(),
        binding: name,
        for_span,
        body,
    })
}

// Follow every actual incoming; reject foreign producers and unanchored cycles.
fn lineage(g: &Graph<'_>, current: EntityId, anchor: EntityId) -> Result<(), Failure> {
    let ty = g.function.entity(anchor).unwrap().ty;
    let mut pending = vec![current];
    let mut dependencies = BTreeMap::new();
    while let Some(entity) = pending.pop() {
        if entity == anchor || dependencies.contains_key(&entity) {
            continue;
        }
        let data = g
            .function
            .entity(entity)
            .ok_or_else(|| failure("actual transport entity", entity))?;
        let Definition::BlockParameter { block, .. } = data.definition else {
            return Err(failure("foreign owner/lease producer", data));
        };
        need(data.ty == ty, "transport preserves represented type", data)?;
        let incoming = g.incoming(block);
        need(!incoming.is_empty(), "full predecessor set", entity)?;
        let values = incoming
            .iter()
            .map(|(_, edge)| g.argument(edge, entity))
            .collect::<Result<Vec<_>, _>>()?;
        pending.extend(values.iter().copied());
        dependencies.insert(entity, values);
    }
    let mut rooted = BTreeSet::from([anchor]);
    loop {
        let before = rooted.len();
        for (&entity, values) in &dependencies {
            if values.iter().any(|v| rooted.contains(v)) {
                rooted.insert(entity);
            }
        }
        if rooted.len() == before {
            break;
        }
    }
    need(
        dependencies.keys().all(|e| rooted.contains(e)),
        "all incoming eventually reach immutable factory/lease anchor",
        dependencies,
    )
}
fn family(g: &Graph<'_>, anchor: EntityId) -> Result<BTreeSet<EntityId>, Failure> {
    let mut members = BTreeSet::from([anchor]);
    loop {
        let before = members.len();
        for (_, edge) in &g.edges {
            for (&arg, &parameter) in edge.arguments.iter().zip(&g.block(edge.target)?.parameters) {
                if members.contains(&arg) {
                    members.insert(parameter);
                }
            }
        }
        if before == members.len() {
            break;
        }
    }
    for &member in &members {
        lineage(g, member, anchor)?;
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
fn marker<'a>(
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
            let [a] = arguments.as_slice() else {
                return None;
            };
            (inside(n.span(), scan)
                && sources.slice(parsed.ast().expressions().get(*callee).ok()?.span())
                    == Ok("println")
                && sources.slice(parsed.ast().expressions().get(a.value).ok()?.span())
                    == Ok(format!("\"{wanted}\"").as_str()))
            .then_some(n.span())
        }),
        "original marker call",
    )?;
    one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(span)
                && matches!(
                    i.operation,
                    Operation::PrintString { .. } | Operation::PrintLiteral { .. }
                )
        }),
        "actual marker call instruction",
    )
}

fn returns(
    case: &Case,
    g: &Graph<'_>,
    module: &Module,
    (sources, parsed): (&SourceMap, &ParsedFile),
    site: &Site,
    p: &provider::Evidence<'_>,
    uses: &mut Uses,
) -> Result<Option<(BlockId, ValueId)>, Failure> {
    let original = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, n)| match n.payload() {
            Expression::Return { value: Some(v), .. } if inside(n.span(), site.scan) => {
                Some((n.span(), *v))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let (outer_span, zero) = one(
        original
            .iter()
            .copied()
            .filter(|(s, _)| s.start() > site.for_span.end()),
        "original outer Return zero",
    )?;
    let zero_ast = parsed.ast().expressions().get(zero).unwrap();
    need(
        sources.slice(zero_ast.span()) == Ok("0"),
        "original fallback is zero",
        zero_ast,
    )?;
    let zero = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(zero_ast.span())
                && i.operation == Operation::Constant(ScalarConstant::Integer(0))
        }),
        "actual fallback zero value",
    )?;
    let outer=one(g.blocks.iter().copied().filter(|&b|g.term(b).is_ok_and(|t|t.origin==Origin::Source(outer_span) && matches!(&t.kind,TerminatorKind::Return{values} if values.as_slice()==[value(zero).unwrap()]))),"actual fallback Return delivers original zero")?;
    scalar_use(g, module, result(zero)?, outer)?;
    need(
        original.len() == if case.exit == 3 { 2 } else { 1 },
        "only original body and outer Returns",
        &original,
    )?;
    need(
        g.blocks
            .iter()
            .filter(|&&b| matches!(g.term(b).unwrap().kind, TerminatorKind::Return { .. }))
            .count()
            == original.len(),
        "every SSA Return has original AST site",
        g.function.id,
    )?;
    if case.exit != 3 {
        return Ok(None);
    }
    let body = parsed.ast().statements().get(site.body).unwrap().span();
    let (span, operand) = one(
        original.iter().copied().filter(|(s, _)| inside(*s, body)),
        "original direct body Return",
    )?;
    let ast = parsed.ast().expressions().get(operand).unwrap();
    let Expression::Call {
        callee, arguments, ..
    } = ast.payload()
    else {
        return Err(failure("actual Return operand Call", ast));
    };
    need(
        arguments.len() == 2
            && sources.slice(parsed.ast().expressions().get(*callee).unwrap().span())
                == Ok("result"),
        "original result callee",
        ast,
    )?;
    for (a, wanted) in arguments.iter().zip(["key", "value"]) {
        need(
            sources.slice(parsed.ast().expressions().get(a.value).unwrap().span()) == Ok(wanted),
            "original result argument",
            a.value,
        )?;
    }
    let callee = function(module, declaration(parsed, sources, "result")?)?;
    let call=one(g.instructions.iter().copied().filter(|i|i.origin==Origin::Source(ast.span()) && matches!(&i.operation,Operation::DirectCall{callee:c,receiver:None,arguments} if *c==callee.id && arguments.as_slice()==[result(p.key).unwrap(),result(p.val).unwrap()])),"actual result call uses current K/V loans")?;
    need(
        call.block == p.body
            && g.position(p.read)? < g.position(call)?
            && g.position(call)? < g.position(p.guard_drop)?,
        "return operand evaluated before body cleanup",
        call,
    )?;
    for (index, component) in [p.key, p.val].into_iter().enumerate() {
        let e = result(component)?;
        need(
            callee
                .block(callee.entry_block().unwrap())
                .unwrap()
                .parameters
                .get(index)
                .and_then(|id| callee.entity(*id))
                .is_some_and(|d| d.ty == g.function.entity(e).unwrap().ty),
            "result actual Shared component ABI",
            callee.id,
        )?;
        uses.permit(e, call);
    }
    let returned = value(call)?;
    let term = g.term(p.body)?;
    need(
        term.origin == Origin::Source(span)
            && matches!(&term.kind,TerminatorKind::Return{values} if values.as_slice()==[returned])
            && callee.return_types.as_slice()
                == [g
                    .function
                    .entity(EntityId::Value(returned))
                    .unwrap()
                    .ty
                    .semantic_type()],
        "original Return delivers actual evaluated result",
        term,
    )?;
    scalar_use(g, module, EntityId::Value(returned), p.body)?;
    uses.protect(EntityId::Value(returned));
    Ok(Some((p.body, returned)))
}
fn scalar_use(
    g: &Graph<'_>,
    module: &Module,
    entity: EntityId,
    returned: BlockId,
) -> Result<(), Failure> {
    need(g.function.entity(entity).is_some_and(|e|matches!(e.ty,EntityType::Value(t) if module.type_kind(t)==Some(&SsaTypeKind::Integer{bits:32,signed:true}))),"original Int result storage",entity)?;
    need(
        g.instructions
            .iter()
            .all(|i| !i.operation.entities().contains(&entity))
            && g.edges.iter().all(|(_, e)| !e.arguments.contains(&entity))
            && g.blocks
                .iter()
                .all(|&b| b == returned || !g.term(b).unwrap().kind.entities().contains(&entity)),
        "scalar delivered once without cleanup or transport consumption",
        entity,
    )
}
fn paths(
    g: &Graph<'_>,
    producer: &Instruction,
    formed: &Instruction,
    after: &Instruction,
    ends: &[&Instruction],
    drops: &[&Instruction],
) -> Result<(), Failure> {
    let mut pending = vec![(g.function.entry_block().unwrap(), 0u8, 0u8, 0u8, false)];
    let mut seen = BTreeSet::new();
    while let Some((block, mut produced, mut ended, mut dropped, mut marked)) = pending.pop() {
        if !seen.insert((block, produced, ended, dropped, marked)) {
            continue;
        }
        for i in g.ops(block)? {
            if i.id == producer.id {
                produced += 1;
                need(produced == 1, "factory once per finite path", i)?;
            }
            if i.id == formed.id {
                need(
                    produced == 1 && dropped == 0,
                    "formed marker sees live real owner",
                    i,
                )?;
                marked = true;
            }
            if ends.iter().any(|e| e.id == i.id) {
                ended += 1;
                need(
                    ended == 1 && dropped == 0,
                    "each exit ends exactly its own source once",
                    i,
                )?;
            }
            if i.id == after.id {
                need(
                    ended == 1 && dropped == 0,
                    "Resource survives provider through ordinary continuation",
                    i,
                )?;
            }
            if drops.iter().any(|d| d.id == i.id) {
                dropped += 1;
                need(
                    ended == 1 && dropped == 1,
                    "one lexical Resource Map Drop after source End",
                    i,
                )?;
            }
        }
        if matches!(g.term(block)?.kind, TerminatorKind::Return { .. }) {
            need(
                produced == 1 && marked && ended == 1 && dropped == 1,
                "every finite Return has factory/source End/Map Drop once",
                g.term(block)?,
            )?;
        }
        for (_, edge) in g.edges.iter().filter(|(from, _)| *from == block) {
            pending.push((edge.target, produced, ended, dropped, marked));
        }
    }
    Ok(())
}
