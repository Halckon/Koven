//! Original Call ownership across providers and future loans, retained until lexical Return.
mod anchors;
mod continuation;
mod provider;
#[path = "../local_resources/graph/storage.rs"]
mod storage;
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
        let types = storage::check(case, sources, parsed, module, map_type)?;
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
