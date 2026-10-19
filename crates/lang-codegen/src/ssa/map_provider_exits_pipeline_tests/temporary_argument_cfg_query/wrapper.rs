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
