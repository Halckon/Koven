//! Read-only proof for the empty-Map factory's ordinary argument CFG and hidden owner.
pub(super) mod argument;
mod ast {
    //! Exact ordinary source Call and argument If; independent of expression arena slots.
    use super::*;
    use lang_frontend::{
        ast::StatementId,
        parser::{IntegerLiteralKind, Item, LiteralKind, NameMarker, Statement},
    };
    pub(super) struct Loop {
        pub span: Span,
        pub body: StatementId,
        pub read: Span,
        pub arguments: [Span; 2],
    }
    pub(super) struct Input {
        pub scan: Span,
        pub factory: Span,
        pub call: Span,
        pub argument: Span,
        pub flag: Span,
        pub arms: [Span; 2],
        pub loop_site: Option<Loop>,
        pub size: Option<Span>,
        pub drop_origin: Span,
        pub construct: Span,
        pub factory_return: Span,
        pub mutable: bool,
    }
    fn inside(inner: Span, outer: Span) -> bool {
        inner.source_id() == outer.source_id()
            && inner.start() > outer.start()
            && inner.end() < outer.end()
    }
    pub(super) fn input(sources: &SourceMap, parsed: &ParsedFile) -> Result<Input, Failure> {
        let expressions = parsed.ast().expressions();
        let (call_id, call, argument) = one(
            expressions.iter().filter_map(|(id, n)| match n.payload() {
                Expression::Call {
                    callee, arguments, ..
                } if sources.slice(expressions.get(*callee).ok()?.span()) == Ok("produce")
                    && arguments.len() == 1 =>
                {
                    Some((id, n.span(), arguments[0].value))
                }
                _ => None,
            }),
            "original unique produce Call",
        )?;
        let scan = one(
            parsed.roots().iter().filter_map(|id| {
                let n = parsed.ast().items().get(*id).ok()?;
                (inside(call, n.span()) && matches!(n.payload(), Item::Function { .. })).then_some(n)
            }),
            "actual containing scanner declaration",
        )?;
        let Item::Function { parameters, .. } = scan.payload() else {
            unreachable!()
        };
        let [parameter] = parameters.as_slice() else {
            return Err(failure("scanner original one Boolean parameter", scan));
        };
        let NameMarker::Present(parameter_name) = parameter.name else {
            return Err(failure("scanner real parameter Name", parameter));
        };
        need(
            parameter.mode_marker.is_none()
                && sources.slice(parameter_name) == Ok("flag")
                && sources.slice(
                    parsed
                        .ast()
                        .type_refs()
                        .get(parameter.type_ref)
                        .unwrap()
                        .span(),
                ) == Ok("Boolean"),
            "original scanner Borrow Boolean syntax",
            parameter,
        )?;
        let factory = one(
            parsed.roots().iter().filter_map(|id| {
                let n = parsed.ast().items().get(*id).ok()?;
                let Item::Function {
                    name: NameMarker::Present(name),
                    ..
                } = n.payload()
                else {
                    return None;
                };
                (sources.slice(*name) == Ok("produce")).then_some(n)
            }),
            "original selected produce declaration",
        )?;
        let Item::Function { parameters, .. } = factory.payload() else {
            unreachable!()
        };
        let [p] = parameters.as_slice() else {
            return Err(failure("factory original one Int parameter", factory));
        };
        let NameMarker::Present(factory_parameter) = p.name else {
            return Err(failure("factory parameter original Name", p));
        };
        need(
            p.mode_marker.is_none()
                && sources.slice(factory_parameter) == Ok("input")
                && sources.slice(parsed.ast().type_refs().get(p.type_ref).unwrap().span()) == Ok("Int"),
            "factory original Borrow Int syntax",
            p,
        )?;
        let argument_node = expressions.get(argument).unwrap();
        let Expression::If {
            condition,
            then_branch,
            else_branch: Some(no),
            ..
        } = argument_node.payload()
        else {
            return Err(failure("original actual argument If", argument_node));
        };
        let flag = expressions.get(*condition).unwrap();
        need(
            flag.payload() == &Expression::Name && sources.slice(flag.span()) == Ok("flag"),
            "original argument condition flag Name",
            flag,
        )?;
        let arm = |statement, wanted| -> Result<Span, Failure> {
            let n = parsed.ast().statements().get(statement).unwrap();
            // parse_if -> parse_control_body -> parse_control_block preserves a
            // ControlBody for a braced value arm; an ordinary Block is a different node.
            let Statement::ControlBody { elements } = n.payload() else {
                return Err(failure(
                    "original argument arm ControlBody",
                    (n, n.payload()),
                ));
            };
            let [statement] = elements.as_slice() else {
                return Err(failure("original single literal arm", n));
            };
            let n = parsed.ast().statements().get(*statement).unwrap();
            let Statement::Expression { expression } = n.payload() else {
                return Err(failure("original literal arm statement", n));
            };
            let n = expressions.get(*expression).unwrap();
            need(
                n.payload()
                    == &Expression::Literal(LiteralKind::Integer(IntegerLiteralKind::Unsuffixed))
                    && sources.slice(n.span()) == Ok(wanted),
                "original selected arm literal",
                n,
            )?;
            Ok(n.span())
        };
        let arms = [arm(*then_branch, "1")?, arm(*no, "2")?];
        let loops = parsed
            .ast()
            .statements()
            .iter()
            .filter_map(|(_, n)| match n.payload() {
                Statement::For { source, body, .. } if inside(n.span(), scan.span()) => {
                    Some((n.span(), *source, *body))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let loop_site = if let [(span, source, body)] = loops.as_slice() {
            need(
                *source == call_id,
                "original For source is the selected Call itself",
                source,
            )?;
            let (read, args) = one(
                expressions.iter().filter_map(|(_, n)| match n.payload() {
                    Expression::Call {
                        callee, arguments, ..
                    } if inside(n.span(), *span)
                        && sources.slice(expressions.get(*callee).ok()?.span()) == Ok("read")
                        && arguments.len() == 2 =>
                    {
                        Some((n.span(), [arguments[0].value, arguments[1].value]))
                    }
                    _ => None,
                }),
                "actual body read Call",
            )?;
            let arguments = args.map(|id| expressions.get(id).unwrap().span());
            need(
                arguments
                    .iter()
                    .zip(["key", "value"])
                    .all(|(s, w)| sources.slice(*s) == Ok(w)),
                "original body K/V argument order",
                arguments,
            )?;
            Some(Loop {
                span: *span,
                body: *body,
                read,
                arguments,
            })
        } else {
            need(loops.is_empty(), "exact zero or one original For", &loops)?;
            None
        };
        let (size, drop_origin) = if loop_site.is_none() {
            let marker = one(
                parsed
                    .ast()
                    .items()
                    .iter()
                    .filter_map(|(_, n)| match n.payload() {
                        Item::Variable {
                            name: NameMarker::Present(name),
                            initializer,
                            ..
                        } if *initializer == call_id => Some(*name),
                        _ => None,
                    }),
                "ordinary control original owner binding",
            )?;
            let size = one(
                expressions.iter().filter_map(|(_, n)| match n.payload() {
                    Expression::Member {
                        receiver,
                        name_span,
                        ..
                    } if inside(n.span(), scan.span())
                        && sources.slice(*name_span) == Ok("size")
                        && sources.slice(expressions.get(*receiver).ok()?.span())
                            == sources.slice(marker) =>
                    {
                        Some(n.span())
                    }
                    _ => None,
                }),
                "control actual source size",
            )?;
            (Some(size), marker)
        } else {
            (None, call)
        };
        let (construct, mutable) = one(
            expressions.iter().filter_map(|(_, n)| {
                let Expression::Call {
                    callee, arguments, ..
                } = n.payload()
                else {
                    return None;
                };
                if !inside(n.span(), factory.span()) || !arguments.is_empty() {
                    return None;
                }
                match sources.slice(expressions.get(*callee).ok()?.span()) {
                    Ok("mapOf") => Some((n.span(), false)),
                    Ok("mutableMapOf") => Some((n.span(), true)),
                    _ => None,
                }
            }),
            "factory original empty Map construction",
        )?;
        let factory_return = one(
            expressions.iter().filter_map(|(_, n)| match n.payload() {
                Expression::Return { value: Some(v), .. }
                    if inside(n.span(), factory.span())
                        && expressions.get(*v).ok()?.span() == construct =>
                {
                    Some(n.span())
                }
                _ => None,
            }),
            "factory returns actual empty construct",
        )?;
        Ok(Input {
            scan: scan.span(),
            factory: factory.span(),
            call,
            argument: argument_node.span(),
            flag: flag.span(),
            arms,
            loop_site,
            size,
            drop_origin,
            construct,
            factory_return,
            mutable,
        })
    }
}
mod provider;
mod wrapper {
    //! Original runtime entry's literal Boolean is delivered to this exact scanner Borrow ABI.
    use super::*;
    use lang_frontend::parser::{Item, LiteralKind, NameMarker};

    pub(super) fn check(
        sources: &SourceMap,
        parsed: &ParsedFile,
        module: &Module,
        scan: &Function,
    ) -> Result<(), Failure> {
        let declaration = one(
            parsed.roots().iter().filter_map(|id| {
                let node = parsed.ast().items().get(*id).ok()?;
                (Origin::Source(node.span()) == scan.origin).then_some(node)
            }),
            "argument CFG original containing scanner item",
        )?;
        let Item::Function {
            name: NameMarker::Present(name),
            ..
        } = declaration.payload()
        else {
            return Err(failure(
                "argument CFG scanner original function name",
                declaration,
            ));
        };
        if sources.slice(*name) == Ok("entry") {
            return Ok(());
        }
        need(
            sources.slice(*name) == Ok("scan"),
            "runtime scanner original name",
            name,
        )?;
        let original = one(
            parsed.roots().iter().filter_map(|id| {
                let node = parsed.ast().items().get(*id).ok()?;
                let Item::Function {
                    name: NameMarker::Present(name),
                    parameters,
                    ..
                } = node.payload()
                else {
                    return None;
                };
                (sources.slice(*name) == Ok("entry") && parameters.is_empty()).then_some(node.span())
            }),
            "runtime wrapper actual zeroarg entry declaration",
        )?;
        let inside = |span: Span| {
            span.source_id() == original.source_id()
                && original.start() < span.start()
                && span.end() < original.end()
        };
        let expressions = parsed.ast().expressions();
        let (call_span, argument) = one(
            expressions.iter().filter_map(|(_, node)| {
                let Expression::Call {
                    callee, arguments, ..
                } = node.payload()
                else {
                    return None;
                };
                (inside(node.span())
                    && sources.slice(expressions.get(*callee).ok()?.span()) == Ok("scan")
                    && arguments.len() == 1)
                    .then_some((node.span(), arguments.first()?.value))
            }),
            "runtime wrapper original unique scan Call",
        )?;
        let argument = expressions.get(argument).unwrap();
        let Expression::Literal(LiteralKind::Boolean(expected)) = argument.payload() else {
            return Err(failure("runtime wrapper actual Boolean literal", argument));
        };
        need(
            sources.slice(argument.span()) == Ok(if *expected { "true" } else { "false" }),
            "runtime wrapper original literal spelling",
            argument,
        )?;
        let entry = function(module, original)?;
        let g = Graph::new(entry)?;
        let block = entry.entry_block().unwrap();
        need(
            g.block(block)?.parameters.is_empty() && g.blocks == [block] && g.edges.is_empty(),
            "runtime wrapper actual zeroarg straight normal entry",
            entry.id,
        )?;
        let constant = at(&g, argument.span(), |o| {
            *o == Operation::Constant(ScalarConstant::Boolean(*expected))
        })?;
        let call = one(
            g.instructions.iter().copied().filter(
                |i| matches!(i.operation, Operation::DirectCall { callee, .. } if callee == scan.id),
            ),
            "runtime wrapper exact actual scanner Call",
        )?;
        let Operation::DirectCall {
            receiver: None,
            arguments,
            ..
        } = &call.operation
        else {
            return Err(failure("runtime wrapper actual scanner direct ABI", call));
        };
        let [arg @ EntityId::Loan(actual_loan)] = arguments.as_slice() else {
            return Err(failure(
                "runtime wrapper actual scanner Borrow argument",
                call,
            ));
        };
        let begin = g.definition(*arg)?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = begin.operation
        else {
            return Err(failure(
                "runtime wrapper actual Shared Boolean birth",
                begin,
            ));
        };
        let root = g.definition(EntityId::Place(place))?;
        let boolean = entry.entity(result(constant)?).unwrap().ty.semantic_type();
        let parameters = &scan.block(scan.entry_block().unwrap()).unwrap().parameters;
        let shared = EntityType::Loan {
            kind: LoanKind::Shared,
            target: boolean,
        };
        need(
            module.type_kind(boolean) == Some(&SsaTypeKind::Boolean)
                && entry.entity(*arg).unwrap().ty == shared
                && entry.entity(EntityId::Place(place)).unwrap().ty == EntityType::Place(boolean)
                && matches!(parameters.as_slice(), [p] if scan.entity(*p).unwrap().ty == shared)
                && scan.return_types.is_empty()
                && call.results.is_empty(),
            "runtime literal actual Boolean Value/place/Shared/scanner ABI",
            call,
        )?;
        let end = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: *actual_loan }),
            "runtime wrapper unique actual scanner CallReturn End",
        )?;
        let before = marker(sources, parsed, &g, original, "begin")?;
        let after = marker(sources, parsed, &g, original, "after")?;
        need(
            root.operation
                == Operation::RootPlace {
                    owner: value(constant)?,
                }
                && root.origin == Origin::Source(argument.span())
                && begin.origin == root.origin
                && call.origin == Origin::Source(call_span)
                && end.origin == call.origin
                && [before, constant, root, begin, call, end, after]
                    .iter()
                    .all(|i| i.block == block)
                && g.position(before)? < g.position(constant)?
                && g.position(constant)? < g.position(root)?
                && g.position(root)? < g.position(begin)?
                && g.position(begin)? < g.position(call)?
                && g.position(call)? < g.position(end)?
                && g.position(end)? < g.position(after)?,
            "runtime begin/literal/root/Shared/actual scan/unique End/after order",
            (before, constant, root, begin, call, end, after),
        )?;
        need(
            matches!(&g.term(block)?.kind, TerminatorKind::Return { values } if values.is_empty())
                && g.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
                    .count()
                    == 1
                && !g.instructions.iter().any(|i| {
                    iteration(&i.operation)
                        || matches!(
                            i.operation,
                            Operation::MapConstruct { .. } | Operation::MapSize { .. }
                        )
                }),
            "runtime wrapper normal Return delegates container work to exact scanner",
            g.term(block)?,
        )?;
        let mut uses = Uses::default();
        uses.protect(result(constant)?);
        uses.permit(result(constant)?, root);
        uses.protect(EntityId::Place(place));
        uses.permit(EntityId::Place(place), begin);
        uses.protect(*arg);
        uses.permit(*arg, call);
        uses.permit(*arg, end);
        uses.check(&g, None)
    }

    fn marker<'p>(
        sources: &SourceMap,
        parsed: &ParsedFile,
        g: &Graph<'p>,
        scope: Span,
        wanted: &str,
    ) -> Result<&'p Instruction, Failure> {
        let (call, argument) = one(
            parsed.ast().expressions().iter().filter_map(|(_, node)| {
                let Expression::Call {
                    callee, arguments, ..
                } = node.payload()
                else {
                    return None;
                };
                let [argument] = arguments.as_slice() else {
                    return None;
                };
                let arg = parsed.ast().expressions().get(argument.value).ok()?.span();
                (scope.source_id() == node.span().source_id()
                    && scope.start() < node.span().start()
                    && node.span().end() < scope.end()
                    && sources.slice(parsed.ast().expressions().get(*callee).ok()?.span())
                        == Ok("println")
                    && sources.slice(arg) == Ok(format!("\"{wanted}\"").as_str()))
                .then_some((node.span(), arg))
            }),
            "runtime wrapper original marker Call",
        )?;
        at(g, call, |o| match o {
            Operation::PrintLiteral { bytes } => {
                let mut expected = wanted.as_bytes().to_vec();
                expected.push(b'\n');
                *bytes == expected
            }
            Operation::PrintString { value: loan } => {
                let Ok(begin) = g.definition(EntityId::Loan(*loan)) else {
                    return false;
                };
                let Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                } = begin.operation
                else {
                    return false;
                };
                let Ok(root) = g.definition(EntityId::Place(place)) else {
                    return false;
                };
                let Operation::RootPlace { owner } = root.operation else {
                    return false;
                };
                g.definition(EntityId::Value(owner)).is_ok_and(|i| i.origin == Origin::Source(argument)
                    && matches!(&i.operation, Operation::StringLiteral { bytes, .. } if bytes == wanted.as_bytes()))
            }
            _ => false,
        })
    }
}
use super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, inherits, loan, result, value},
    query_inputs::one,
};
use crate::ssa::model::*;
use lang_frontend::{
    parser::{Expression, ParsedFile},
    source::{SourceMap, Span},
};
use std::collections::BTreeSet;

fn at<'p>(
    g: &Graph<'p>,
    span: Span,
    predicate: impl Fn(&Operation) -> bool,
) -> Result<&'p Instruction, Failure> {
    one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.origin == Origin::Source(span) && predicate(&i.operation)),
        "argument CFG original operation",
    )
}
fn family(g: &Graph<'_>, anchor: EntityId) -> Result<BTreeSet<EntityId>, Failure> {
    let mut entities = BTreeSet::from([anchor]);
    loop {
        let before = entities.len();
        for (_, edge) in &g.edges {
            for (&arg, &p) in edge.arguments.iter().zip(&g.block(edge.target)?.parameters) {
                if entities.contains(&arg) {
                    entities.insert(p);
                }
            }
        }
        if before == entities.len() {
            break;
        }
    }
    for &e in &entities {
        g.anchored(e, &[anchor])?;
    }
    Ok(entities)
}

pub(super) fn check(
    sources: &SourceMap,
    parsed: &ParsedFile,
    program: &Program,
) -> Result<(), Failure> {
    let input = ast::input(sources, parsed)?;
    let module = one(program.modules.iter(), "argument CFG actual module")?;
    let scan = function(module, input.scan)?;
    let factory = function(module, input.factory)?;
    wrapper::check(sources, parsed, module, scan)?;
    let g = Graph::with_abort(scan)?;
    let mut uses = Uses::default();
    let argument_input = argument::Input {
        call: input.call,
        argument: input.argument,
        flag: input.flag,
        arms: input.arms,
    };
    let (call, argument_end) = argument::check_unit(
        sources,
        parsed,
        &argument_input,
        module,
        &g,
        factory,
        &mut uses,
    )?;
    let owner = value(call)?;
    let map = scan
        .entity(EntityId::Value(owner))
        .unwrap()
        .ty
        .semantic_type();
    let (kind, key, val) = module
        .map_container(map)
        .ok_or_else(|| failure("argument CFG actual Map result", call))?;
    need(
        kind == if input.mutable {
            MapContainerKind::MutableMap
        } else {
            MapContainerKind::Map
        } && [key, val]
            .iter()
            .all(|t| module.type_kind(*t) == Some(&SsaTypeKind::StringOwner))
            && factory.return_types.as_slice() == [map],
        "selected factory returns exact empty Map String storage",
        call,
    )?;
    empty_factory(&input, module, factory, map)?;
    let owners = family(&g, EntityId::Value(owner))?;
    for &e in &owners {
        uses.protect(e);
    }
    for (index, data) in scan.values.iter().enumerate() {
        if module.map_container(data.ty.semantic_type()).is_some() {
            let e = EntityId::Value(ValueId {
                function: scan.id,
                index,
            });
            need(
                data.ty == EntityType::Value(map) && owners.contains(&e),
                "all Map Value nodes belong to sole actual Call lineage",
                (e, data),
            )?;
            g.anchored(e, &[EntityId::Value(owner)])?;
        }
    }
    let drops = g
        .instructions
        .iter()
        .copied()
        .filter(|i| {
            let Operation::Drop { owner } = i.operation else {
                return false;
            };
            let ty = scan
                .entity(EntityId::Value(owner))
                .unwrap()
                .ty
                .semantic_type();
            module.map_container(ty).is_some()
        })
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        return Err(failure("argument CFG unique Map Drop", &drops));
    };
    let source_end = if let Some(site) = &input.loop_site {
        Some(provider::check(
            sources,
            parsed,
            &input,
            site,
            module,
            &g,
            call,
            argument_end,
            drop,
            &owners,
            &mut uses,
        )?)
    } else {
        let size = at(&g, input.size.unwrap(), |o| {
            matches!(o, Operation::MapSize { .. })
        })?;
        need(
            size.operation
                == Operation::MapSize {
                    owner: EntityId::Value(owner),
                }
                && size.block == call.block
                && argument_end.block == size.block
                && g.position(argument_end)? < g.position(size)?
                && drop.operation == Operation::Drop { owner }
                && drop.origin == Origin::Source(input.drop_origin)
                && drop.block == size.block
                && g.position(drop)? == g.position(size)? + 1,
            "ordinary control argument End/actual owner size/ASAP Drop",
            (argument_end, size, drop),
        )?;
        need(
            !g.instructions.iter().any(|i| iteration(&i.operation)),
            "noFor ordinary control contains no iteration operations",
            scan.id,
        )?;
        uses.permit(EntityId::Value(owner), size);
        uses.permit(EntityId::Value(owner), drop);
        super::query_inventory::check(&g, &[map], &BTreeSet::from([EntityId::Value(owner)]))?;
        None
    };
    uses.check(&g, None)?;
    paths(&g, call.id, drop.id, source_end.map(|i| i.id))
}
fn iteration(o: &Operation) -> bool {
    matches!(
        o,
        Operation::MapIterationFirst { .. }
            | Operation::MapIterationHasNext { .. }
            | Operation::MapIterationNext { .. }
            | Operation::MapIterationKeyLoan { .. }
            | Operation::MapIterationValueLoan { .. }
    )
}
fn empty_factory(
    input: &ast::Input,
    module: &Module,
    f: &Function,
    map: SsaTypeId,
) -> Result<(), Failure> {
    let fg = Graph::with_abort(f)?;
    let construct = at(&fg, input.construct, |o| {
        matches!(o, Operation::MapConstruct { .. })
    })?;
    need(
        construct.operation == Operation::MapConstruct { map_type: map }
            && fg
                .instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapConstruct { .. }))
                .count()
                == 1
            && !fg.instructions.iter().any(|i| {
                matches!(
                    i.operation,
                    Operation::MapPut { .. }
                        | Operation::MapConsume { .. }
                        | Operation::DirectCall { .. }
                        | Operation::HeapAllocate { .. }
                )
            }),
        "factory creates only actual original zero-entry Map",
        construct,
    )?;
    let returns = fg
        .blocks
        .iter()
        .map(|&block| fg.term(block))
        .collect::<Result<Vec<_>, _>>()?;
    let returned = one(
        returns
            .iter()
            .copied()
            .filter(|t| matches!(t.kind, TerminatorKind::Return { .. })),
        "factory unique normal Return",
    )?;
    need(
        matches!(&returned.kind, TerminatorKind::Return { values }
            if values.as_slice() == [value(construct)?])
            && returned.origin == Origin::Source(input.factory_return)
            && returned == fg.term(construct.block)?,
        "factory sole normal Return delivers original empty construct",
        returned,
    )?;
    let map_values = f
        .values
        .iter()
        .filter(|d| module.map_container(d.ty.semantic_type()).is_some())
        .count();
    need(
        map_values == 1,
        "empty factory has no additional owner transport/producer",
        f.id,
    )
}
// Cycles are folded only after their complete live cleanup state has been visited.
fn paths(
    g: &Graph<'_>,
    call: InstructionId,
    drop: InstructionId,
    end: Option<InstructionId>,
) -> Result<(), Failure> {
    let mut pending = vec![(g.function.entry_block().unwrap(), 0u8, 0u8, 0u8)];
    let mut seen = BTreeSet::new();
    let mut returned = 0;
    while let Some((block, mut made, mut ended, mut dropped)) = pending.pop() {
        if !seen.insert((block, made, ended, dropped)) {
            continue;
        }
        for i in g.ops(block)? {
            if i.id == call {
                made += 1;
                need(
                    made == 1 && dropped == 0,
                    "argument CFG selected factory called once",
                    i,
                )?;
            }
            if Some(i.id) == end {
                ended += 1;
                need(
                    ended == 1 && made == 1 && dropped == 0,
                    "source ends once while hidden owner live",
                    i,
                )?;
            }
            if i.id == drop {
                dropped += 1;
                need(
                    dropped == 1 && made == 1 && ended == u8::from(end.is_some()),
                    "unique Drop follows actual source End",
                    i,
                )?;
            }
            if iteration(&i.operation) {
                need(
                    made == 1 && dropped == 0,
                    "provider operations only while original owner live",
                    i,
                )?;
            }
        }
        match &g.term(block)?.kind {
            TerminatorKind::Return { values } => {
                returned += 1;
                need(
                    values.is_empty()
                        && made == 1
                        && dropped == 1
                        && ended == u8::from(end.is_some()),
                    "every normal Return has one actual hidden-owner cleanup",
                    g.term(block)?,
                )?;
            }
            TerminatorKind::Abort => need(
                made == 1 && dropped == 1,
                "noFor failure branch starts after Pure source cleanup",
                g.term(block)?,
            )?,
            _ => {}
        }
        for (_, edge) in g.edges.iter().filter(|(from, _)| *from == block) {
            pending.push((edge.target, made, ended, dropped));
        }
    }
    need(
        returned > 0,
        "normal empty-Map CFG path exists",
        g.function.id,
    )
}
