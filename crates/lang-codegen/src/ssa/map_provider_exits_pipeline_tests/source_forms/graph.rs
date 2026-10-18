//! The new owned MutableMap rows must root and drop the original parameter on every exit.
use super::super::{
    fixture::{failure, need},
    query_graph::Graph,
};
use super::*;
use crate::ssa::model::*;
use lang_frontend::parser::{Item, NameMarker};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn owned_mutable(
    sources: &SourceMap,
    parsed: &lang_frontend::parser::ParsedFile,
    program: &Program,
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
    let Some(scan) = scan else { return Ok(()) };
    let function = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .find(|f| f.origin == Origin::Source(scan))
        .ok_or_else(|| failure("original scan", scan))?;
    let expected = parsed.ast().items().iter().any(|(_, n)| {
        n.span() == scan
            && sources
                .slice(n.span())
                .is_ok_and(|text| text.contains("own source: MutableMap<String, String>"))
    });
    if !expected {
        return Ok(());
    }
    let &anchor = function
        .block(function.entry_block().unwrap())
        .unwrap()
        .parameters
        .first()
        .ok_or_else(|| failure("actual owned source parameter", function.id))?;
    let ty = function.entity(anchor).unwrap().ty;
    let module = program.module(function.id.module()).unwrap();
    need(
        matches!(ty,EntityType::Value(t) if matches!(module.type_kind(t),Some(SsaTypeKind::MapContainer {kind:MapContainerKind::MutableMap,..}))),
        "actual owned MutableMap parameter ABI",
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
        "owned MutableMap original parameter root",
        &roots,
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
    need(!drops.is_empty(), "owned MutableMap real Drop", &drops)?;
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
                "every normal return drops original MutableMap once",
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
