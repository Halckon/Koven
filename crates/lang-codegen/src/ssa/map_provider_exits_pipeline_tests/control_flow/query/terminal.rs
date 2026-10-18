//! Actual Return operand delivery and exhaustive successful-path cleanup.
use super::*;

fn tail(parsed: &ParsedFile, statement: StatementId) -> Result<ExpressionId, Failure> {
    let node = parsed.ast().statements().get(statement).unwrap();
    match node.payload() {
        Statement::ControlBody { elements } | Statement::Block { elements } => tail(
            parsed,
            *elements
                .last()
                .ok_or_else(|| failure("nonempty return If arm", node))?,
        ),
        Statement::Expression { expression } => Ok(*expression),
        _ => Err(failure("actual return If value arm", node)),
    }
}

/// Complete operand producers, including both real If values and checked-success routing.
pub(super) fn operand(
    g: &Graph<'_>,
    parsed: &ParsedFile,
    sources: &SourceMap,
    id: ExpressionId,
) -> Result<Vec<EntityId>, Failure> {
    let n = parsed.ast().expressions().get(id).unwrap();
    match n.payload() {
        Expression::Group { expression } => return operand(g, parsed, sources, *expression),
        Expression::If {
            condition,
            then_branch,
            else_branch: Some(other),
            ..
        } => {
            return if_operand(
                g,
                parsed,
                sources,
                n.span(),
                *condition,
                [*then_branch, *other],
            );
        }
        _ => {}
    }
    let instruction = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(n.span())
                && matches!(
                    i.operation,
                    Operation::Constant(_)
                        | Operation::StringLiteral { .. }
                        | Operation::StringClone { .. }
                        | Operation::DirectCall { .. }
                        | Operation::CheckedArithmetic { .. }
                        | Operation::Read { .. }
                )
        }),
        "actual full original Return operand producer",
    )?;
    if let Expression::Binary {
        left,
        right,
        operator,
        ..
    } = n.payload()
    {
        need(
            *operator == lang_frontend::parser::BinaryOperator::Add,
            "original checked Return add",
            n,
        )?;
        let lhs = operand(g, parsed, sources, *left)?;
        let rhs = operand(g, parsed, sources, *right)?;
        let Operation::CheckedArithmetic {
            operator: CheckedArithmeticOperator::Add,
            left,
            right,
        } = instruction.operation
        else {
            return Err(failure("actual checked Return operation", instruction));
        };
        g.anchored(EntityId::Value(left), &lhs)?;
        g.anchored(EntityId::Value(right), &rhs)?;
        let [EntityId::Value(value), EntityId::Value(failed)] = instruction.results.as_slice()
        else {
            return Err(failure(
                "checked Return result and failure identities",
                instruction,
            ));
        };
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &g.term(instruction.block)?.kind
        else {
            return Err(failure("actual checked Return success branch", instruction));
        };
        need(
            *condition == *failed
                && matches!(g.term(when_true.target)?.kind, TerminatorKind::Abort)
                && !matches!(g.term(when_false.target)?.kind, TerminatorKind::Abort),
            "checked operand overflow aborts before cleanup and result delivery",
            instruction,
        )?;
        return Ok(vec![EntityId::Value(*value)]);
    }
    match n.payload() {
        Expression::Literal(_) => {
            let expected = sources
                .slice(n.span())
                .unwrap()
                .parse::<i128>()
                .map_err(|e| failure("original Int return literal", e))?;
            need(
                instruction.operation == Operation::Constant(ScalarConstant::Integer(expected)),
                "exact Return literal value",
                instruction,
            )?;
        }
        Expression::Call { .. } => need(
            matches!(
                instruction.operation,
                Operation::DirectCall { .. } | Operation::StringClone { .. }
            ),
            "original full Call result",
            instruction,
        )?,
        Expression::Name => need(
            matches!(instruction.operation, Operation::Read { .. }),
            "original Copyable component read",
            instruction,
        )?,
        Expression::String { .. } => need(
            matches!(instruction.operation, Operation::StringLiteral { .. }),
            "original String operand",
            instruction,
        )?,
        _ => return Err(failure("unproved original Return operand identity", n)),
    }
    Ok(vec![result(instruction)?])
}

/// Preserve branch polarity, original arm ownership and the complete result merge.
fn if_operand(
    g: &Graph<'_>,
    parsed: &ParsedFile,
    sources: &SourceMap,
    span: Span,
    condition_ast: ExpressionId,
    arms: [StatementId; 2],
) -> Result<Vec<EntityId>, Failure> {
    let (condition_block, condition, true_edge, false_edge) = one(
        g.blocks.iter().filter_map(|&block| {
            let term = g.term(block).ok()?;
            let TerminatorKind::Conditional {
                condition,
                when_true,
                when_false,
            } = &term.kind
            else {
                return None;
            };
            (term.origin == Origin::Source(span))
                .then_some((block, *condition, when_true, when_false))
        }),
        "actual returned If conditional and original span",
    )?;
    let condition_node = parsed.ast().expressions().get(condition_ast).unwrap();
    need(
        matches!(condition_node.payload(), Expression::Name),
        "original returned If named Copyable selector",
        condition_node,
    )?;
    let name = sources.slice(condition_node.span()).unwrap();
    let scan = one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            match n.payload() {
                Item::Function { parameters, .. }
                    if g.function.origin == Origin::Source(n.span()) =>
                {
                    Some(parameters)
                }
                _ => None,
            }
        }),
        "returned If actual scanner parameters",
    )?;
    let index = one(
        scan.iter().enumerate().filter_map(|(index, parameter)| {
            matches!(parameter.name, NameMarker::Present(s) if sources.slice(s)==Ok(name))
                .then_some(index)
        }),
        "returned If original selector parameter slot",
    )?;
    let parameter = *g
        .block(g.function.entry_block().unwrap())?
        .parameters
        .get(index)
        .ok_or_else(|| failure("returned If actual selector ABI slot", index))?;
    need(
        matches!(parameter, EntityId::Value(_)),
        "returned If selector is Copyable Value",
        parameter,
    )?;
    g.anchored(EntityId::Value(condition), &[parameter])?;
    let mut arm_results = Vec::new();
    let mut outgoing = Vec::new();
    for (arm, edge) in arms.into_iter().zip([true_edge, false_edge]) {
        let original_span = parsed.ast().statements().get(arm).unwrap().span();
        need(
            g.block(edge.target)?.origin == Origin::Source(original_span),
            "true/false edge targets its own original AST arm",
            (edge, original_span),
        )?;
        g.incoming_from(edge.target, &[condition_block])?;
        let anchor = one(
            operand(g, parsed, sources, tail(parsed, arm)?)?.into_iter(),
            "one full returned If arm value",
        )?;
        let producer = g.definition(anchor)?;
        let finish = if matches!(producer.operation, Operation::CheckedArithmetic { .. }) {
            let TerminatorKind::Conditional { when_false, .. } = &g.term(producer.block)?.kind
            else {
                unreachable!()
            };
            when_false.target
        } else {
            producer.block
        };
        arm_results.push(anchor);
        outgoing.push((edge.target, producer.block, finish, g.branch(finish)?));
    }
    let merge = outgoing[0].3.target;
    need(
        outgoing[1].3.target == merge,
        "both original If arms reach their one actual value merge",
        &outgoing,
    )?;
    g.incoming_from(merge, &[outgoing[0].2, outgoing[1].2])?;
    for (index, &(start, producer, _, _)) in outgoing.iter().enumerate() {
        let nodes = arm_nodes(g, start, merge)?;
        need(
            nodes.contains(&producer) && !nodes.contains(&outgoing[1 - index].1),
            "each branch executes only its original full operand producer",
            (index, nodes),
        )?;
    }
    let result = one(
        g.block(merge)?
            .parameters
            .iter()
            .copied()
            .filter(|&parameter| {
                [0, 1].into_iter().all(|index| {
                    g.argument(outgoing[index].3, parameter)
                        .and_then(|argument| g.anchored(argument, &[arm_results[index]]))
                        .is_ok()
                })
            }),
        "unique actual returned If value column with polarity-specific incoming",
    )?;
    need(
        matches!(result, EntityId::Value(_)),
        "returned If merge delivers a Value",
        result,
    )?;
    Ok(vec![result])
}

fn arm_nodes(g: &Graph<'_>, start: BlockId, merge: BlockId) -> Result<BTreeSet<BlockId>, Failure> {
    let mut pending = vec![start];
    let mut seen = BTreeSet::new();
    while let Some(block) = pending.pop() {
        if block == merge || !seen.insert(block) {
            continue;
        }
        match &g.term(block)?.kind {
            TerminatorKind::Branch(edge) => pending.push(edge.target),
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => {
                pending.extend([when_true.target, when_false.target]);
            }
            TerminatorKind::Abort => {}
            _ => {
                return Err(failure(
                    "If arm must finish at the actual value merge or checked abort",
                    g.term(block)?,
                ));
            }
        }
    }
    Ok(seen)
}

#[derive(Clone, Default)]
pub(super) struct Path {
    pub(super) owner: Option<EntityId>,
    pub(super) header_owner: Option<EntityId>,
    blocks: BTreeSet<BlockId>,
    history: Vec<InstructionId>,
    edges: Vec<Edge>,
    live: BTreeSet<EntityId>,
    ended: Vec<usize>,
    guard_live: bool,
    guard_drops: usize,
    source_ends: usize,
    map_drops: usize,
    next: usize,
    cleanup: Option<usize>,
    root_end_origins: Vec<Origin>,
}

impl Path {
    pub(super) fn at(owner: Option<EntityId>, header_owner: Option<EntityId>) -> Self {
        Self {
            owner,
            header_owner,
            ..Self::default()
        }
    }
}

fn actual_definition(
    g: &Graph<'_>,
    mut entity: EntityId,
    edges: &[Edge],
) -> Result<InstructionId, Failure> {
    let mut seen = BTreeSet::new();
    loop {
        need(
            seen.insert(entity),
            "Return path resolves to real producer without cycle",
            entity,
        )?;
        match g.function.entity(entity).unwrap().definition {
            Definition::InstructionResult { instruction, .. } => return Ok(instruction),
            Definition::BlockParameter { block, .. } => {
                let edge = edges
                    .iter()
                    .rev()
                    .find(|e| e.target == block)
                    .ok_or_else(|| failure("actual Return path incoming", entity))?;
                entity = g.argument(edge, entity)?;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn walk(
    g: &Graph<'_>,
    parsed: &ParsedFile,
    for_span: Span,
    body_span: Span,
    unit: bool,
    header: BlockId,
    block: BlockId,
    components: &BTreeMap<EntityId, (usize, EntityId)>,
    lease: &BTreeSet<EntityId>,
    guards: &BTreeSet<EntityId>,
    map_drops: &BTreeSet<InstructionId>,
    owners: &BTreeSet<EntityId>,
    body: bool,
    mut state: Path,
    reached: &mut BTreeSet<BlockId>,
) -> Result<(), Failure> {
    if block == header {
        return need(
            body && state.next == 1
                && state.source_ends == 0
                && state.map_drops == 0
                && state.owner == state.header_owner,
            "Continue/fallthrough retains source/owner through exact common header",
            (block, state.next, state.source_ends),
        );
    }
    need(
        state.blocks.insert(block),
        "no uninspected cycle besides original provider backedge",
        block,
    )?;
    reached.insert(block);
    for i in g.ops(block)? {
        state.history.push(i.id);
        match i.operation {
            Operation::MapIterationKeyLoan { .. } | Operation::MapIterationValueLoan { .. } => {
                let entity = result(i)?;
                need(
                    body && state.live.insert(components[&entity].1),
                    "component original birth once per iteration",
                    i,
                )?;
            }
            Operation::SharedReborrow { source }
                if components.contains_key(&EntityId::Loan(source)) =>
            {
                let parent = components[&EntityId::Loan(source)].1;
                let current = components[&result(i)?].1;
                need(
                    state.live.contains(&parent) && state.live.insert(current),
                    "derived Borrow has its live original parent",
                    i,
                )?;
            }
            Operation::BorrowEnd { loan } if components.contains_key(&EntityId::Loan(loan)) => {
                let (role, anchor) = components[&EntityId::Loan(loan)];
                need(
                    state.live.remove(&anchor),
                    "each actual component Borrow ends once on its path",
                    i,
                )?;
                let producer = g.definition(anchor)?;
                if matches!(
                    producer.operation,
                    Operation::MapIterationKeyLoan { .. } | Operation::MapIterationValueLoan { .. }
                ) {
                    need(
                        !state.guard_live && state.guard_drops == 1,
                        "Guard Drop precedes binding Ends",
                        i,
                    )?;
                    // K ends only after V. Derived reborrows must already have ended.
                    if role == 0 {
                        need(
                            state.ended.as_slice() == [1],
                            "exact VEnd then KEnd",
                            &state.ended,
                        )?;
                    }
                    need(
                        !state
                            .live
                            .iter()
                            .any(|e| components.get(e).is_some_and(|r| r.0 == role)),
                        "derived component loans end before their binding",
                        i,
                    )?;
                    state.ended.push(role);
                    state.root_end_origins.push(i.origin.clone());
                }
            }
            Operation::HeapAllocate { .. } if guards.contains(&result(i)?) => {
                need(
                    !state.guard_live && state.guard_drops == 0,
                    "one actual body Guard allocation",
                    i,
                )?;
                state.guard_live = true;
            }
            Operation::Drop { owner } if guards.contains(&EntityId::Value(owner)) => {
                need(
                    state.guard_live && state.ended.is_empty(),
                    "live original Guard drops before binding cleanup",
                    i,
                )?;
                state.guard_live = false;
                state.guard_drops += 1;
                state.cleanup.get_or_insert(state.history.len() - 1);
            }
            Operation::BorrowEnd { loan } if lease.contains(&EntityId::Loan(loan)) => {
                need(
                    !body || (state.ended.as_slice() == [1, 0] && state.live.is_empty()),
                    "binding Ends precede terminal source End",
                    i,
                )?;
                if body {
                    let expected = if unit {
                        Origin::Source(for_span)
                    } else {
                        i.origin.clone()
                    };
                    if !unit {
                        need(
                            parsed.ast().expressions().iter().any(|(_, n)| {
                                inside(n.span(), body_span)
                                    && i.origin == Origin::Source(n.span())
                                    && matches!(
                                        n.payload(),
                                        Expression::Break { .. } | Expression::Return { .. }
                                    )
                            }),
                            "source End retains original terminal AST span",
                            i,
                        )?;
                    }
                    need(
                        i.origin == expected
                            && state.root_end_origins.iter().all(|o| *o == expected),
                        "V/K/source terminal Ends share their exact entry contract",
                        i,
                    )?;
                }
                state.source_ends += 1;
                need(
                    state.source_ends == 1 && state.next == 0,
                    "source End once only on terminal/exhaustion",
                    i,
                )?;
            }
            Operation::Drop { owner } if map_drops.contains(&i.id) => {
                need(
                    state.owner == Some(EntityId::Value(owner)),
                    "source Drop uses exact current owner on this actual path",
                    (i, state.owner),
                )?;
                state.owner = None;
                need(
                    state.source_ends == 1 && (!body || state.ended.as_slice() == [1, 0]),
                    "source owner Drop after source/component Ends",
                    i,
                )?;
                state.map_drops += 1;
                need(
                    state.map_drops == 1,
                    "source owner exactly once on each finite path",
                    i,
                )?;
            }
            Operation::MapIterationNext { .. } => {
                need(
                    body && state.guard_drops == 1
                        && !state.guard_live
                        && state.live.is_empty()
                        && state.ended.as_slice() == [1, 0]
                        && state.source_ends == 0
                        && state.map_drops == 0,
                    "Guard/derived End/VEnd/KEnd before Next with source retained",
                    i,
                )?;
                let expected = if unit {
                    Origin::Source(for_span)
                } else {
                    i.origin.clone()
                };
                need(
                    state
                        .root_end_origins
                        .iter()
                        .all(|origin| *origin == expected),
                    "actual entry-specific Continue/fallthrough cleanup spans",
                    &state.root_end_origins,
                )?;
                state.next += 1;
            }
            _ => {}
        }
    }
    let term = g.term(block)?;
    match &term.kind {
        TerminatorKind::Return { values } => {
            need(
                state.next == 0
                    && state.source_ends == 1
                    && state.map_drops == usize::from(!owners.is_empty())
                    && (!body
                        || (state.guard_drops == 1
                            && !state.guard_live
                            && state.live.is_empty()
                            && state.ended.as_slice() == [1, 0])),
                "every finite Return path has complete exact cleanup and source ownership",
                term,
            )?;
            if body {
                let original = parsed.ast().expressions().iter().find_map(|(_, n)| {
                    (term.origin == Origin::Source(n.span())
                        && inside(n.span(), body_span)
                        && matches!(n.payload(), Expression::Return { .. }))
                    .then_some(n.span())
                });
                if let Some(span) = original {
                    let expected = Origin::Source(if unit { for_span } else { span });
                    need(
                        state.root_end_origins.iter().all(|o| *o == expected),
                        "actual entry-specific Return cleanup origin",
                        &state.root_end_origins,
                    )?;
                    for returned in values {
                        let instruction =
                            actual_definition(g, EntityId::Value(*returned), &state.edges)?;
                        let position = state
                            .history
                            .iter()
                            .position(|id| *id == instruction)
                            .ok_or_else(|| {
                                failure("actual Return value evaluated on its path", returned)
                            })?;
                        need(
                            position < state.cleanup.unwrap(),
                            "actual Return operand completes before Guard and source cleanup",
                            (returned, position, state.cleanup),
                        )?;
                    }
                }
            }
        }
        TerminatorKind::Abort => {
            let predecessors = g.incoming(block);
            need(
                !predecessors.is_empty()
                    && predecessors.iter().all(|(from, edge)| {
                        let Ok(term) = g.term(*from) else {
                            return false;
                        };
                        let TerminatorKind::Conditional {
                            condition,
                            when_true,
                            ..
                        } = &term.kind
                        else {
                            return false;
                        };
                        when_true == *edge
                            && g.instructions.iter().any(|i| {
                                matches!(i.operation, Operation::CheckedArithmetic { .. })
                                    && i.results.get(1) == Some(&EntityId::Value(*condition))
                            })
                    }),
                "only actual checked failure edges abort before Return delivery",
                term,
            )?;
        }
        TerminatorKind::Branch(_) | TerminatorKind::Conditional { .. } => {
            for (_, edge) in g.edges.iter().filter(|(from, _)| *from == block) {
                let mut next = state.clone();
                next.edges.push((*edge).clone());
                if let Some(current) = next.owner {
                    if edge.arguments.contains(&current) {
                        next.owner = Some(g.transported(edge, current)?);
                    } else {
                        need(
                            !edge.arguments.iter().any(|e| owners.contains(e)),
                            "edge cannot substitute another historical source owner",
                            edge,
                        )?;
                    }
                }
                walk(
                    g,
                    parsed,
                    for_span,
                    body_span,
                    unit,
                    header,
                    edge.target,
                    components,
                    lease,
                    guards,
                    map_drops,
                    owners,
                    body,
                    next,
                    reached,
                )?;
            }
        }
        _ => return Err(failure("all actual scanner terminators proved", term)),
    }
    Ok(())
}
