//! The new owned MutableMap rows must root and drop the original parameter on every exit.
use super::super::{
    fixture::{failure, need},
    query_graph::Graph,
};
use super::*;
use crate::ssa::model::*;
use lang_frontend::parser::{Item, NameMarker};
use std::collections::{BTreeMap, BTreeSet};

pub(in super::super) fn owned_map(
    sources: &SourceMap,
    parsed: &lang_frontend::parser::ParsedFile,
    program: &Program,
    mutable: bool,
) -> Result<(), Failure> {
    let scan = parsed
        .ast()
        .items()
        .iter()
        .find_map(|(_, n)| match n.payload() {
            Item::Function {
                name: NameMarker::Present(name),
                ..
            } if sources.slice(*name) == Ok("scan") => Some(n.span()),
            _ => None,
        });
    let scan = scan.ok_or_else(|| failure("original owned scan AST", parsed.source_id()))?;
    let function = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .find(|f| f.origin == Origin::Source(scan))
        .ok_or_else(|| failure("original scan", scan))?;
    let &anchor = function
        .block(function.entry_block().unwrap())
        .unwrap()
        .parameters
        .first()
        .ok_or_else(|| failure("actual owned source parameter", function.id))?;
    let ty = function.entity(anchor).unwrap().ty;
    let module = program.module(function.id.module()).unwrap();
    need(
        matches!(ty,EntityType::Value(t) if matches!(module.type_kind(t),Some(SsaTypeKind::MapContainer {kind,..}) if *kind==if mutable{MapContainerKind::MutableMap}else{MapContainerKind::Map})),
        "actual owned Map parameter ABI",
        ty,
    )?;
    let graph = Graph::with_abort(function)?;
    let roots = graph
        .instructions
        .iter()
        .filter(
            |i| matches!(i.operation,Operation::RootPlace{owner} if EntityId::Value(owner)==anchor),
        )
        .collect::<Vec<_>>();
    need(
        roots.len() == 1,
        "owned Map original parameter root",
        &roots,
    )?;
    let first = super::super::query_inputs::one(
        graph
            .instructions
            .iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::MapIterationFirst { .. })),
        "actual provider First",
    )?;
    let Operation::MapIterationFirst { source } = first.operation else {
        unreachable!()
    };
    let birth = graph.definition(EntityId::Loan(source))?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = birth.operation
    else {
        return Err(failure("provider original source acquisition", birth));
    };
    let root = graph.definition(EntityId::Place(place))?;
    need(
        roots[0].id == root.id,
        "actual provider First roots the original parameter",
        (root, first),
    )?;
    let mut drops = BTreeSet::new();
    for i in &graph.instructions {
        if let Operation::Drop { owner } = i.operation
            && function.entity(EntityId::Value(owner)).unwrap().ty == ty
        {
            transport(&graph, EntityId::Value(owner), anchor)?;
            drops.insert(i.id);
        }
    }
    need(!drops.is_empty(), "owned Map real Drop", &drops)?;
    // A cycle may execute arbitrarily often; source Drop must be outside every cycle.
    for instruction in graph.instructions.iter().filter(|i| drops.contains(&i.id)) {
        let block = instruction.block;
        let mut pending = graph
            .edges
            .iter()
            .filter(|(from, _)| *from == block)
            .map(|(_, e)| e.target)
            .collect::<Vec<_>>();
        let mut seen = BTreeSet::new();
        while let Some(next) = pending.pop() {
            need(
                next != block,
                "source Drop is outside all CFG cycles",
                instruction,
            )?;
            if seen.insert(next) {
                pending.extend(
                    graph
                        .edges
                        .iter()
                        .filter(|(from, _)| *from == next)
                        .map(|(_, e)| e.target),
                );
            }
        }
    }

    let mut pending = vec![(function.entry_block().unwrap(), 0, BTreeSet::new())];
    let mut returns = 0;
    while let Some((block, count, mut visited)) = pending.pop() {
        if !visited.insert(block) {
            continue;
        }
        let count = count
            + graph
                .ops(block)?
                .iter()
                .filter(|i| drops.contains(&i.id))
                .count();
        if matches!(graph.term(block)?.kind, TerminatorKind::Return { .. }) {
            returns += 1;
            need(
                count == 1,
                "every normal return drops original Map once",
                (block, count),
            )?;
        }
        for (_, edge) in graph.edges.iter().filter(|(from, _)| *from == block) {
            pending.push((edge.target, count, visited.clone()));
        }
    }
    need(returns > 0, "actual normal exit checked", returns)
}
fn transport(g: &Graph<'_>, current: EntityId, anchor: EntityId) -> Result<(), Failure> {
    let mut pending = vec![current];
    let mut parents = BTreeMap::new();
    while let Some(entity) = pending.pop() {
        if entity == anchor || parents.contains_key(&entity) {
            continue;
        }
        let data = g
            .function
            .entity(entity)
            .ok_or_else(|| failure("owner entity", entity))?;
        let Definition::BlockParameter { block, .. } = data.definition else {
            return Err(failure("unrelated owner", data));
        };
        need(
            data.ty == g.function.entity(anchor).unwrap().ty,
            "owner type",
            data,
        )?;
        let incoming = g.incoming(block);
        need(!incoming.is_empty(), "full predecessor set", block)?;
        let values = incoming
            .iter()
            .map(|(_, e)| g.argument(e, entity))
            .collect::<Result<Vec<_>, _>>()?;
        pending.extend(values.iter().copied());
        parents.insert(entity, values);
    }
    let mut reached = BTreeSet::from([anchor]);
    loop {
        let before = reached.len();
        for (entity, values) in &parents {
            if values.iter().any(|v| reached.contains(v)) {
                reached.insert(*entity);
            }
        }
        if reached.len() == before {
            break;
        }
    }
    need(
        parents.keys().all(|e| reached.contains(e)),
        "all incoming reach original owner",
        parents,
    )
}
