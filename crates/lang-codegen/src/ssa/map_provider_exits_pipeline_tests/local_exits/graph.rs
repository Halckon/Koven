//! Actual local producer identities and all normal exit cleanup, without fabricated Move ops.
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

pub(super) fn check(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    program: &Program,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "local exits one original module")?;
    for (scanner, factory) in [("scan", "make"), ("scanEmpty", "makeEmpty")] {
        let site = ast(case, sources, parsed, scanner, factory)?;
        let f = one(
            module
                .functions
                .iter()
                .filter(|f| f.origin == Origin::Source(site.scan)),
            "actual independent original scanner",
        )?;
        let g = Graph::new(f)?;
        need(
            g.block(f.entry_block().unwrap())?.parameters.is_empty(),
            "local scanner has no invented entry owner parameter",
            f.id,
        )?;
        let factory = one(
            module
                .functions
                .iter()
                .filter(|f| f.origin == Origin::Source(site.factory)),
            "actual selected original factory",
        )?;
        let producer = one(g.instructions.iter().copied().filter(|i| i.origin == Origin::Source(site.call)
            && matches!(&i.operation, Operation::DirectCall { callee, receiver: None, arguments } if *callee == factory.id && arguments.is_empty())), "actual zero-argument source formation call")?;
        let anchor = result(producer)?;
        let EntityId::Value(_) = anchor else {
            return Err(failure("actual factory delivers owned Map value", producer));
        };
        let map_type = f.entity(anchor).unwrap().ty.semantic_type();
        let (kind, key, val) = module
            .map_container(map_type)
            .ok_or_else(|| failure("actual local Map storage", map_type))?;
        need(
            factory
                .block(factory.entry_block().unwrap())
                .unwrap()
                .parameters
                .is_empty()
                && factory.return_types.as_slice() == [map_type]
                && kind
                    == if case.mutable {
                        MapContainerKind::MutableMap
                    } else {
                        MapContainerKind::Map
                    }
                && module.type_kind(key) == Some(&SsaTypeKind::StringOwner)
                && module.type_kind(val) == Some(&SsaTypeKind::StringOwner),
            "actual selected factory Value result/Map storage",
            factory,
        )?;
        let owners = family(&g, anchor)?;
        let mut uses = Uses::default();
        for &owner in &owners {
            uses.protect(owner);
        }
        let formed = marker(&g, sources, parsed, site.scan, "formed")?;
        let first = one(
            g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(site.for_span)
                    && matches!(i.operation, Operation::MapIterationFirst { .. })
            }),
            "actual original First",
        )?;
        need(
            producer.block == f.entry_block().unwrap()
                && formed.block == producer.block
                && first.block == producer.block
                && g.position(producer)? < g.position(formed)?
                && g.position(formed)? < g.position(first)?,
            "actual producer once before formed marker and provider",
            (producer, formed, first),
        )?;
        let evidence = provider::check(
            &g,
            module,
            sources,
            parsed,
            (site.for_span, site.body, case.exit),
            &owners,
            &mut uses,
        )?;
        let drops = g.instructions.iter().copied().filter(|i| matches!(i.operation,Operation::Drop {owner} if f.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()==map_type)).collect::<Vec<_>>();
        need(
            drops.len() == if case.exit >= 2 { 2 } else { 1 },
            "exact local immediate terminal Drop sites",
            &drops,
        )?;
        for drop in &drops {
            let Operation::Drop { owner } = drop.operation else {
                unreachable!()
            };
            transport(&g, EntityId::Value(owner), anchor)?;
            need(
                evidence.terminal_owners.get(&drop.block) == Some(&EntityId::Value(owner)),
                "Drop consumes exact terminal block current owner column",
                drop,
            )?;
            need(
                drop.origin == Origin::Source(site.binding) && !cycle(&g, drop.block),
                "actual current local binding Drop outside CFG cycles",
                drop,
            )?;
            let ended = one(
                evidence
                    .ends
                    .iter()
                    .copied()
                    .filter(|end| end.block == drop.block),
                "each local Drop belongs to actual provider EndSource",
            )?;
            need(
                g.position(drop)? == g.position(ended)? + 1,
                "local Pure source ASAP Drop immediately after actual EndSource",
                (ended, drop),
            )?;
            uses.permit(EntityId::Value(owner), drop);
        }
        let after = marker(&g, sources, parsed, site.scan, "after")?;
        let returned = returned(
            &g,
            module,
            sources,
            parsed,
            &site,
            &evidence,
            (case.exit, &mut uses),
        )?;
        uses.check(&g, returned)?;
        paths(
            &g,
            producer.id,
            formed.id,
            after.id,
            &evidence.ends,
            &drops,
            case.exit,
        )?;
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
fn ast(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    scanner: &str,
    producer: &str,
) -> Result<Site, Failure> {
    let declaration = |wanted: &str| {
        one(parsed.roots().iter().filter_map(|id| {
        let n = parsed.ast().items().get(*id).ok()?;
        matches!(n.payload(),Item::Function {name:NameMarker::Present(name),..} if sources.slice(*name)==Ok(wanted)).then_some(n)
    }), "original actual source declaration")
    };
    let scan = declaration(scanner)?;
    let Item::Function { parameters, .. } = scan.payload() else {
        unreachable!()
    };
    need(
        parameters.is_empty(),
        "original scanner is truly zero-argument",
        scan,
    )?;
    let (for_span, source, body) = one(
        parsed.ast().statements().iter().filter_map(|(_, n)| {
            let Statement::For { source, body, .. } = n.payload() else {
                return None;
            };
            inside(n.span(), scan.span()).then_some((n.span(), *source, *body))
        }),
        "original local For source",
    )?;
    let source = parsed.ast().expressions().get(source).unwrap();
    need(
        source.payload() == &Expression::Name && sources.slice(source.span()) == Ok("source"),
        "actual named local iteration source",
        source,
    )?;
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
                    && n.span().end() < for_span.start()
                    && sources.slice(*name) == Ok(wanted))
                .then_some((*name, *initializer, *kind))
            }),
            "original pre-provider local binding",
        )
    };
    let (binding, mut initializer, kind) = find("source")?;
    need(
        kind == if case.form == 1 {
            VariableKind::Var
        } else {
            VariableKind::Val
        },
        "original val/var source form",
        kind,
    )?;
    if case.form == 2 {
        let alias = parsed.ast().expressions().get(initializer).unwrap();
        need(
            alias.payload() == &Expression::Name && sources.slice(alias.span()) == Ok("original"),
            "actual moved alias initializer",
            alias,
        )?;
        let (original, init, kind) = find("original")?;
        need(
            kind == VariableKind::Val && original.start() < binding.start(),
            "original alias formation precedes new root",
            original,
        )?;
        initializer = init;
    }
    let call = parsed.ast().expressions().get(initializer).unwrap();
    let Expression::Call {
        callee, arguments, ..
    } = call.payload()
    else {
        return Err(failure("actual source formation expression", call));
    };
    need(
        arguments.is_empty()
            && sources.slice(parsed.ast().expressions().get(*callee).unwrap().span())
                == Ok(producer),
        "actual selected zero-argument factory AST",
        call,
    )?;
    Ok(Site {
        scan: scan.span(),
        factory: declaration(producer)?.span(),
        call: call.span(),
        binding,
        for_span,
        body,
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
            .ok_or_else(|| failure("local transport actual entity", entity))?;
        let Definition::BlockParameter { block, .. } = data.definition else {
            return Err(failure("unrelated local owner/loan producer", data));
        };
        need(data.ty == ty, "local current transport type", data)?;
        let incoming = g.incoming(block);
        need(
            !incoming.is_empty(),
            "local full incoming is nonempty",
            entity,
        )?;
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
        if anchored.len() == before {
            break;
        }
    }
    need(
        parents.keys().all(|entity| anchored.contains(entity)),
        "every local incoming reaches immutable actual factory Call anchor",
        &parents,
    )
}
fn family(g: &Graph<'_>, anchor: EntityId) -> Result<BTreeSet<EntityId>, Failure> {
    let mut entities = BTreeSet::from([anchor]);
    loop {
        let before = entities.len();
        for (_, edge) in &g.edges {
            for (&arg, &param) in edge.arguments.iter().zip(&g.block(edge.target)?.parameters) {
                if entities.contains(&arg) {
                    entities.insert(param);
                }
            }
        }
        if entities.len() == before {
            break;
        }
    }
    for &entity in &entities {
        transport(g, entity, anchor)?;
    }
    Ok(entities)
}
fn cycle(g: &Graph<'_>, start: BlockId) -> bool {
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
    let (call_span, argument_span) = one(
        parsed.ast().expressions().iter().filter_map(|(_, n)| {
            let Expression::Call {
                callee, arguments, ..
            } = n.payload()
            else {
                return None;
            };
            let [argument] = arguments.as_slice() else {
                return None;
            };
            let argument_span = parsed
                .ast()
                .expressions()
                .get(argument.value)
                .unwrap()
                .span();
            (inside(n.span(), scan)
                && sources.slice(parsed.ast().expressions().get(*callee).unwrap().span())
                    == Ok("println")
                && sources
                    .slice(argument_span)
                    .ok()
                    .and_then(|s| s.strip_prefix('"'))
                    .and_then(|s| s.strip_suffix('"'))
                    == Some(wanted))
            .then_some((n.span(), argument_span))
        }),
        "original scanner marker call",
    )?;
    let print = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(call_span)
                && matches!(
                    i.operation,
                    Operation::PrintString { .. } | Operation::PrintLiteral { .. }
                )
        }),
        "actual original scanner marker",
    )?;
    match &print.operation {
        Operation::PrintLiteral { bytes } => {
            let mut expected = wanted.as_bytes().to_vec();
            expected.push(b'\n');
            need(
                bytes == &expected,
                "actual scanner marker literal bytes",
                print,
            )?;
        }
        Operation::PrintString { value: loan } => {
            let birth = g.definition(EntityId::Loan(*loan))?;
            let Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            } = birth.operation
            else {
                return Err(failure("scanner marker real String loan", birth));
            };
            let root = g.definition(EntityId::Place(place))?;
            let Operation::RootPlace { owner } = root.operation else {
                return Err(failure("scanner marker String owner", root));
            };
            let literal = g.definition(EntityId::Value(owner))?;
            need(
                literal.origin == Origin::Source(argument_span)
                    && matches!(&literal.operation,Operation::StringLiteral {bytes,..} if bytes==wanted.as_bytes()),
                "actual scanner marker String bytes",
                literal,
            )?;
        }
        _ => unreachable!(),
    }
    Ok(print)
}

fn returned(
    g: &Graph<'_>,
    module: &Module,
    sources: &SourceMap,
    parsed: &ParsedFile,
    site: &Site,
    evidence: &provider::Evidence<'_>,
    exit_uses: (usize, &mut Uses),
) -> Result<Option<(BlockId, ValueId)>, Failure> {
    let (exit, uses) = exit_uses;
    let body = parsed.ast().statements().get(site.body).unwrap().span();
    let original = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, n)| {
            let Expression::Return {
                value: Some(value), ..
            } = n.payload()
            else {
                return None;
            };
            inside(n.span(), site.scan).then_some((n.span(), *value))
        })
        .collect::<Vec<_>>();
    let (outer_span, outer_value) = one(
        original
            .iter()
            .copied()
            .filter(|(span, _)| span.start() > site.for_span.end()),
        "original scanner empty fallback Return",
    )?;
    let zero_ast = parsed.ast().expressions().get(outer_value).unwrap();
    need(
        sources.slice(zero_ast.span()) == Ok("0"),
        "original scanner fallback zero",
        zero_ast,
    )?;
    let zero = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(zero_ast.span())
                && i.operation == Operation::Constant(ScalarConstant::Integer(0))
        }),
        "actual scanner fallback zero",
    )?;
    let outer=one(g.blocks.iter().copied().filter(|&block|g.term(block).is_ok_and(|t|t.origin==Origin::Source(outer_span) && matches!(&t.kind,TerminatorKind::Return {values} if values.as_slice()==[value(zero).unwrap()]))),"actual empty/fallthrough Return uses original fallback zero")?;
    need(g.function.entity(result(zero)?).is_some_and(|e|matches!(e.ty,EntityType::Value(t) if module.type_kind(t)==Some(&SsaTypeKind::Integer {bits:32,signed:true}))),"actual fallback Int storage",zero)?;
    complete_return_value(g, result(zero)?, outer)?;
    need(
        reachable(g, evidence.exhausted, outer),
        "actual exhaustion reaches original fallback Return",
        outer,
    )?;
    if exit != 3 {
        need(
            original.len() == 1
                && g.blocks
                    .iter()
                    .filter(|&&block| {
                        matches!(g.term(block).unwrap().kind, TerminatorKind::Return { .. })
                    })
                    .count()
                    == 1,
            "normal/Continue/Break have only original fallback Return",
            &original,
        )?;
        return Ok(None);
    }
    need(
        original.len() == 2,
        "body and outer original Returns",
        &original,
    )?;
    let (return_span, operand) = one(
        original
            .iter()
            .copied()
            .filter(|(span, _)| inside(*span, body)),
        "actual original body Return",
    )?;
    let call_ast = parsed.ast().expressions().get(operand).unwrap();
    let Expression::Call {
        callee, arguments, ..
    } = call_ast.payload()
    else {
        return Err(failure("body actual result call AST", call_ast));
    };
    need(
        sources.slice(parsed.ast().expressions().get(*callee).unwrap().span()) == Ok("result")
            && arguments.len() == 2,
        "original actual result call",
        call_ast,
    )?;
    for (index, argument) in arguments.iter().enumerate() {
        need(
            sources.slice(
                parsed
                    .ast()
                    .expressions()
                    .get(argument.value)
                    .unwrap()
                    .span(),
            ) == Ok(if index == 0 { "key" } else { "value" }),
            "actual original result K/V arguments",
            call_ast,
        )?;
    }
    let decl=one(parsed.roots().iter().filter_map(|id|{let n=parsed.ast().items().get(*id).ok()?;matches!(n.payload(),Item::Function {name:NameMarker::Present(name),..} if sources.slice(*name)==Ok("result")).then_some(n.span())}),"original result callable declaration")?;
    let callee = one(
        module
            .functions
            .iter()
            .filter(|f| f.origin == Origin::Source(decl)),
        "actual selected result callable",
    )?;
    let call=one(g.instructions.iter().copied().filter(|i|i.origin==Origin::Source(call_ast.span()) && matches!(&i.operation,Operation::DirectCall {callee:actual,receiver:None,arguments} if *actual==callee.id && arguments.as_slice()==[result(evidence.key).unwrap(),result(evidence.val).unwrap()])),"actual result call K/V loans")?;
    need(
        call.block == evidence.body
            && g.position(evidence.read)? < g.position(call)?
            && g.position(call)? < g.position(evidence.local_drop)?,
        "actual Return operand evaluated before Guard cleanup",
        call,
    )?;
    for (index, component) in [evidence.key, evidence.val].into_iter().enumerate() {
        let entity = result(component)?;
        need(
            callee
                .block(callee.entry_block().unwrap())
                .unwrap()
                .parameters
                .get(index)
                .and_then(|p| callee.entity(*p))
                .is_some_and(|p| p.ty == g.function.entity(entity).unwrap().ty),
            "result exact Shared component ABI",
            callee,
        )?;
        uses.permit(entity, call);
    }
    let returned = value(call)?;
    let ty = g
        .function
        .entity(EntityId::Value(returned))
        .unwrap()
        .ty
        .semantic_type();
    need(
        module.type_kind(ty)
            == Some(&SsaTypeKind::Integer {
                bits: 32,
                signed: true,
            })
            && callee.return_types.as_slice() == [ty],
        "result actual Int Value return ABI",
        call,
    )?;
    let term = g.term(evidence.body)?;
    need(
        term.origin == Origin::Source(return_span)
            && matches!(&term.kind,TerminatorKind::Return {values} if values.as_slice()==[returned]),
        "actual transfer Return returns actual evaluated operand",
        term,
    )?;
    complete_return_value(g, EntityId::Value(returned), evidence.body)?;
    uses.protect(EntityId::Value(returned));
    need(
        g.blocks
            .iter()
            .filter(|&&block| matches!(g.term(block).unwrap().kind, TerminatorKind::Return { .. }))
            .count()
            == 2,
        "exact body/fallback Return successors",
        g.function.id,
    )?;
    Ok(Some((evidence.body, returned)))
}
fn complete_return_value(
    g: &Graph<'_>,
    entity: EntityId,
    returned: BlockId,
) -> Result<(), Failure> {
    need(g.instructions.iter().all(|i|!i.operation.entities().contains(&entity)) && g.edges.iter().all(|(_,edge)|!edge.arguments.contains(&entity))
        && g.blocks.iter().all(|&block|{let term=g.term(block).unwrap();if block==returned{matches!(&term.kind,TerminatorKind::Return {values} if matches!(entity,EntityId::Value(v) if values.as_slice()==[v]))}else{!term.kind.entities().contains(&entity)}}),"actual scalar result has exactly original Return use and no cleanup consumption",entity)
}

fn paths(
    g: &Graph<'_>,
    producer: InstructionId,
    formed: InstructionId,
    after: InstructionId,
    ends: &[&Instruction],
    drops: &[&Instruction],
    exit: usize,
) -> Result<(), Failure> {
    let mut pending = vec![(g.function.entry_block().unwrap(), 0u8, 0u8, 0u8, false)];
    let mut seen = BTreeSet::new();
    while let Some((block, mut produced, mut ended, mut dropped, mut marked)) = pending.pop() {
        if !seen.insert((block, produced, ended, dropped, marked)) {
            continue;
        }
        for i in g.ops(block)? {
            if i.id == producer {
                produced += 1;
                need(
                    produced == 1,
                    "actual factory evaluated once on every path",
                    i,
                )?;
            }
            if i.id == formed {
                need(
                    produced == 1 && dropped == 0,
                    "formed marker after real producer while owner live",
                    i,
                )?;
                marked = true;
            }
            if matches!(i.operation, Operation::MapIterationFirst { .. }) {
                need(
                    produced == 1 && marked && dropped == 0,
                    "provider starts on real formed live owner",
                    i,
                )?;
            }
            if ends.iter().any(|end| end.id == i.id) {
                ended += 1;
                need(
                    ended == 1 && dropped == 0,
                    "each local path ends its provider once",
                    i,
                )?;
            }
            if drops.iter().any(|drop| drop.id == i.id) {
                dropped += 1;
                need(
                    dropped == 1 && ended == 1,
                    "each local path drops exact owner once after EndSource",
                    i,
                )?;
            }
            if i.id == after {
                need(
                    dropped == 1,
                    "local ASAP Drop precedes actual post-loop marker",
                    i,
                )?;
            }
        }
        if matches!(g.term(block)?.kind, TerminatorKind::Return { .. }) {
            need(
                produced == 1 && marked && ended == 1 && dropped == 1,
                "every finite normal Return has one real producer/EndSource/MapDrop",
                g.term(block)?,
            )?;
        }
        for (_, edge) in g.edges.iter().filter(|(from, _)| *from == block) {
            pending.push((edge.target, produced, ended, dropped, marked));
        }
    }
    need(exit <= 3, "original direct exit kind", exit)
}

fn reachable(g: &Graph<'_>, start: BlockId, target: BlockId) -> bool {
    let mut pending = vec![start];
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
