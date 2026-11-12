//! The new owned MutableMap rows must root and drop the original parameter on every exit.
mod entries {
    //! The original outer source is evaluated before the actual temporary provider entry.
    use crate::ssa::{
        map_provider_exits_pipeline_tests::{
            fixture::{Failure, failure, need},
            query_graph::{Graph, result},
            query_inputs::one,
        },
        model::*,
    };
    use lang_frontend::parser::{Expression, ParsedFile, Statement};

    pub(in crate::ssa::map_provider_exits_pipeline_tests) fn check(
        parsed: &ParsedFile,
        program: &Program,
    ) -> Result<(), Failure> {
        for (_, statement) in parsed.ast().statements().iter() {
            let Statement::For { source, .. } = statement.payload() else {
                continue;
            };
            let outer = parsed
                .ast()
                .expressions()
                .get(*source)
                .map_err(|e| failure("temporary Entry original outer source", e))?;
            let mut leaf = outer;
            while let Expression::Group { expression } = leaf.payload() {
                leaf = parsed
                    .ast()
                    .expressions()
                    .get(*expression)
                    .map_err(|e| failure("temporary Entry complete original Group chain", e))?;
            }
            if !matches!(leaf.payload(), Expression::Call { .. }) {
                continue;
            }
            need(
                outer.span().source_id() == parsed.source_id()
                    && leaf.span().source_id() == outer.span().source_id()
                    && outer.span().start() <= leaf.span().start()
                    && leaf.span().end() <= outer.span().end()
                    && outer.span().end() < statement.span().end(),
                "temporary Entry retains exact outer AfterExpression and canonical Call spans",
                (outer.span(), leaf.span(), statement.span()),
            )?;
            let mut selected = 0;
            for module in &program.modules {
                for function in &module.functions {
                    if !function.instructions.iter().any(|op| {
                        op.origin == Origin::Source(statement.span())
                            && matches!(op.operation, Operation::MapIterationFirst { .. })
                    }) {
                        continue;
                    }
                    selected += 1;
                    let graph = Graph::with_abort(function)?;
                    let first = one(
                        graph.instructions.iter().copied().filter(|op| {
                            op.origin == Origin::Source(statement.span())
                                && matches!(op.operation, Operation::MapIterationFirst { .. })
                        }),
                        "temporary Entry unique actual original First",
                    )?;
                    let producer = one(
                        graph.instructions.iter().copied().filter(|op| {
                            op.origin == Origin::Source(leaf.span())
                                && matches!(op.results.as_slice(), [EntityId::Value(value)]
                                if function.entity(EntityId::Value(*value)).is_some_and(|v|
                                    module.map_container(v.ty.semantic_type()).is_some()))
                        }),
                        "temporary Entry exact canonical Call owner producer",
                    )?;
                    need(
                        matches!(
                            producer.operation,
                            Operation::DirectCall { .. }
                                | Operation::CallableInvoke { .. }
                                | Operation::MapConstruct { .. }
                                | Operation::MapConsume { .. }
                        ),
                        "temporary Entry is the original call/intrinsic result, never a substitute",
                        producer,
                    )?;
                    let Operation::MapIterationFirst { source: lease } = first.operation else {
                        unreachable!()
                    };
                    let begin = graph.definition(EntityId::Loan(lease))?;
                    let Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    } = begin.operation
                    else {
                        return Err(failure("temporary Entry actual Shared source lease", begin));
                    };
                    let root = graph.definition(EntityId::Place(place))?;
                    let Operation::RootPlace { owner } = root.operation else {
                        return Err(failure("temporary Entry actual original owner root", root));
                    };
                    graph.anchored(EntityId::Value(owner), &[result(producer)?])?;
                    need(
                        producer.block == root.block
                            && root.block == begin.block
                            && begin.block == first.block
                            && graph.position(producer)? < graph.position(root)?
                            && graph.position(root)? < graph.position(begin)?
                            && graph.position(begin)? < graph.position(first)?
                            && root.origin == Origin::Source(statement.span())
                            && begin.origin == Origin::Source(statement.span()),
                        "temporary Entry follows complete outer source evaluation before acquisition",
                        (outer.span(), producer, root, begin, first),
                    )?;
                    let edge = graph.branch(first.block)?;
                    let header_owner = graph.transported(edge, EntityId::Value(owner))?;
                    let header_lease = graph.transported(edge, EntityId::Loan(lease))?;
                    let header_cursor = graph.transported(edge, result(first)?)?;
                    graph.anchored(header_owner, &[result(producer)?])?;
                    let has = one(
                        graph.ops(edge.target)?.into_iter().filter(|op| {
                            op.origin == Origin::Source(statement.span())
                                && matches!(op.operation, Operation::MapIterationHasNext { .. })
                        }),
                        "temporary Entry actual header HasNext",
                    )?;
                    need(
                        matches!(has.operation, Operation::MapIterationHasNext { source, cursor }
                            if EntityId::Loan(source) == header_lease
                                && EntityId::Value(cursor) == header_cursor)
                            && edge
                                .arguments
                                .iter()
                                .filter(|&&id| id == EntityId::Value(owner))
                                .count()
                                == 1
                            && edge
                                .arguments
                                .iter()
                                .filter(|&&id| id == EntityId::Loan(lease))
                                .count()
                                == 1,
                        "temporary Entry exact owner/lease/cursor columns and all owner predecessors",
                        (edge, header_owner, has),
                    )?;
                }
            }
            need(
                selected > 0,
                "temporary Entry original source has an actual selected provider",
                statement.span(),
            )?;
        }
        Ok(())
    }
}
use super::super::{
    fixture::{failure, need},
    query_graph::Graph,
};
use super::*;
use crate::ssa::model::*;
pub(in super::super) use entries::check as temporary_entries;
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
    crate::ssa::map_provider_exits_pipeline_tests::local_exits::owner_flow::check(
        &graph,
        module,
        &BTreeSet::from([anchor]),
    )?;
    let owners = edge_family(&graph, anchor)?;
    let leases = edge_family(&graph, EntityId::Loan(source))?;
    let mut expected = owners
        .iter()
        .chain(&leases)
        .copied()
        .collect::<BTreeSet<_>>();
    expected.insert(EntityId::Place(place));
    // resource_forms shares this observer and has an original post-For
    // readMap(source). Only that exact source call may add argument capabilities.
    for (_, node) in parsed.ast().expressions().iter() {
        if node.span().source_id() != scan.source_id()
            || node.span().start() <= scan.start()
            || node.span().end() >= scan.end()
        {
            continue;
        }
        let lang_frontend::parser::Expression::Call {
            callee, arguments, ..
        } = node.payload()
        else {
            continue;
        };
        let callee_span = parsed.ast().expressions().get(*callee).unwrap().span();
        if sources.slice(callee_span) != Ok("readMap") {
            continue;
        }
        let declaration = super::super::query_inputs::one(parsed.roots().iter().filter_map(|id| {
            let declaration = parsed.ast().items().get(*id).ok()?;
            matches!(declaration.payload(), Item::Function { name: NameMarker::Present(name), .. }
                if sources.slice(*name) == Ok("readMap")).then_some(declaration.span())
        }), "owned source original post-provider readMap declaration")?;
        let target = super::super::query_graph::function(module, declaration)?;
        let call = super::super::query_inputs::one(graph.instructions.iter().copied().filter(|op| {
            op.origin == Origin::Source(node.span())
                && matches!(op.operation, Operation::DirectCall { callee, .. } if callee == target.id)
        }), "owned source exact original post-provider readMap call")?;
        let [argument] = arguments.as_slice() else {
            return Err(failure("owned source original readMap argument", arguments));
        };
        let argument_span = parsed
            .ast()
            .expressions()
            .get(argument.value)
            .unwrap()
            .span();
        let Operation::DirectCall {
            receiver: None,
            arguments,
            ..
        } = &call.operation
        else {
            return Err(failure("owned source actual readMap ABI", call));
        };
        let [argument @ EntityId::Loan(_)] = arguments.as_slice() else {
            return Err(failure("owned source actual readMap Shared argument", call));
        };
        let root = super::super::query_inputs::one(graph.instructions.iter().copied().filter(|op| {
            op.origin == Origin::Source(argument_span)
                && matches!(op.operation, Operation::RootPlace { owner } if owners.contains(&EntityId::Value(owner)))
        }), "owned source original readMap argument exact current RootPlace")?;
        let Operation::RootPlace { owner } = root.operation else {
            unreachable!()
        };
        expected.extend(
            super::super::scope_future_observer::graph::observe_capabilities(
                &graph,
                call,
                *argument,
                owner,
                argument_span,
                node.span(),
            )?,
        );
    }
    super::super::query_inventory::check(&graph, &[ty.semantic_type()], &expected)?;
    need(returns > 0, "actual normal exit checked", returns)
}

fn edge_family(g: &Graph<'_>, anchor: EntityId) -> Result<BTreeSet<EntityId>, Failure> {
    let mut members = BTreeSet::from([anchor]);
    loop {
        let before = members.len();
        for (_, edge) in &g.edges {
            for (&argument, &parameter) in
                edge.arguments.iter().zip(&g.block(edge.target)?.parameters)
            {
                if members.contains(&argument) {
                    members.insert(parameter);
                }
            }
        }
        if before == members.len() {
            break;
        }
    }
    for &member in &members {
        g.anchored(member, &[anchor])?;
    }
    Ok(members)
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
