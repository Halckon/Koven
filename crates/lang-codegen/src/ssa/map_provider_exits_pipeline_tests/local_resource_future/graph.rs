//! Original Call ownership across providers and future loans, retained until lexical Return.
mod anchors {
    //! Read only original declaration/formation anchors and complete actual transport columns.
    use super::*;
    pub(super) fn inside(span: Span, outer: Span) -> bool {
        span.source_id() == outer.source_id()
            && span.start() > outer.start()
            && span.end() < outer.end()
    }
    pub(super) fn declaration(
        parsed: &ParsedFile,
        sources: &SourceMap,
        name: &str,
    ) -> Result<Span, Failure> {
        one(parsed.roots().iter().filter_map(|id|{let n=parsed.ast().items().get(*id).ok()?;
            matches!(n.payload(),Item::Function {name:NameMarker::Present(s),..} if sources.slice(*s)==Ok(name)).then_some(n.span())}),"original callable declaration")
    }
    pub(super) fn site(
        case: &Case,
        sources: &SourceMap,
        parsed: &ParsedFile,
        scanner: &str,
        maker: &str,
    ) -> Result<Site, Failure> {
        let scan = declaration(parsed, sources, scanner)?;
        let mut loops = parsed
            .ast()
            .statements()
            .iter()
            .filter_map(|(_, n)| match n.payload() {
                Statement::For { source, body, .. } if inside(n.span(), scan) => {
                    Some((n.span(), *source, *body))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        loops.sort_by_key(|(s, _, _)| s.start());
        need(
            loops.len() == if case.form == 2 { 2 } else { 1 },
            "all original scanner For sites",
            &loops,
        )?;
        for (_, source, _) in &loops {
            let n = parsed.ast().expressions().get(*source).unwrap();
            need(
                n.payload() == &Expression::Name && sources.slice(n.span()) == Ok("source"),
                "original source Name for each For",
                n,
            )?;
        }
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
                            && n.span().end() < loops[0].0.start()
                            && sources.slice(*name) == Ok(wanted) =>
                        {
                            Some((*name, *initializer, *kind))
                        }
                        _ => None,
                    }),
                "actual original source declaration",
            )
        };
        let (name, mut initializer, kind) = binding("source")?;
        need(
            kind == if case.binding == 1 {
                VariableKind::Var
            } else {
                VariableKind::Val
            },
            "actual selected val/var formation",
            kind,
        )?;
        if case.binding == 2 {
            let alias = parsed.ast().expressions().get(initializer).unwrap();
            need(
                alias.payload() == &Expression::Name
                    && sources.slice(alias.span()) == Ok("original"),
                "original moved alias Name",
                alias,
            )?;
            let (original, init, kind) = binding("original")?;
            need(
                kind == VariableKind::Val && original.start() < name.start(),
                "original factory precedes alias Move",
                original,
            )?;
            initializer = init;
        }
        let call = parsed.ast().expressions().get(initializer).unwrap();
        need(
            matches!(call.payload(),Expression::Call {callee,arguments,..} if arguments.is_empty() && sources.slice(parsed.ast().expressions().get(*callee).unwrap().span())==Ok(maker)),
            "actual selected zeroarg factory Call",
            call,
        )?;
        Ok(Site {
            scan,
            binding: name,
            call: call.span(),
            factory: declaration(parsed, sources, maker)?,
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

    pub(super) fn family(g: &Graph<'_>, anchor: EntityId) -> Result<BTreeSet<EntityId>, Failure> {
        let mut members = BTreeSet::from([anchor]);
        loop {
            let before = members.len();
            for (_, edge) in &g.edges {
                for (&arg, &parameter) in
                    edge.arguments.iter().zip(&g.block(edge.target)?.parameters)
                {
                    if members.contains(&arg) {
                        members.insert(parameter);
                    }
                }
            }
            if members.len() == before {
                break;
            }
        }
        for &member in &members {
            transport(g, member, anchor)?;
        }
        Ok(members)
    }
    // Every dependency is inspected; a recurrent column also needs a real anchor path.
    fn transport(g: &Graph<'_>, current: EntityId, anchor: EntityId) -> Result<(), Failure> {
        let ty = g.function.entity(anchor).unwrap().ty;
        let mut pending = vec![current];
        let mut incoming = BTreeMap::new();
        while let Some(e) = pending.pop() {
            if e == anchor || incoming.contains_key(&e) {
                continue;
            }
            let d = g
                .function
                .entity(e)
                .ok_or_else(|| failure("actual current transport entity", e))?;
            let Definition::BlockParameter { block, .. } = d.definition else {
                return Err(failure("foreign current owner/loan producer", d));
            };
            need(d.ty == ty, "current transport keeps represented type", d)?;
            let edges = g.incoming(block);
            need(!edges.is_empty(), "complete nonempty predecessor set", e)?;
            let args = edges
                .iter()
                .map(|(_, edge)| g.argument(edge, e))
                .collect::<Result<Vec<_>, _>>()?;
            pending.extend(args.iter().copied());
            incoming.insert(e, args);
        }
        let mut rooted = BTreeSet::from([anchor]);
        loop {
            let before = rooted.len();
            for (&e, args) in &incoming {
                if args.iter().any(|a| rooted.contains(a)) {
                    rooted.insert(e);
                }
            }
            if before == rooted.len() {
                break;
            }
        }
        need(
            incoming.keys().all(|e| rooted.contains(e)),
            "all current incoming paths eventually reach original real anchor",
            incoming,
        )
    }
    pub(super) fn reaches(g: &Graph<'_>, from: BlockId, to: BlockId, include_start: bool) -> bool {
        let mut pending = if include_start {
            vec![from]
        } else {
            g.edges
                .iter()
                .filter_map(|(b, e)| (*b == from).then_some(e.target))
                .collect()
        };
        let mut seen = BTreeSet::new();
        while let Some(b) = pending.pop() {
            if b == to {
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
    pub(super) fn print<'a, 'p>(
        q: &Query<'a, 'p>,
        outer: Span,
        wanted: &str,
    ) -> Result<&'p Instruction, Failure> {
        let (span, arg) = one(
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
                let arg = q.parsed.ast().expressions().get(a.value).ok()?.span();
                (inside(n.span(), outer)
                    && q.sources
                        .slice(q.parsed.ast().expressions().get(*callee).ok()?.span())
                        == Ok("println")
                    && q.sources.slice(arg) == Ok(format!("\"{wanted}\"").as_str()))
                .then_some((n.span(), arg))
            }),
            "original scanner marker call",
        )?;
        let i = one(
            q.g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(span)
                    && matches!(
                        i.operation,
                        Operation::PrintString { .. } | Operation::PrintLiteral { .. }
                    )
            }),
            "actual scanner marker instruction",
        )?;
        match &i.operation {
            Operation::PrintLiteral { bytes } => {
                let mut expected = wanted.as_bytes().to_vec();
                expected.push(b'\n');
                need(*bytes == expected, "marker actual bytes", i)?;
            }
            Operation::PrintString { value: loan } => {
                let birth = q.g.definition(EntityId::Loan(*loan))?;
                let Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                } = birth.operation
                else {
                    return Err(failure("marker Shared String loan", birth));
                };
                let root = q.g.definition(EntityId::Place(place))?;
                let Operation::RootPlace { owner } = root.operation else {
                    return Err(failure("marker actual String root", root));
                };
                let literal = q.g.definition(EntityId::Value(owner))?;
                need(
                    literal.origin == Origin::Source(arg)
                        && matches!(&literal.operation,Operation::StringLiteral {bytes,..} if bytes==wanted.as_bytes()),
                    "marker actual String literal identity",
                    literal,
                )?;
            }
            _ => unreachable!(),
        }
        Ok(i)
    }
    pub(super) fn reader(q: &Query<'_, '_>, nullable: bool) -> Result<(), Failure> {
        let reader = function(q.module, declaration(q.parsed, q.sources, "read")?)?;
        let rg = Graph::new(reader)?;
        let params = &rg.block(reader.entry_block().unwrap())?.parameters;
        need(
            params.len() == 2
                && params.iter().zip([q.types.0, q.types.1]).all(|(p, t)| {
                    reader.entity(*p).unwrap().ty
                        == EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: t,
                        }
                }),
            "actual reader Shared Resource component ABI",
            params,
        )?;
        let tests = rg
            .instructions
            .iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::NullableLoanIsNull { .. }))
            .collect::<Vec<_>>();
        need(tests.len()==usize::from(nullable) && tests.iter().all(|i|matches!(i.operation,Operation::NullableLoanIsNull {source} if EntityId::Loan(source)==params[1])),"nullable check uses actual Shared parameter without owner extraction",&tests)?;
        let resource = match q.module.type_kind(q.types.1) {
            Some(SsaTypeKind::NullableHandle { inner }) => *inner,
            _ => q.types.1,
        };
        need(
            !rg.instructions.iter().any(|i| {
                matches!(i.operation, Operation::NullableTake { .. })
                    || matches!(i.operation, Operation::Drop {owner} if [q.types.1,resource].contains(&reader.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()))
            }),
            "reader never consumes borrowed Resource payload",
            reader.id,
        )
    }
}
#[allow(unused_imports)]
pub(in crate::ssa) use provider::continuation;
pub(in crate::ssa) mod provider;
use super::super::local_resources::graph::storage;
use super::Case;
use crate::ssa::{
    map_provider_exits_pipeline_tests::{
        fixture::{Failure, failure, need},
        query_graph::{Graph, Uses, function, result, value},
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
    types: (SsaTypeId, SsaTypeId),
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
    let module = one(program.modules.iter(), "Resource future original module")?;
    for (scanner, maker) in [("scan", "make"), ("scanEmpty", "makeEmpty")] {
        let site = anchors::site(case, sources, parsed, scanner, maker)?;
        let scan = function(module, site.scan)?;
        let factory = function(module, site.factory)?;
        let g = Graph::new(scan)?;
        let producer = one(g.instructions.iter().copied().filter(|i| i.origin == Origin::Source(site.call)
            && matches!(&i.operation, Operation::DirectCall {callee,receiver:None,arguments} if *callee==factory.id && arguments.is_empty())), "original zeroarg factory Call")?;
        let anchor = result(producer)?;
        need(
            matches!(anchor, EntityId::Value(_))
                && g.block(scan.entry_block().unwrap())?.parameters.is_empty(),
            "actual zeroarg scanner receives factory Value owner",
            producer,
        )?;
        let map_type = scan.entity(anchor).unwrap().ty.semantic_type();
        need(
            factory.return_types.as_slice() == [map_type]
                && factory
                    .block(factory.entry_block().unwrap())
                    .unwrap()
                    .parameters
                    .is_empty(),
            "actual selected zeroarg factory return ABI",
            factory.id,
        )?;
        if maker == "makeEmpty" {
            let fg = Graph::new(factory)?;
            let empty = one(
                fg.instructions
                    .iter()
                    .copied()
                    .filter(|i| i.operation == Operation::MapConstruct { map_type }),
                "actual empty MapConstruct",
            )?;
            need(!fg.instructions.iter().any(|i| matches!(i.operation, Operation::MapPut {..}|Operation::MapConsume {..}|Operation::HeapAllocate {..}|Operation::DirectCall {..}))
                && fg.blocks.iter().filter(|&&b| matches!(&fg.term(b).unwrap().kind,TerminatorKind::Return {values} if values.as_slice()==[value(empty).unwrap()])).count()==1,
                "empty factory returns its original owner without payload construction", factory.id)?;
        }
        let types = storage::check(
            (case.mutable, case.nullable),
            sources,
            parsed,
            module,
            map_type,
        )?;
        let q = Query {
            g: &g,
            module,
            sources,
            parsed,
            types,
        };
        anchors::reader(&q, case.nullable)?;
        let owners = anchors::family(&g, anchor)?;
        let mut uses = Uses::default();
        for &owner in &owners {
            uses.protect(owner);
        }
        let formed = anchors::print(&q, site.scan, "formed")?;
        let first = one(
            g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(site.loops[0].span)
                    && matches!(i.operation, Operation::MapIterationFirst { .. })
            }),
            "original first provider",
        )?;
        need(
            producer.block == scan.entry_block().unwrap()
                && formed.block == producer.block
                && first.block == producer.block
                && g.position(producer)? < g.position(formed)?
                && g.position(formed)? < g.position(first)?,
            "real Call before formed before provider",
            producer,
        )?;
        let mut current = anchor;
        let mut ends = Vec::new();
        for loop_site in &site.loops {
            let proof = provider::check(&q, loop_site, current, &owners, &mut uses)?;
            current = proof.current;
            ends.push(proof.ends);
        }
        need(
            g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapIterationFirst { .. }))
                .count()
                == site.loops.len(),
            "all actual For providers queried",
            scan.id,
        )?;
        let last = continuation::check(&q, &site, case.form, current, &owners, &mut uses)?;
        let after = anchors::print(&q, site.scan, "after-last-use")?;
        let tail = anchors::print(&q, site.scan, "tail")?;
        let drop = one(g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::Drop {owner} if scan.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()==map_type)), "unique lexical Resource Map Drop")?;
        need(
            drop.operation
                == Operation::Drop {
                    owner: match last.current {
                        EntityId::Value(v) => v,
                        _ => return Err(failure("lexical current Value", last.current)),
                    },
                }
                && drop.origin == Origin::Source(site.binding)
                && tail.block == drop.block
                && g.position(tail)? < g.position(drop)?
                && !anchors::reaches(&g, drop.block, drop.block, false),
            "exact current Resource owner dropped after tail outside all cycles",
            drop,
        )?;
        need(
            anchors::reaches(&g, last.instruction.block, after.block, true)
                && (last.instruction.block != after.block
                    || g.position(last.instruction)? < g.position(after)?)
                && anchors::reaches(&g, after.block, tail.block, true)
                && (after.block != tail.block || g.position(after)? < g.position(tail)?),
            "real last use before marker before tail",
            (last.instruction, after, tail),
        )?;
        uses.permit(last.current, drop);
        let (return_span, zero_ast) = one(
            parsed
                .ast()
                .expressions()
                .iter()
                .filter_map(|(_, n)| match n.payload() {
                    Expression::Return { value: Some(v), .. }
                        if anchors::inside(n.span(), site.scan) =>
                    {
                        Some((n.span(), *v))
                    }
                    _ => None,
                }),
            "actual sole scanner Return",
        )?;
        let zero_span = parsed.ast().expressions().get(zero_ast).unwrap().span();
        need(
            sources.slice(zero_span) == Ok("0"),
            "actual scanner result zero",
            zero_span,
        )?;
        let zero = one(
            g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(zero_span)
                    && i.operation == Operation::Constant(ScalarConstant::Integer(0))
            }),
            "actual returned zero",
        )?;
        need(
            g.term(drop.block)?.origin == Origin::Source(return_span)
                && matches!(&g.term(drop.block)?.kind,TerminatorKind::Return {values} if values.as_slice()==[value(zero).unwrap()])
                && zero.block == drop.block
                && g.position(zero)? < g.position(drop)?,
            "lexical Drop follows real return operand evaluation",
            g.term(drop.block)?,
        )?;
        need(
            g.blocks
                .iter()
                .filter(|&&b| matches!(g.term(b).unwrap().kind, TerminatorKind::Return { .. }))
                .count()
                == 1,
            "all scanner Returns queried",
            scan.id,
        )?;
        uses.protect(result(zero)?);
        uses.check(&g, Some((drop.block, value(zero)?)))?;
        crate::ssa::map_provider_exits_pipeline_tests::local_exits::owner_flow::check(
            &g,
            module,
            &BTreeSet::from([anchor]),
        )?;
        paths(
            &g,
            producer.id,
            formed.id,
            after.id,
            tail.id,
            drop.id,
            &ends,
        )?;
    }
    Ok(())
}

// A finite state proof: arbitrary loop repetitions revisit the same live state.
// Every reachable Return state must have completed all providers and exactly one Drop.
fn paths(
    g: &Graph<'_>,
    producer: InstructionId,
    formed: InstructionId,
    after: InstructionId,
    tail: InstructionId,
    drop: InstructionId,
    ends: &[Vec<&Instruction>],
) -> Result<(), Failure> {
    let mut pending = vec![(
        g.function.entry_block().unwrap(),
        0u8,
        false,
        false,
        false,
        0u8,
        vec![0u8; ends.len()],
    )];
    let mut seen = BTreeSet::new();
    let mut returns = 0;
    while let Some((
        block,
        mut made,
        mut formed_seen,
        mut after_seen,
        mut tail_seen,
        mut dropped,
        mut ended,
    )) = pending.pop()
    {
        if !seen.insert((
            block,
            made,
            formed_seen,
            after_seen,
            tail_seen,
            dropped,
            ended.clone(),
        )) {
            continue;
        }
        for i in g.ops(block)? {
            if i.id == producer {
                made += 1;
                need(made == 1, "factory exactly once on every normal path", i)?;
            }
            if i.id == formed {
                need(
                    made == 1 && dropped == 0,
                    "formed marker preserves live source",
                    i,
                )?;
                formed_seen = true;
            }
            if matches!(
                i.operation,
                Operation::MapIterationFirst { .. } | Operation::MapSize { .. }
            ) {
                need(
                    formed_seen && dropped == 0,
                    "provider/size reads retain live source",
                    i,
                )?;
            }
            for (index, group) in ends.iter().enumerate() {
                if group.iter().any(|e| e.id == i.id) {
                    ended[index] += 1;
                    need(
                        ended[index] == 1 && dropped == 0,
                        "each actual provider source ends once before final Drop",
                        i,
                    )?;
                }
            }
            if i.id == after {
                need(
                    dropped == 0 && ended.iter().all(|n| *n == 1),
                    "after-last-use marker precedes Resource cleanup",
                    i,
                )?;
                after_seen = true;
            }
            if i.id == tail {
                need(
                    after_seen && dropped == 0,
                    "tail still observes live Resource ownership",
                    i,
                )?;
                tail_seen = true;
            }
            if i.id == drop {
                dropped += 1;
                need(
                    dropped == 1 && tail_seen && ended.iter().all(|n| *n == 1),
                    "one lexical Drop after tail and all source Ends",
                    i,
                )?;
            }
        }
        if let TerminatorKind::Return { values } = &g.term(block)?.kind {
            returns += 1;
            need(
                values.len() == 1
                    && made == 1
                    && formed_seen
                    && after_seen
                    && tail_seen
                    && dropped == 1
                    && ended.iter().all(|n| *n == 1),
                "every finite normal Return completes all Resource cleanup exactly once",
                g.term(block)?,
            )?;
        }
        for (_, edge) in g.edges.iter().filter(|(from, _)| *from == block) {
            pending.push((
                edge.target,
                made,
                formed_seen,
                after_seen,
                tail_seen,
                dropped,
                ended.clone(),
            ));
        }
    }
    need(
        returns > 0,
        "actual finite completion paths exist",
        g.function.id,
    )
}
