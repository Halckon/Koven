//! A moved alias keeps its producer when the old variable receives a different owner.
use crate::ssa::{
    map_provider_exits_pipeline_tests::{
        fixture::{Failure, failure, need},
        query_graph::{Graph, Uses, function as source_function, result, value},
        query_inputs::one,
    },
    model::*,
};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::{UnitReferenceTarget, UnitSymbolId, ValidatedCompilationUnitNames},
    ownership_checking::{
        CompilationUnitOwnership, LoanKind as FrontendLoanKind, UnitDropPoint, UnitDropTarget,
        UnitIterationCleanupAction as Action, UnitIterationExitKind, UnitIterationSourceAccess,
        UnitLoanTarget,
    },
    parser::{
        AssignmentOperator, Expression, Item, NameMarker, ParsedFile, Statement, VariableKind,
    },
    source::{SourceMap, Span},
    type_checking::{
        BuiltinType, CompilationUnitTypes, ParameterMode, UnitCallTarget, UnitExpressionId,
        UnitStatementId, UnitTypeKind,
    },
};
use std::collections::BTreeSet;

struct Site {
    scan: Span,
    old: Span,
    alias: Span,
    moved: ExpressionId,
    assignment: ExpressionId,
    target: ExpressionId,
    make: ExpressionId,
    other: ExpressionId,
    statement: StatementId,
    for_span: Span,
    source: ExpressionId,
    read: ExpressionId,
    read_map: ExpressionId,
    argument: ExpressionId,
}

fn span(parsed: &ParsedFile, id: ExpressionId) -> Span {
    parsed.ast().expressions().get(id).unwrap().span()
}

fn inside(inner: Span, outer: Span) -> bool {
    inner.source_id() == outer.source_id()
        && inner.start() > outer.start()
        && inner.end() < outer.end()
}

fn declaration(
    sources: &SourceMap,
    parsed: &ParsedFile,
    wanted: &str,
) -> Result<(Span, Span), Failure> {
    one(
        parsed.roots().iter().filter_map(|id| {
            let node = parsed.ast().items().get(*id).ok()?;
            matches!(node.payload(), Item::Function {name:NameMarker::Present(name), ..}
            if sources.slice(*name) == Ok(wanted))
            .then(|| {
                let Item::Function {
                    name: NameMarker::Present(name),
                    ..
                } = node.payload()
                else {
                    unreachable!()
                };
                (node.span(), *name)
            })
        }),
        "rebound alias original function",
    )
}

fn call(
    sources: &SourceMap,
    parsed: &ParsedFile,
    scan: Span,
    wanted: &str,
) -> Result<ExpressionId, Failure> {
    one(
        parsed.ast().expressions().iter().filter_map(|(id, node)| {
            let Expression::Call { callee, .. } = node.payload() else {
                return None;
            };
            (inside(node.span(), scan) && sources.slice(span(parsed, *callee)) == Ok(wanted))
                .then_some(id)
        }),
        "rebound alias unique actual Call",
    )
}

fn site(sources: &SourceMap, parsed: &ParsedFile) -> Result<Site, Failure> {
    let scan = declaration(sources, parsed, "scan")?.0;
    let local = |wanted, kind| {
        one(
            parsed.ast().items().iter().filter_map(|(_, node)| {
                let Item::Variable {
                    name: NameMarker::Present(name),
                    initializer,
                    kind: actual,
                    ..
                } = node.payload()
                else {
                    return None;
                };
                (inside(node.span(), scan) && sources.slice(*name) == Ok(wanted) && *actual == kind)
                    .then_some((*name, *initializer))
            }),
            "rebound alias original local declaration",
        )
    };
    let (old, make) = local("old", VariableKind::Var)?;
    let (alias, moved) = local("source", VariableKind::Val)?;
    let (assignment, target, other) = one(
        parsed.ast().expressions().iter().filter_map(|(id, node)| {
            let Expression::Assignment {
                target,
                value,
                operator: AssignmentOperator::Assign,
                ..
            } = node.payload()
            else {
                return None;
            };
            inside(node.span(), scan).then_some((id, *target, *value))
        }),
        "rebound alias exact assignment",
    )?;
    let (statement, for_span, source) = one(
        parsed.ast().statements().iter().filter_map(|(id, node)| {
            let Statement::For { source, .. } = node.payload() else {
                return None;
            };
            inside(node.span(), scan).then_some((id, node.span(), *source))
        }),
        "rebound alias actual For",
    )?;
    let read = call(sources, parsed, scan, "read")?;
    let read_map = call(sources, parsed, scan, "readMap")?;
    let Expression::Call { arguments, .. } =
        parsed.ast().expressions().get(read_map).unwrap().payload()
    else {
        unreachable!()
    };
    let [argument] = arguments.as_slice() else {
        return Err(failure(
            "rebound alias original readMap argument",
            arguments,
        ));
    };
    need(
        make == call(sources, parsed, scan, "make")?
            && other == call(sources, parsed, scan, "other")?
            && sources.slice(span(parsed, moved)) == Ok("old")
            && sources.slice(span(parsed, target)) == Ok("old")
            && sources.slice(span(parsed, source)) == Ok("source")
            && sources.slice(span(parsed, argument.value)) == Ok("old")
            && span(parsed, make).end() < span(parsed, moved).start()
            && span(parsed, moved).end() < span(parsed, assignment).start()
            && span(parsed, assignment).end() < for_span.start()
            && for_span.end() < span(parsed, read_map).start(),
        "original Move precedes old rebind, alias For and distinct later Borrow",
        scan,
    )?;
    Ok(Site {
        scan,
        old,
        alias,
        moved,
        assignment,
        target,
        make,
        other,
        statement,
        for_span,
        source,
        read,
        read_map,
        argument: argument.value,
    })
}

fn reference(
    names: &ValidatedCompilationUnitNames,
    parsed: &ParsedFile,
    id: ExpressionId,
) -> Result<UnitSymbolId, Failure> {
    one(
        names
            .names()
            .references()
            .iter()
            .filter(|r| r.span() == span(parsed, id))
            .filter_map(|r| {
                if let UnitReferenceTarget::Symbol(symbol) = r.target() {
                    Some(*symbol)
                } else {
                    None
                }
            }),
        "rebound alias original qualified Name",
    )
}

pub(super) fn unit_facts(
    sources: &SourceMap,
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), Failure> {
    let site = site(sources, parsed)?;
    let old = reference(names, parsed, site.moved)?;
    let alias = reference(names, parsed, site.source)?;
    let unit = old.source_unit();
    let expression = |id| UnitExpressionId::new(unit, id);
    need(
        old != alias
            && alias.source_unit() == unit
            && reference(names, parsed, site.target)? == old
            && reference(names, parsed, site.argument)? == old
            && [old, alias]
                .iter()
                .all(|root| owned.formed_owned_locals().contains(root)),
        "distinct formed roots preserve Move and actual post-rebind Borrow identity",
        (old, alias),
    )?;
    for (root, marker) in [(old, site.old), (alias, site.alias)] {
        need(
            names.names().source_units().iter().any(|u| {
                u.source_unit() == unit
                    && u.resolution()
                        .symbols()
                        .get(root.symbol().index())
                        .is_some_and(|s| s.span() == marker)
            }),
            "actual original root declaration markers",
            (root, marker),
        )?;
    }
    for (id, name) in [
        (site.make, "make"),
        (site.other, "other"),
        (site.read_map, "readMap"),
    ] {
        let call = typed
            .call(expression(id))
            .ok_or_else(|| failure("rebound alias original typed Call", id))?;
        let UnitCallTarget::Declaration(target) = call.target() else {
            return Err(failure("rebound alias source callable target", call));
        };
        let signature = typed
            .signatures()
            .declaration(target)
            .and_then(|d| d.callable())
            .ok_or_else(|| failure("rebound alias actual callee signature", target))?;
        need(
            signature.name_span() == declaration(sources, parsed, name)?.1
                && call.receiver().is_none()
                && call.arguments().len() == usize::from(name == "readMap"),
            "actual selected source callable and arity",
            call,
        )?;
    }
    let statement = UnitStatementId::new(unit, site.statement);
    let plan = owned
        .map_iteration(statement)
        .ok_or_else(|| failure("rebound alias original For plan", statement))?;
    need(
        typed.map_iterations().len() == 1
            && owned.map_iterations().len() == 1
            && typed.map_iteration(statement) == Some(plan.descriptor())
            && plan.descriptor().source() == expression(site.source)
            && plan.source_access() == UnitIterationSourceAccess::Owned
            && matches!(plan.source(), Some(UnitLoanTarget::Place(place)) if place.is_root() && place.root() == alias)
            && typed.expression_type(expression(site.make))
                == Some(plan.descriptor().source_type())
            && typed.expression_type(expression(site.other))
                == Some(plan.descriptor().source_type())
            && typed.is_resource_type(plan.descriptor().source_type()) == Some(false),
        "For owns moved make result through source, never later other root",
        plan,
    )?;
    let drops = owned.drops().iter().filter(|drop| matches!(drop.target(), UnitDropTarget::Named(root) if [old, alias].contains(&root))).collect::<Vec<_>>();
    need(
        drops.len() == 2,
        "exactly two original root obligations without old moved-owner duplicate",
        &drops,
    )?;
    let alias_drop = one(
        drops
            .iter()
            .copied()
            .filter(|d| d.target() == UnitDropTarget::Named(alias)),
        "actual alias obligation",
    )?;
    let old_drop = one(
        drops
            .iter()
            .copied()
            .filter(|d| d.target() == UnitDropTarget::Named(old)),
        "actual rebound obligation",
    )?;
    need(
        alias_drop.point() == UnitDropPoint::LoopExit(statement)
            && alias_drop.value_origin() == site.alias
            && old_drop.point() == UnitDropPoint::CallReturn(expression(site.read_map))
            && old_drop.value_origin() == span(parsed, site.assignment),
        "precise alias LoopExit and rebound Borrow-return Drop points/origins",
        (alias_drop, old_drop),
    )?;
    let exit = one(
        plan.exits()
            .iter()
            .filter(|e| e.kind() == UnitIterationExitKind::Exhaustion),
        "original exhausted alias cleanup",
    )?;
    need(
        exit.actions()
            == [
                Action::FinishProvider(statement),
                Action::EndSource(statement),
                Action::Drop(*alias_drop),
            ],
        "alias provider End precedes its own immediate Drop",
        exit,
    )?;
    let call = typed.call(expression(site.read_map)).unwrap();
    let loan = one(
        owned
            .loans()
            .iter()
            .filter(|loan| loan.call() == expression(site.read_map)),
        "actual post-loop readMap loan",
    )?;
    need(
        matches!(call.arguments(), [argument] if argument.mode() == ParameterMode::Borrow
        && argument.argument_index() == 0 && argument.parameter_index() == 0
        && argument.parameter_type() == plan.descriptor().source_type())
            && loan.argument() == expression(site.argument)
            && loan.kind() == FrontendLoanKind::Shared
            && matches!(loan.target(), UnitLoanTarget::Place(place) if place.is_root() && place.root() == old)
            && !owned
                .value_deliveries()
                .iter()
                .any(|delivery| delivery.call() == expression(site.read_map)),
        "readMap borrows new old root, never moves source alias",
        loan,
    )?;
    let scanner = one(
        typed
            .signatures()
            .declarations()
            .iter()
            .filter_map(|d| d.callable())
            .filter(|c| c.name_span() == declaration(sources, parsed, "scan").unwrap().1),
        "actual scanner signature",
    )?;
    need(
        scanner.parameters().is_empty()
            && typed.types().get(scanner.return_type())
                == Some(&UnitTypeKind::Builtin(BuiltinType::Unit)),
        "actual alias scanner is Unit and has no owner parameter",
        scanner,
    )
}

fn transport(g: &Graph<'_>, entity: EntityId, anchor: EntityId) -> Result<(), Failure> {
    let mut pending = vec![entity];
    let mut seen = BTreeSet::new();
    let mut dependencies = Vec::new();
    while let Some(entity) = pending.pop() {
        if entity == anchor {
            continue;
        }
        if !seen.insert(entity) {
            continue;
        }
        let data = g
            .function
            .entity(entity)
            .ok_or_else(|| failure("alias transported entity", entity))?;
        let Definition::BlockParameter { block, .. } = data.definition else {
            return Err(failure(
                "alias full incoming reached different epoch",
                (entity, anchor, data),
            ));
        };
        let incoming = g.incoming(block);
        need(
            !incoming.is_empty(),
            "alias complete incoming exists",
            entity,
        )?;
        for (_, edge) in incoming {
            let argument = g.argument(edge, entity)?;
            dependencies.push((entity, argument));
            pending.push(argument);
        }
    }
    let mut anchored = BTreeSet::from([anchor]);
    loop {
        let before = anchored.len();
        for &(entity, argument) in &dependencies {
            if anchored.contains(&argument) {
                anchored.insert(entity);
            }
        }
        if before == anchored.len() {
            break;
        }
    }
    need(
        seen.is_subset(&anchored),
        "alias every visited incoming can reach actual factory",
        (entity, anchor, seen, anchored),
    )
}

fn family(g: &Graph<'_>, anchor: EntityId) -> Result<BTreeSet<EntityId>, Failure> {
    let mut entities = BTreeSet::from([anchor]);
    loop {
        let before = entities.len();
        for (_, edge) in &g.edges {
            for (&argument, &parameter) in
                edge.arguments.iter().zip(&g.block(edge.target)?.parameters)
            {
                if entities.contains(&argument) {
                    entities.insert(parameter);
                }
            }
        }
        if before == entities.len() {
            break;
        }
    }
    for &entity in &entities {
        transport(g, entity, anchor)?;
    }
    Ok(entities)
}

pub(super) fn graph(
    sources: &SourceMap,
    parsed: &ParsedFile,
    program: &Program,
) -> Result<(), Failure> {
    let site = site(sources, parsed)?;
    let module = one(program.modules.iter(), "alias original module")?;
    let scanner = source_function(module, site.scan)?;
    let g = Graph::new(scanner)?;
    need(
        g.block(scanner.entry_block().unwrap())?
            .parameters
            .is_empty(),
        "alias source scanner has no owner parameters",
        scanner.id,
    )?;
    let source_call = |id, name| -> Result<&Instruction, Failure> {
        let callee = source_function(module, declaration(sources, parsed, name)?.0)?;
        one(g.instructions.iter().copied().filter(|i| i.origin == Origin::Source(span(parsed, id))
            && matches!(&i.operation, Operation::DirectCall {callee:c, receiver:None, ..} if *c == callee.id)), "alias actual original Call/callee")
    };
    let make = source_call(site.make, "make")?;
    let other = source_call(site.other, "other")?;
    for producer in [make, other] {
        need(
            matches!(&producer.operation, Operation::DirectCall {arguments, ..} if arguments.is_empty())
                && matches!(result(producer)?, EntityId::Value(_)),
            "alias factory actual zeroarg owned Map result",
            producer,
        )?;
    }
    let old = family(&g, result(make)?)?;
    let new = family(&g, result(other)?)?;
    need(
        old.is_disjoint(&new),
        "moved and rebound owner columns never overlap",
        (&old, &new),
    )?;
    let mut uses = Uses::default();
    for &entity in old.iter().chain(&new) {
        uses.protect(entity);
    }
    let op = |predicate: fn(&Operation) -> bool| {
        one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.origin == Origin::Source(site.for_span) && predicate(&i.operation)),
            "alias actual For operation",
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
    let source_begin = g.definition(EntityId::Loan(source))?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = source_begin.operation
    else {
        return Err(failure("alias source actual shared lease", source_begin));
    };
    let root = g.definition(EntityId::Place(place))?;
    let Operation::RootPlace { owner } = root.operation else {
        return Err(failure("alias actual source root", root));
    };
    transport(&g, EntityId::Value(owner), result(make)?)?;
    uses.permit(EntityId::Value(owner), root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), source_begin);
    let entry = g.branch(first.block)?;
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
        return Err(failure("alias actual provider conditional", has));
    };
    need(
        entry.target == has.block
            && *condition == value(has)?
            && when_true.target == key.block
            && g.transported(entry, EntityId::Loan(source))? == EntityId::Loan(hs)
            && g.transported(entry, result(first)?)? == EntityId::Value(hc)
            && g.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc)
            && val.block == key.block
            && val.operation
                == Operation::MapIterationValueLoan {
                    source: bs,
                    cursor: bc,
                },
        "alias full actual provider entry/header/body columns",
        (entry, when_true),
    )?;
    let ho = g.transported(entry, EntityId::Value(owner))?;
    let bo = g.transported(when_true, ho)?;
    let xo = g.transported(when_false, ho)?;
    let back = g.branch(key.block)?;
    need(
        back.target == has.block
            && next.block == key.block
            && next.operation
                == Operation::MapIterationNext {
                    source: bs,
                    cursor: bc,
                }
            && g.argument(back, ho)? == bo
            && g.argument(back, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.argument(back, EntityId::Value(hc))? == result(next)?,
        "alias full actual normal backedge",
        back,
    )?;
    g.incoming_from(has.block, &[first.block, key.block])?;
    g.incoming_from(key.block, &[has.block])?;
    g.incoming_from(when_false.target, &[has.block])?;
    let leases = family(&g, EntityId::Loan(source))?;
    for &lease in &leases {
        uses.protect(lease);
    }
    for (loan, instruction) in [(source, first), (hs, has), (bs, key), (bs, val), (bs, next)] {
        uses.permit(EntityId::Loan(loan), instruction);
    }
    for entity in [
        result(first)?,
        EntityId::Value(hc),
        EntityId::Value(bc),
        g.transported(when_false, EntityId::Value(hc))?,
        result(next)?,
    ] {
        uses.protect(entity);
    }
    uses.permit(EntityId::Value(hc), has);
    for instruction in [key, val, next] {
        uses.permit(EntityId::Value(bc), instruction);
    }
    let source_end = one(
        g.instructions.iter().copied().filter(|i| {
            matches!(i.operation, Operation::BorrowEnd {loan}
        if leases.contains(&EntityId::Loan(loan)))
        }),
        "alias exact EndSource",
    )?;
    need(
        source_end.block == when_false.target
            && matches!(source_end.operation, Operation::BorrowEnd {loan} if EntityId::Loan(loan) == g.transported(when_false, EntityId::Loan(hs))?),
        "actual false column source End",
        source_end,
    )?;
    if let Operation::BorrowEnd { loan } = source_end.operation {
        uses.permit(EntityId::Loan(loan), source_end);
    }
    let read = source_call(site.read, "read")?;
    need(
        read.block == key.block
            && matches!(&read.operation, Operation::DirectCall {arguments, ..}
        if arguments.as_slice() == [result(key)?, result(val)?]),
        "alias actual reader consumes K/V loans",
        read,
    )?;
    let reader = source_function(module, declaration(sources, parsed, "read")?.0)?;
    let parameters = &reader
        .block(reader.entry_block().unwrap())
        .unwrap()
        .parameters;
    need(
        parameters.len() == 2
            && [key, val]
                .iter()
                .zip(parameters)
                .all(|(component, parameter)| {
                    let ty = scanner.entity(result(component).unwrap()).unwrap().ty;
                    matches!(ty, EntityType::Loan {kind:LoanKind::Shared, target}
            if module.type_kind(target) == Some(&SsaTypeKind::StringOwner))
                        && reader.entity(*parameter).unwrap().ty == ty
                }),
        "actual K/V Shared String ABI reaches original reader",
        reader.id,
    )?;
    let mut previous = read;
    for component in [val, key] {
        let entity = result(component)?;
        let end = one(g.instructions.iter().copied().filter(|i| matches!(i.operation, Operation::BorrowEnd {loan} if EntityId::Loan(loan) == entity)), "alias actual unique component End")?;
        need(
            end.block == key.block && g.position(previous)? < g.position(end)?,
            "alias read then VEnd then KEnd",
            end,
        )?;
        uses.protect(entity);
        uses.permit(entity, read);
        uses.permit(entity, end);
        previous = end;
    }
    need(
        g.position(previous)? < g.position(next)?,
        "alias components end before Next",
        next,
    )?;
    let read_map = source_call(site.read_map, "readMap")?;
    let Operation::DirectCall { ref arguments, .. } = read_map.operation else {
        unreachable!()
    };
    let [EntityId::Loan(borrow)] = arguments.as_slice() else {
        return Err(failure("alias readMap real shared Map ABI", read_map));
    };
    let begin = g.definition(EntityId::Loan(*borrow))?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = begin.operation
    else {
        return Err(failure("alias post-loop actual shared BorrowBegin", begin));
    };
    let post_root = g.definition(EntityId::Place(place))?;
    let Operation::RootPlace { owner: new_owner } = post_root.operation else {
        return Err(failure("alias post-loop new epoch root", post_root));
    };
    transport(&g, EntityId::Value(new_owner), result(other)?)?;
    uses.permit(EntityId::Value(new_owner), post_root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), begin);
    uses.protect(EntityId::Loan(*borrow));
    uses.permit(EntityId::Loan(*borrow), read_map);
    let end = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::BorrowEnd { loan: *borrow }),
        "alias actual post-call Borrow End",
    )?;
    uses.permit(EntityId::Loan(*borrow), end);
    let map_type = scanner.entity(result(make)?).unwrap().ty.semantic_type();
    need(
        module
            .map_container(map_type)
            .is_some_and(|(kind, key, val)| {
                kind == MapContainerKind::Map
                    && module.type_kind(key) == Some(&SsaTypeKind::StringOwner)
                    && module.type_kind(val) == Some(&SsaTypeKind::StringOwner)
            })
            && scanner.entity(result(other)?).unwrap().ty.semantic_type() == map_type,
        "alias two independent results share actual Map representation",
        map_type,
    )?;
    let callee = source_function(module, declaration(sources, parsed, "readMap")?.0)?;
    let parameters = &callee
        .block(callee.entry_block().unwrap())
        .unwrap()
        .parameters;
    let ty = EntityType::Loan {
        kind: LoanKind::Shared,
        target: map_type,
    };
    need(
        scanner.entity(EntityId::Loan(*borrow)).unwrap().ty == ty
            && matches!(parameters.as_slice(), [parameter] if callee.entity(*parameter).unwrap().ty == ty),
        "actual readMap parameter is Shared Borrow of the new Map owner",
        callee.id,
    )?;
    let drops = g
        .instructions
        .iter()
        .copied()
        .filter(|i| {
            matches!(i.operation, Operation::Drop {owner}
        if scanner.entity(EntityId::Value(owner)).unwrap().ty.semantic_type() == map_type)
        })
        .collect::<Vec<_>>();
    need(
        drops.len() == 2,
        "alias exactly two Map Drop instructions",
        &drops,
    )?;
    let old_drop = one(
        drops.iter().copied().filter(
            |i| matches!(i.operation, Operation::Drop {owner} if EntityId::Value(owner) == xo),
        ),
        "alias Drop consumes exact exhausted current owner column",
    )?;
    let new_drop = one(
        drops
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::Drop { owner: new_owner }),
        "rebound Drop consumes exact post-Borrow current owner",
    )?;
    need(
        old_drop.id != new_drop.id
            && old_drop.origin == Origin::Source(site.alias)
            && new_drop.origin == Origin::Source(span(parsed, site.assignment))
            && source_end.block == old_drop.block
            && g.position(old_drop)? == g.position(source_end)? + 1
            && read_map.block == old_drop.block
            && g.position(old_drop)? < g.position(post_root)?
            && post_root.block == read_map.block
            && begin.block == read_map.block
            && end.block == read_map.block
            && new_drop.block == read_map.block
            && g.position(post_root)? < g.position(begin)?
            && g.position(begin)? < g.position(read_map)?
            && g.position(read_map)? < g.position(end)?
            && g.position(new_drop)? == g.position(end)? + 1,
        "disjoint owner Drop origins and actual ASAP source/Borrow cleanup order",
        (old_drop, new_drop),
    )?;
    for drop in [old_drop, new_drop] {
        let Operation::Drop { owner } = drop.operation else {
            unreachable!()
        };
        transport(
            &g,
            EntityId::Value(owner),
            if drop.id == old_drop.id {
                result(make)?
            } else {
                result(other)?
            },
        )?;
        uses.permit(EntityId::Value(owner), drop);
    }
    need(
        matches!(
            g.term(when_false.target)?.kind,
            TerminatorKind::Return { .. }
        ) && g
            .blocks
            .iter()
            .filter(|&&block| matches!(g.term(block).unwrap().kind, TerminatorKind::Return { .. }))
            .count()
            == 1,
        "every normal Return follows both distinct acyclic Map Drops",
        scanner.id,
    )?;
    uses.check(&g, None)
}
