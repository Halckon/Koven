//! Original Return operands and successful-path cleanup; shared provider proof lives in graph.
use super::*;
use lang_frontend::ast::ExpressionId;

pub(super) fn check(
    g: &Graph<'_>,
    module: &Module,
    input: &Input,
    sources: &SourceMap,
    parsed: &ParsedFile,
    uses: &mut Uses,
    body: (
        &Instruction,
        &Instruction,
        &Instruction,
        &Instruction,
        &Instruction,
    ),
) -> Result<(), Failure> {
    let (key, val, read, value_end, source_end) = body;
    let span = parsed.ast().statements().get(input.body).unwrap().span();
    let (return_span, operand) = one(
        parsed.ast().expressions().iter().filter_map(|(_, n)| {
            let Expression::Return { value, .. } = n.payload() else {
                return None;
            };
            (n.span().start() > span.start() && n.span().end() < span.end())
                .then_some((n.span(), *value))
        }),
        "original body Return",
    )?;
    let term = g.term(source_end.block)?;
    need(
        term.origin == Origin::Source(return_span),
        "actual terminal Return origin",
        term,
    )?;
    let TerminatorKind::Return { values } = &term.kind else {
        return Err(failure("body cleanup reaches actual Return", term));
    };
    let mut evaluated = BTreeSet::from([read.id]);
    // Both a returned finish call and a Unit prefix finish call consume the same
    // component identities. This is a source query, not a production capability gate.
    for (_, node) in parsed.ast().expressions().iter() {
        if node.span().start() <= span.start() || node.span().end() >= span.end() {
            continue;
        }
        let Expression::Call {
            callee, arguments, ..
        } = node.payload()
        else {
            continue;
        };
        if sources.slice(parsed.ast().expressions().get(*callee).unwrap().span()) != Ok("finish") {
            continue;
        }
        need(arguments.len() == 2, "original finish arity", node)?;
        let decl=one(parsed.roots().iter().filter_map(|id|{let n=parsed.ast().items().get(*id).ok()?; matches!(n.payload(),Item::Function{name:NameMarker::Present(name),..} if sources.slice(*name)==Ok("finish")).then_some(n.span())}),"original finish declaration")?;
        let target = one(
            module
                .functions
                .iter()
                .filter(|f| f.origin == Origin::Source(decl)),
            "actual finish callee",
        )?;
        let call = one(
            g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(node.span())
                    && matches!(i.operation, Operation::DirectCall { .. })
            }),
            "actual finish operand/prefix",
        )?;
        let Operation::DirectCall {
            callee,
            receiver: None,
            arguments: actual,
        } = &call.operation
        else {
            return Err(failure("finish source call ABI", call));
        };
        need(
            *callee == target.id && actual.len() == 2,
            "exact finish callee/arity",
            call,
        )?;
        for (index, component) in [key, val].into_iter().enumerate() {
            need(
                sources.slice(
                    parsed
                        .ast()
                        .expressions()
                        .get(arguments[index].value)
                        .unwrap()
                        .span(),
                ) == Ok(if index == 0 { "key" } else { "value" }),
                "finish original K/V argument",
                node,
            )?;
            transport(g, actual[index], result(component)?)?;
            need(
                target
                    .block(target.entry_block().unwrap())
                    .unwrap()
                    .parameters
                    .get(index)
                    .and_then(|p| target.entity(*p))
                    .is_some_and(|p| p.ty == g.function.entity(actual[index]).unwrap().ty),
                "finish exact Borrow parameter storage",
                target,
            )?;
            uses.protect(actual[index]);
            uses.permit(actual[index], call);
        }
        evaluated.insert(call.id);
    }
    match operand {
        None => need(
            values.is_empty(),
            "bare Unit Return has no fabricated result",
            term,
        )?,
        Some(expression) => {
            let expected = expression_value(g, parsed, sources, expression, &mut evaluated)?;
            let [actual] = values.as_slice() else {
                return Err(failure("one actual Return result", term));
            };
            transport(g, EntityId::Value(*actual), expected)?;
            returned_uses(g, expected, source_end.block)?;
        }
    }
    let guard = guard(g, module, input, sources, parsed, read, uses)?;
    normal_body(g, key.block, source_end, value_end, guard, &evaluated)?;
    // Exhaustion executes the real outer Return (or Unit fallthrough), rather
    // than fabricating the nonempty body result on the empty source edge.
    let outer = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, n)| {
            let Expression::Return { value, .. } = n.payload() else {
                return None;
            };
            (n.span().start() > input.for_span.end() && n.span().end() < input.scan.end())
                .then_some((n.span(), *value))
        })
        .collect::<Vec<_>>();
    let returns = g
        .blocks
        .iter()
        .copied()
        .filter(|b| {
            matches!(
                g.term(*b).map(|t| &t.kind),
                Ok(TerminatorKind::Return { .. })
            )
        })
        .collect::<Vec<_>>();
    need(
        returns.len() == 2,
        "body and exhaustion have independent normal Returns",
        &returns,
    )?;
    let exhausted = one(
        returns.iter().copied().filter(|b| *b != source_end.block),
        "actual exhaustion Return",
    )?;
    let end_term = g.term(exhausted)?;
    if outer.is_empty() {
        need(
            matches!(&end_term.kind,TerminatorKind::Return {values} if values.is_empty()),
            "empty source Unit fallthrough",
            end_term,
        )?;
    } else {
        let [(span, Some(expression))] = outer.as_slice() else {
            return Err(failure("one original outer result Return", &outer));
        };
        need(
            end_term.origin == Origin::Source(*span),
            "actual outer Return identity",
            end_term,
        )?;
        let mut evaluated = BTreeSet::new();
        let expected = expression_value(g, parsed, sources, *expression, &mut evaluated)?;
        let TerminatorKind::Return { values } = &end_term.kind else {
            unreachable!()
        };
        let [actual] = values.as_slice() else {
            return Err(failure("outer Return result", end_term));
        };
        transport(g, EntityId::Value(*actual), expected)?;
        returned_uses(g, expected, exhausted)?;
        let first = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| matches!(i.operation, Operation::MapIterationHasNext { .. })),
            "actual exhaustion selector",
        )?;
        let TerminatorKind::Conditional { when_false, .. } = &g.term(first.block)?.kind else {
            unreachable!()
        };
        outer_order(
            g,
            when_false.target,
            &evaluated,
            g.function
                .entity(g.block(g.function.entry_block().unwrap())?.parameters[0])
                .unwrap()
                .ty
                .semantic_type(),
            input,
        )?;
    }
    Ok(())
}

fn expression_value(
    g: &Graph<'_>,
    parsed: &ParsedFile,
    sources: &SourceMap,
    id: ExpressionId,
    evaluated: &mut BTreeSet<InstructionId>,
) -> Result<EntityId, Failure> {
    let n = parsed.ast().expressions().get(id).unwrap();
    let instruction = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(n.span())
                && matches!(
                    i.operation,
                    Operation::Constant(_)
                        | Operation::StringLiteral { .. }
                        | Operation::DirectCall { .. }
                        | Operation::CheckedArithmetic { .. }
                )
        }),
        "actual original Return operand operation",
    )?;
    match n.payload() {
        Expression::Call { .. } => need(
            matches!(instruction.operation, Operation::DirectCall { .. }),
            "actual returned call",
            instruction,
        )?,
        Expression::Literal(_) => {
            let expected = sources
                .slice(n.span())
                .unwrap()
                .parse::<i128>()
                .map_err(|e| failure("original numeric Return literal", e))?;
            need(
                instruction.operation == Operation::Constant(ScalarConstant::Integer(expected)),
                "exact literal Return operand",
                instruction,
            )?;
        }
        Expression::String { .. } => {
            let raw = sources.slice(n.span()).unwrap();
            let bytes = raw
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .ok_or_else(|| failure("original simple Return String", raw))?
                .as_bytes();
            need(
                matches!(&instruction.operation,Operation::StringLiteral {bytes:actual,..} if actual==bytes),
                "exact owned String Return bytes",
                instruction,
            )?;
        }
        Expression::Binary {
            left,
            operator,
            right,
            ..
        } => {
            need(
                *operator == lang_frontend::parser::BinaryOperator::Add,
                "original Return addition",
                operator,
            )?;
            let left = expression_value(g, parsed, sources, *left, evaluated)?;
            let right = expression_value(g, parsed, sources, *right, evaluated)?;
            need(
                matches!(instruction.operation,Operation::CheckedArithmetic {operator:CheckedArithmeticOperator::Add,left:l,right:r} if EntityId::Value(l)==left && EntityId::Value(r)==right),
                "exact original Return arithmetic operands",
                instruction,
            )?;
            let [EntityId::Value(_), EntityId::Value(failed)] = instruction.results.as_slice()
            else {
                return Err(failure(
                    "checked operand value/failure results",
                    instruction,
                ));
            };
            let TerminatorKind::Conditional {
                condition,
                when_true,
                when_false,
            } = &g.term(instruction.block)?.kind
            else {
                return Err(failure(
                    "checked Return operand overflow branch",
                    instruction,
                ));
            };
            need(
                condition == failed
                    && matches!(g.term(when_true.target)?.kind, TerminatorKind::Abort)
                    && !matches!(g.term(when_false.target)?.kind, TerminatorKind::Abort),
                "Return operand failure aborts before cleanup",
                instruction,
            )?;
        }
        _ => return Err(failure("unaccounted fixture Return operand", n)),
    }
    evaluated.insert(instruction.id);
    instruction
        .results
        .first()
        .copied()
        .ok_or_else(|| failure("actual Return operand value", instruction))
}

fn returned_uses(g: &Graph<'_>, anchor: EntityId, returned: BlockId) -> Result<(), Failure> {
    let values = family(g, anchor)?;
    need(
        !g.instructions
            .iter()
            .any(|i| i.operation.entities().iter().any(|e| values.contains(e))),
        "returned value is delivered, never dropped or consumed by cleanup",
        anchor,
    )?;
    for &block in &g.blocks {
        let term = g.term(block)?;
        for entity in term.kind.entities() {
            if values.contains(&entity) {
                need(
                    matches!(&term.kind,TerminatorKind::Return {values:actual} if block==returned && matches!(entity,EntityId::Value(v) if actual.as_slice()==[v]))
                        || g.edges
                            .iter()
                            .any(|(from, edge)| *from == block && edge.arguments.contains(&entity)),
                    "actual result only transported or returned at original Return",
                    (entity, term),
                )?;
            }
        }
    }
    Ok(())
}

fn guard<'a>(
    g: &Graph<'a>,
    module: &Module,
    input: &Input,
    sources: &SourceMap,
    parsed: &ParsedFile,
    read: &Instruction,
    uses: &mut Uses,
) -> Result<Option<&'a Instruction>, Failure> {
    let body = parsed.ast().statements().get(input.body).unwrap().span();
    let locals = parsed
        .ast()
        .items()
        .iter()
        .filter_map(|(_, n)| {
            let Item::Variable {
                name: NameMarker::Present(name),
                initializer,
                ..
            } = n.payload()
            else {
                return None;
            };
            (n.span().start() > body.start()
                && n.span().end() < body.end()
                && sources.slice(*name) == Ok("local"))
            .then_some((*name, *initializer))
        })
        .collect::<Vec<_>>();
    let allocations = g
        .instructions
        .iter()
        .copied()
        .filter(|i| matches!(i.operation, Operation::HeapAllocate { .. }))
        .collect::<Vec<_>>();
    if locals.is_empty() {
        need(
            allocations.is_empty(),
            "no unrequested Return-local owner",
            &allocations,
        )?;
        return Ok(None);
    }
    let [(name, init)] = locals.as_slice() else {
        return Err(failure("one original Return Guard", &locals));
    };
    let origin = parsed.ast().expressions().get(*init).unwrap().span();
    need(
        sources.slice(origin) == Ok("Guard()"),
        "original Return Guard constructor",
        origin,
    )?;
    let [allocation] = allocations.as_slice() else {
        return Err(failure("one actual Return Guard allocation", &allocations));
    };
    let Operation::HeapAllocate { owner: ty, payload } = allocation.operation else {
        unreachable!()
    };
    need(
        allocation.origin == Origin::Source(origin)
            && allocation.block == read.block
            && g.position(allocation)? < g.position(read)?,
        "actual Guard construction precedes read",
        allocation,
    )?;
    let class = one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            let Item::Classifier(c) = n.payload() else {
                return None;
            };
            matches!(c.name,NameMarker::Present(name) if sources.slice(name)==Ok("Guard"))
                .then_some(c)
        }),
        "original Guard class",
    )?;
    let member = one(
        class
            .body
            .as_ref()
            .unwrap()
            .members
            .iter()
            .filter_map(|id| parsed.ast().items().get(*id).ok())
            .filter(|n| matches!(n.payload(), Item::Deinit { .. })),
        "original Guard deinit",
    )?;
    need(
        module
            .deinits
            .get(&ty)
            .and_then(|id| module.function(*id))
            .is_some_and(|f| f.origin == Origin::Source(member.span())),
        "actual Return Guard type/deinit identity",
        ty,
    )?;
    let payload = g.definition(EntityId::Value(payload))?;
    need(
        payload.origin == Origin::Source(origin)
            && matches!(&payload.operation,Operation::AggregateConstruct {fields,..} if fields.is_empty()),
        "actual empty Guard payload",
        payload,
    )?;
    let owners = family(g, result(allocation)?)?;
    let drop=one(g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::Drop {owner} if owners.contains(&EntityId::Value(owner)))),"unique current Return Guard Drop")?;
    need(
        drop.origin == Origin::Source(*name),
        "Guard Drop original binding",
        drop,
    )?;
    for &owner in &owners {
        uses.protect(owner);
    }
    if let Operation::Drop { owner } = drop.operation {
        uses.permit(EntityId::Value(owner), drop);
    }
    Ok(Some(drop))
}

fn normal_body(
    g: &Graph<'_>,
    entry: BlockId,
    source_end: &Instruction,
    value_end: &Instruction,
    guard: Option<&Instruction>,
    evaluated: &BTreeSet<InstructionId>,
) -> Result<(), Failure> {
    let mut pending = vec![(entry, BTreeSet::new(), 0u8, false)];
    let mut seen = BTreeSet::new();
    while let Some((block, mut completed, mut guards, mut ended)) = pending.pop() {
        if !seen.insert((block, completed.clone(), guards, ended)) {
            continue;
        }
        for i in g.ops(block)? {
            if evaluated.contains(&i.id) {
                need(
                    !ended && guards == 0,
                    "Return operand/prefix evaluation precedes cleanup",
                    i,
                )?;
                completed.insert(i.id);
            }
            if matches!(
                i.operation,
                Operation::Drop { .. } | Operation::BorrowEnd { .. }
            ) {
                need(
                    completed == *evaluated,
                    "all actual Return evaluation completed before first cleanup",
                    i,
                )?;
            }
            if guard.is_some_and(|d| d.id == i.id) {
                guards += 1;
                need(
                    guards == 1 && !ended,
                    "body Guard dropped once before source End",
                    i,
                )?;
            }
            if i.id == value_end.id {
                need(
                    guards == u8::from(guard.is_some()),
                    "body Guard Drop precedes component End",
                    i,
                )?;
            }
            if i.id == source_end.id {
                ended = true;
            }
        }
        if matches!(g.term(block)?.kind, TerminatorKind::Return { .. }) {
            need(
                ended && guards == u8::from(guard.is_some()) && completed == *evaluated,
                "every successful body Return executes operand and complete cleanup",
                g.term(block)?,
            )?;
        }
        for (_, edge) in g.edges.iter().filter(|(from, _)| *from == block) {
            pending.push((edge.target, completed.clone(), guards, ended));
        }
    }
    Ok(())
}

// The false edge ends its provider before continuation; a later explicit return
// still evaluates its own operand before consuming the retained source owner.
fn outer_order(
    g: &Graph<'_>,
    entry: BlockId,
    evaluated: &BTreeSet<InstructionId>,
    map_type: SsaTypeId,
    input: &Input,
) -> Result<(), Failure> {
    let mut pending = vec![(entry, BTreeSet::new())];
    let mut seen = BTreeSet::new();
    while let Some((block, mut completed)) = pending.pop() {
        if !seen.insert((block, completed.clone())) {
            continue;
        }
        for i in g.ops(block)? {
            if evaluated.contains(&i.id) {
                completed.insert(i.id);
            }
            if matches!(i.operation,Operation::Drop {owner} if g.function.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()==map_type)
            {
                need(
                    input.owned && completed == *evaluated,
                    "exhaustion return operand evaluated before exact source Map Drop",
                    i,
                )?;
            }
        }
        if matches!(g.term(block)?.kind, TerminatorKind::Return { .. }) {
            need(
                completed == *evaluated,
                "outer original operand actually evaluated on every exhaustion Return",
                g.term(block)?,
            )?;
        }
        for (_, edge) in g.edges.iter().filter(|(from, _)| *from == block) {
            pending.push((edge.target, completed.clone()));
        }
    }
    Ok(())
}
