//! Source-anchored, complete-incoming provider and successful-path cleanup queries.
//! This test never calls production qualification or supplies substitute frontend facts.
use super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, result, value},
    query_inputs::one,
};
use crate::ssa::model::*;
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{Expression, Item, NameMarker, ParsedFile, Statement},
    source::{SourceMap, Span},
};
use std::collections::{BTreeMap, BTreeSet};

mod terminal;
use terminal::{Path, operand, walk};

fn inside(inner: Span, outer: Span) -> bool {
    inner.source_id() == outer.source_id()
        && inner.start() >= outer.start()
        && inner.end() <= outer.end()
}

fn declaration(parsed: &ParsedFile, sources: &SourceMap, name: &str) -> Result<Span, Failure> {
    one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            matches!(n.payload(), Item::Function { name: NameMarker::Present(s), .. }
            if sources.slice(*s) == Ok(name))
            .then_some(n.span())
        }),
        "ordinary CFG original callable",
    )
}

fn family(g: &Graph<'_>, anchors: &[EntityId]) -> Result<BTreeSet<EntityId>, Failure> {
    let mut members = anchors.iter().copied().collect::<BTreeSet<_>>();
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
        g.anchored(member, anchors)?;
    }
    Ok(members)
}

fn original_expression(parsed: &ParsedFile, span: Span) -> Result<ExpressionId, Failure> {
    one(
        parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(id, n)| (n.span() == span).then_some(id)),
        "ordinary CFG exact original expression",
    )
}

fn falls_through(parsed: &ParsedFile, id: StatementId) -> Result<bool, Failure> {
    let node = parsed
        .ast()
        .statements()
        .get(id)
        .map_err(|e| failure("ordinary CFG body AST", e))?;
    Ok(match node.payload() {
        Statement::Block { elements } | Statement::ControlBody { elements } => {
            let mut live = true;
            for element in elements {
                if live {
                    live = falls_through(parsed, *element)?;
                }
            }
            live
        }
        Statement::Expression { expression } => {
            match parsed
                .ast()
                .expressions()
                .get(*expression)
                .unwrap()
                .payload()
            {
                Expression::Return { .. }
                | Expression::Break { .. }
                | Expression::Continue { .. } => false,
                Expression::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    falls_through(parsed, *then_branch)?
                        || match else_branch {
                            Some(e) => falls_through(parsed, *e)?,
                            None => true,
                        }
                }
                _ => true,
            }
        }
        _ => true,
    })
}

/// Every invocation is found from its real source Call and exact selected declaration.
fn factory_calls(module: &Module, sources: &SourceMap, parsed: &ParsedFile) -> Result<(), Failure> {
    let factory = function(module, declaration(parsed, sources, "make")?)?;
    let mut expected = BTreeSet::new();
    for (_, node) in parsed.ast().expressions().iter() {
        if let Expression::Call {
            callee, arguments, ..
        } = node.payload()
        {
            if sources.slice(parsed.ast().expressions().get(*callee).unwrap().span()) == Ok("make")
            {
                need(
                    arguments.len() == 1,
                    "ordinary factory original argument arity",
                    node,
                )?;
                expected.insert((node.span().start(), node.span().end()));
            }
        }
    }
    let mut actual = BTreeSet::new();
    for f in &module.functions {
        let graph = Graph::with_abort(f)?;
        for i in &graph.instructions {
            if matches!(i.operation, Operation::DirectCall { callee, .. } if callee == factory.id) {
                let Origin::Source(span) = i.origin else {
                    return Err(failure("factory real Call origin", i));
                };
                need(
                    expected.contains(&(span.start(), span.end()))
                        && actual.insert((span.start(), span.end()))
                        && matches!(&i.operation, Operation::DirectCall { receiver: None, arguments, .. } if arguments.len() == 1),
                    "factory once per exact source evaluation",
                    i,
                )?;
            }
        }
    }
    need(
        actual == expected,
        "complete original factory Call set",
        (actual, expected),
    )
}

pub(super) fn check(
    sources: &SourceMap,
    parsed: &ParsedFile,
    program: &Program,
    unit: bool,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "ordinary CFG original module")?;
    factory_calls(module, sources, parsed)?;
    let loops = parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(_, n)| {
            if let Statement::For { source, body, .. } = n.payload() {
                Some((n.span(), *source, *body))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    // The independent no-For control still locks its actual factory Calls above.
    if loops.is_empty() {
        return Ok(());
    }
    let &(for_span, source_ast, body_ast) = one(loops.iter(), "one original scanner For")?;
    let scan_span = declaration(parsed, sources, "scan")?;
    let scan = function(module, scan_span)?;
    let g = Graph::with_abort(scan)?;
    let body_span = parsed.ast().statements().get(body_ast).unwrap().span();
    let ops = |predicate: fn(&Operation) -> bool| {
        one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| predicate(&i.operation)),
            "ordinary CFG unique original provider op",
        )
    };
    let first = ops(|o| matches!(o, Operation::MapIterationFirst { .. }))?;
    let has = ops(|o| matches!(o, Operation::MapIterationHasNext { .. }))?;
    let key = ops(|o| matches!(o, Operation::MapIterationKeyLoan { .. }))?;
    let val = ops(|o| matches!(o, Operation::MapIterationValueLoan { .. }))?;
    need(
        [first, has, key, val]
            .iter()
            .all(|i| i.origin == Origin::Source(for_span)),
        "ordinary CFG First/Has/K/V exact For identity",
        (first, has, key, val),
    )?;
    let Operation::MapIterationFirst { source } = first.operation else {
        unreachable!()
    };
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
        return Err(failure("ordinary actual HasNext branch", has));
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
        "ordinary CFG actual condition and ordered components",
        (has, key, val),
    )?;
    let entry = g.branch(first.block)?;
    need(
        entry.target == has.block
            && g.transported(entry, EntityId::Loan(source))? == EntityId::Loan(hs)
            && g.transported(entry, result(first)?)? == EntityId::Value(hc)
            && g.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc),
        "ordinary complete initial/header/body source and cursor columns",
        (entry, when_true),
    )?;
    g.incoming_from(key.block, &[has.block])?;
    g.incoming_from(when_false.target, &[has.block])?;
    let nexts = g
        .instructions
        .iter()
        .copied()
        .filter(|i| matches!(i.operation, Operation::MapIterationNext { .. }))
        .collect::<Vec<_>>();
    let mut expected_next = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, n)| {
            (inside(n.span(), body_span) && matches!(n.payload(), Expression::Continue { .. }))
                .then_some((n.span().start(), n.span().end()))
        })
        .collect::<BTreeSet<_>>();
    if falls_through(parsed, body_ast)? {
        expected_next.insert((for_span.start(), for_span.end()));
    }
    let actual_next = nexts
        .iter()
        .filter_map(|i| match i.origin {
            Origin::Source(s) => Some((s.start(), s.end())),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    need(
        nexts.len() == expected_next.len() && actual_next == expected_next,
        "every Continue and normal fallthrough has exactly its own Next",
        (&nexts, expected_next),
    )?;
    let mut predecessors = vec![first.block];
    for next in &nexts {
        let Operation::MapIterationNext {
            source: ns,
            cursor: nc,
        } = next.operation
        else {
            unreachable!()
        };
        g.anchored(EntityId::Loan(ns), &[EntityId::Loan(bs)])?;
        g.anchored(EntityId::Value(nc), &[EntityId::Value(bc)])?;
        let back = g.branch(next.block)?;
        need(
            back.target == has.block
                && g.argument(back, EntityId::Loan(hs))? == EntityId::Loan(ns)
                && g.argument(back, EntityId::Value(hc))? == result(next)?,
            "each actual Next feeds its exact source/cursor header columns",
            (next, back),
        )?;
        predecessors.push(next.block);
    }
    g.incoming_from(has.block, &predecessors)?;
    let lease = family(&g, &[EntityId::Loan(source)])?;
    let mut cursor_seeds = vec![result(first)?];
    for next in &nexts {
        cursor_seeds.push(result(next)?);
    }
    let cursors = family(&g, &cursor_seeds)?;
    let mut uses = Uses::default();
    for &entity in lease.iter().chain(cursors.iter()) {
        uses.protect(entity);
    }
    for i in [first, has, key, val]
        .into_iter()
        .chain(nexts.iter().copied())
    {
        for entity in i.operation.entities() {
            if lease.contains(&entity) || cursors.contains(&entity) {
                uses.permit(entity, i);
            }
        }
    }
    let birth = g.definition(EntityId::Loan(source))?;
    let source_node = parsed.ast().expressions().get(source_ast).unwrap();
    let mut caller = BTreeSet::new();
    let mut source_owner = None;
    let owners = match birth.operation {
        Operation::SharedReborrow { source: parent } => {
            let parameter = *one(
                g.block(scan.entry_block().unwrap())?
                    .parameters
                    .iter()
                    .filter(|e| matches!(scan.entity(**e).unwrap().ty, EntityType::Loan { .. })),
                "original Borrow parameter",
            )?;
            g.anchored(EntityId::Loan(parent), &[parameter])?;
            caller = family(&g, &[parameter])?;
            BTreeSet::new()
        }
        Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } => {
            let root = g.definition(EntityId::Place(place))?;
            let Operation::RootPlace { owner } = root.operation else {
                return Err(failure("provider actual owner root", root));
            };
            let seed = if matches!(source_node.payload(), Expression::Call { .. }) {
                let producer = one(
                    g.instructions.iter().copied().filter(|i| {
                        i.origin == Origin::Source(source_node.span())
                            && matches!(i.operation, Operation::DirectCall { .. })
                    }),
                    "temporary source actual one Call",
                )?;
                result(producer)?
            } else {
                *one(g.block(scan.entry_block().unwrap())?.parameters.iter().filter(|e|
                    matches!(scan.entity(**e).unwrap().ty, EntityType::Value(t) if matches!(module.type_kind(t), Some(SsaTypeKind::MapContainer { .. })))),
                    "original own Map parameter")?
            };
            g.anchored(EntityId::Value(owner), &[seed])?;
            source_owner = Some(EntityId::Value(owner));
            let family = family(&g, &[seed])?;
            for &entity in &family {
                uses.protect(entity);
            }
            uses.protect(EntityId::Place(place));
            uses.permit(EntityId::Value(owner), root);
            uses.permit(EntityId::Place(place), birth);
            family
        }
        _ => {
            return Err(failure(
                "provider source is actual original Borrow or Value",
                birth,
            ));
        }
    };
    for &entity in &caller {
        uses.protect(entity);
    }
    if let Operation::SharedReborrow { source: parent } = birth.operation {
        uses.permit(EntityId::Loan(parent), birth);
    }
    let map_type = scan
        .entity(EntityId::Loan(source))
        .unwrap()
        .ty
        .semantic_type();
    let (_, key_type, value_type) = module
        .map_container(map_type)
        .ok_or_else(|| failure("actual source Map storage", map_type))?;
    for (instruction, target) in [(key, key_type), (val, value_type)] {
        need(
            scan.entity(result(instruction)?).unwrap().ty
                == EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                },
            "actual independent represented K/V component types",
            instruction,
        )?;
    }
    for cursor in &cursors {
        need(
            module.type_kind(scan.entity(*cursor).unwrap().ty.semantic_type())
                == Some(&SsaTypeKind::Integer {
                    bits: 64,
                    signed: false,
                }),
            "actual full cursor family storage",
            cursor,
        )?;
    }
    resource_storage(module, parsed, sources, value_type, &g)?;
    let roots = [result(key)?, result(val)?];
    let mut component = BTreeMap::<EntityId, (usize, EntityId)>::new();
    let mut seeds = BTreeMap::<EntityId, (usize, EntityId)>::from([
        (roots[0], (0, roots[0])),
        (roots[1], (1, roots[1])),
    ]);
    loop {
        let before = seeds.len();
        component.clear();
        for (&seed, &role) in &seeds {
            for member in family(&g, &[seed])? {
                need(
                    component.insert(member, role).is_none(),
                    "distinct K/V and derived loan families",
                    member,
                )?;
            }
        }
        for i in &g.instructions {
            if let Operation::SharedReborrow { source } = i.operation {
                if let Some(&(index, _)) = component.get(&EntityId::Loan(source)) {
                    let current = result(i)?;
                    need(
                        scan.entity(current).unwrap().ty
                            == scan.entity(EntityId::Loan(source)).unwrap().ty,
                        "actual component reborrow Shared type",
                        i,
                    )?;
                    seeds.insert(current, (index, current));
                }
            }
        }
        if before == seeds.len() {
            break;
        }
    }
    for &entity in component.keys() {
        uses.protect(entity);
    }
    component_calls(
        &g, module, parsed, sources, body_span, &component, &mut uses,
    )?;
    let (guard, guard_type, guard_name) = guard(&g, module, parsed, sources, body_span)?;
    let guards = family(&g, &[result(guard)?])?;
    for &entity in &guards {
        uses.protect(entity);
    }
    let mut ends = Vec::new();
    let mut map_drops = BTreeSet::new();
    for i in &g.instructions {
        match i.operation {
            Operation::BorrowEnd { loan } if lease.contains(&EntityId::Loan(loan)) => {
                uses.permit(EntityId::Loan(loan), i);
                ends.push(i);
            }
            Operation::Drop { owner } if guards.contains(&EntityId::Value(owner)) => {
                need(
                    i.origin == Origin::Source(guard_name)
                        && scan
                            .entity(EntityId::Value(owner))
                            .unwrap()
                            .ty
                            .semantic_type()
                            == guard_type,
                    "original Guard current owner Drop",
                    i,
                )?;
                uses.permit(EntityId::Value(owner), i);
            }
            Operation::Drop { owner }
                if matches!(
                    module.type_kind(
                        scan.entity(EntityId::Value(owner))
                            .unwrap()
                            .ty
                            .semantic_type()
                    ),
                    Some(SsaTypeKind::MapContainer { .. })
                ) =>
            {
                need(
                    owners.contains(&EntityId::Value(owner)) && !cyclic(&g, i.block),
                    "source Drop uses exact all-incoming current owner outside any CFG cycle",
                    i,
                )?;
                uses.permit(EntityId::Value(owner), i);
                map_drops.insert(i.id);
            }
            _ => {}
        }
    }
    let transfers = parsed
        .ast()
        .expressions()
        .iter()
        .filter(|(_, n)| {
            inside(n.span(), body_span)
                && matches!(
                    n.payload(),
                    Expression::Break { .. } | Expression::Return { .. }
                )
        })
        .count();
    need(
        ends.len() == transfers + 1,
        "exact source lease End on every terminal and exhaustion",
        &ends,
    )?;
    need(!g.instructions.iter().any(|i| matches!(i.operation, Operation::BorrowEnd { loan } if caller.contains(&EntityId::Loan(loan)))),
        "callee never ends caller original loan", scan.id)?;
    let normal_source = g.transported(when_false, EntityId::Loan(hs))?;
    need(ends.iter().any(|i| i.block == when_false.target && matches!(i.operation, Operation::BorrowEnd { loan } if EntityId::Loan(loan) == normal_source)),
        "false HasNext edge ends its exact current lease", normal_source)?;
    for i in &g.instructions {
        if let Operation::BorrowEnd { loan } = i.operation {
            if component.contains_key(&EntityId::Loan(loan)) {
                uses.permit(EntityId::Loan(loan), i);
            }
        }
    }
    for &block in &g.blocks {
        if let TerminatorKind::Return { values } = &g.term(block)?.kind {
            let original = parsed
                .ast()
                .expressions()
                .iter()
                .filter_map(|(_, n)| matches!(n.payload(), Expression::Return { .. }).then_some(n))
                .find(|n| g.term(block).unwrap().origin == Origin::Source(n.span()));
            if let Some(n) = original {
                let Expression::Return { value, .. } = n.payload() else {
                    unreachable!()
                };
                match value {
                    None => need(
                        values.is_empty() && scan.return_types.is_empty(),
                        "actual bare Unit Return",
                        g.term(block)?,
                    )?,
                    Some(value) => {
                        let anchors = operand(&g, parsed, sources, *value)?;
                        let actual = one(values.iter().copied(), "actual single Return delivery")?;
                        g.anchored(EntityId::Value(actual), &anchors)?;
                        need(
                            scan.return_types.as_slice()
                                == [scan
                                    .entity(EntityId::Value(actual))
                                    .unwrap()
                                    .ty
                                    .semantic_type()],
                            "actual Return delivery type",
                            actual,
                        )?;
                    }
                }
            } else {
                need(
                    values.is_empty() && scan.return_types.is_empty(),
                    "only actual Unit fallthrough may omit Return AST",
                    g.term(block)?,
                )?;
            }
        }
    }
    let header_owner = source_owner
        .map(|owner| g.transported(entry, owner))
        .transpose()?;
    let body_owner = header_owner
        .map(|owner| g.transported(when_true, owner))
        .transpose()?;
    let exit_owner = header_owner
        .map(|owner| g.transported(when_false, owner))
        .transpose()?;
    let mut reached = BTreeSet::new();
    walk(
        &g,
        parsed,
        for_span,
        body_span,
        unit,
        has.block,
        key.block,
        &component,
        &lease,
        &guards,
        &map_drops,
        &owners,
        true,
        Path::at(body_owner, header_owner),
        &mut reached,
    )?;
    walk(
        &g,
        parsed,
        for_span,
        body_span,
        unit,
        has.block,
        when_false.target,
        &component,
        &lease,
        &guards,
        &map_drops,
        &owners,
        false,
        Path::at(exit_owner, header_owner),
        &mut reached,
    )?;
    need(
        g.blocks
            .iter()
            .all(|b| *b == first.block || *b == has.block || reached.contains(b)),
        "all scanner CFG terminators inspected",
        (&g.blocks, reached),
    )?;
    uses.check(&g, None)
}

fn component_calls(
    g: &Graph<'_>,
    module: &Module,
    parsed: &ParsedFile,
    sources: &SourceMap,
    body: Span,
    components: &BTreeMap<EntityId, (usize, EntityId)>,
    uses: &mut Uses,
) -> Result<(), Failure> {
    for i in &g.instructions {
        let involved = i
            .operation
            .entities()
            .into_iter()
            .filter(|e| components.contains_key(e))
            .collect::<Vec<_>>();
        if involved.is_empty() {
            continue;
        }
        match &i.operation {
            Operation::DirectCall {
                callee,
                receiver: None,
                arguments,
            } => {
                let Origin::Source(span) = i.origin else {
                    return Err(failure("component Call source", i));
                };
                let node = parsed
                    .ast()
                    .expressions()
                    .get(original_expression(parsed, span)?)
                    .unwrap();
                let Expression::Call {
                    callee: ast_callee,
                    arguments: ast_arguments,
                    ..
                } = node.payload()
                else {
                    return Err(failure("component actual Call AST", node));
                };
                let name = sources
                    .slice(parsed.ast().expressions().get(*ast_callee).unwrap().span())
                    .map_err(|e| failure("Call source name", e))?;
                let target = function(module, declaration(parsed, sources, name)?)?;
                need(
                    *callee == target.id && arguments.len() == ast_arguments.len(),
                    "actual selected component callee/arity",
                    i,
                )?;
                let parameters = &target
                    .block(target.entry_block().unwrap())
                    .unwrap()
                    .parameters;
                need(
                    parameters.len() == arguments.len(),
                    "actual reader parameter arity",
                    target,
                )?;
                for (index, (&argument, ast)) in arguments.iter().zip(ast_arguments).enumerate() {
                    let original = sources
                        .slice(parsed.ast().expressions().get(ast.value).unwrap().span())
                        .unwrap();
                    let expected = match original {
                        "key" => 0,
                        "value" => 1,
                        _ => return Err(failure("original component argument", original)),
                    };
                    need(
                        components
                            .get(&argument)
                            .is_some_and(|(role, _)| *role == expected)
                            && g.function.entity(argument).unwrap().ty
                                == target.entity(parameters[index]).unwrap().ty
                            && matches!(
                                target.entity(parameters[index]).unwrap().ty,
                                EntityType::Loan {
                                    kind: LoanKind::Shared,
                                    ..
                                }
                            ),
                        "actual original ordered K/V lineage and Shared ABI",
                        (i, index),
                    )?;
                    uses.permit(argument, i);
                }
            }
            Operation::SharedReborrow { source } => {
                let current = result(i)?;
                need(
                    components.get(&current).map(|x| x.0)
                        == components.get(&EntityId::Loan(*source)).map(|x| x.0),
                    "same component derived Borrow",
                    i,
                )?;
                uses.permit(EntityId::Loan(*source), i);
            }
            Operation::BorrowEnd { .. } => {}
            Operation::StringEqual { .. }
            | Operation::Read { .. }
            | Operation::PrintString { .. } => {
                let Origin::Source(span) = i.origin else {
                    return Err(failure("component primitive real AST", i));
                };
                need(
                    inside(span, body),
                    "component primitive belongs to original body",
                    i,
                )?;
                let original = parsed
                    .ast()
                    .expressions()
                    .get(original_expression(parsed, span)?)
                    .unwrap();
                let text = sources.slice(original.span()).unwrap();
                need(
                    involved.iter().all(|e| {
                        let name = if components[e].0 == 0 { "key" } else { "value" };
                        text.contains(name)
                    }),
                    "component primitive names its actual binding",
                    (i, text),
                )?;
                for entity in involved {
                    uses.permit(entity, i);
                }
            }
            _ => return Err(failure("unproved component operand use", i)),
        }
    }
    Ok(())
}

fn guard<'a>(
    g: &Graph<'a>,
    module: &Module,
    parsed: &ParsedFile,
    sources: &SourceMap,
    body: Span,
) -> Result<(&'a Instruction, SsaTypeId, Span), Failure> {
    let (name, initializer) = one(
        parsed
            .ast()
            .items()
            .iter()
            .filter_map(|(_, n)| match n.payload() {
                Item::Variable {
                    name: NameMarker::Present(name),
                    initializer,
                    ..
                } if inside(n.span(), body) && sources.slice(*name) == Ok("guard") => {
                    Some((*name, *initializer))
                }
                _ => None,
            }),
        "original body Guard declaration",
    )?;
    let constructor = parsed.ast().expressions().get(initializer).unwrap();
    need(
        sources.slice(constructor.span()) == Ok("Guard()"),
        "original Guard constructor AST",
        constructor,
    )?;
    let allocation = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(constructor.span())
                && matches!(i.operation, Operation::HeapAllocate { .. })
        }),
        "one original Guard allocation",
    )?;
    let Operation::HeapAllocate { owner, payload } = allocation.operation else {
        unreachable!()
    };
    let payload = g.definition(EntityId::Value(payload))?;
    need(
        payload.origin == allocation.origin
            && matches!(&payload.operation, Operation::AggregateConstruct { fields, .. } if fields.is_empty()),
        "Guard actual empty payload",
        payload,
    )?;
    let class = one(parsed.roots().iter().filter_map(|id| {
        let n = parsed.ast().items().get(*id).ok()?;
        match n.payload() { Item::Classifier(c) if matches!(c.name, NameMarker::Present(name) if sources.slice(name)==Ok("Guard")) => Some(c), _ => None }
    }), "original Guard class")?;
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
            .get(&owner)
            .and_then(|id| module.function(*id))
            .is_some_and(|f| f.origin == Origin::Source(deinit)),
        "Guard actual allocation type selects original deinit",
        owner,
    )?;
    Ok((allocation, owner, name))
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

fn resource_storage(
    module: &Module,
    parsed: &ParsedFile,
    sources: &SourceMap,
    ty: SsaTypeId,
    g: &Graph<'_>,
) -> Result<(), Failure> {
    let classes = parsed.roots().iter().filter_map(|id| {
        let n = parsed.ast().items().get(*id).ok()?;
        match n.payload() {
            Item::Classifier(c) if matches!(c.name, NameMarker::Present(name) if sources.slice(name)==Ok("Resource")) => Some(c),
            _ => None,
        }
    }).collect::<Vec<_>>();
    if classes.is_empty() {
        return Ok(());
    }
    let class = one(classes.iter().copied(), "original Resource class")?;
    let Some(SsaTypeKind::HeapOwner {
        payload: Some(payload),
        ..
    }) = module.type_kind(ty)
    else {
        return Err(failure("actual Resource value owner storage", ty));
    };
    need(
        matches!(module.type_kind(*payload), Some(SsaTypeKind::Aggregate { fields, .. })
        if fields.len()==1 && module.type_kind(fields[0])==Some(&SsaTypeKind::StringOwner)),
        "Resource actual payload retains its owned String field",
        payload,
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
            .get(&ty)
            .and_then(|id| module.function(*id))
            .is_some_and(|f| f.origin == Origin::Source(deinit)),
        "Map component storage selects the original Resource deinit",
        ty,
    )?;
    need(
        !g.instructions.iter().any(|i| {
            matches!(i.operation, Operation::Drop { owner }
        if g.function.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()==ty)
        }),
        "borrowed Resource component is never materialized as a dropped owner",
        g.function.id,
    )
}
