//! Finite all-incoming transport and the real size-result/control continuation.
use super::*;

pub(super) fn edges(
    evidence: carrier::Evidence<'_>,
    proof: &local::Proof,
) -> Result<(), LoweringError> {
    let function = evidence.function;
    let span = evidence.span;
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let [preheader, header, body, exhausted] = evidence.blocks;
    if evidence.body_exit != IterationExitKind::Fallthrough
        || evidence.conditional.is_some()
        || evidence.nested.is_some()
        || evidence.continuation != exhausted
        || evidence
            .carried
            .slots
            .iter()
            .any(|slot| proof.dead.iter().any(|dead| slot.symbol == Some(dead.root)))
    {
        return Err(missing());
    }
    let owner_column = unique(
        evidence
            .carried
            .slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| (slot.symbol == Some(proof.root)).then_some(index)),
    )
    .ok_or_else(missing)?;
    let parameters = |block| -> Result<&[EntityId], LoweringError> {
        function
            .block(block)
            .map(|block| block.parameters.as_slice())
            .ok_or_else(missing)
    };
    for (target, mut expected) in [
        (header, vec![preheader, body]),
        (body, vec![header]),
        (exhausted, vec![header]),
    ] {
        let incoming = nested::incoming(function, target);
        let mut actual = incoming.iter().map(|(block, _)| *block).collect::<Vec<_>>();
        actual.sort();
        expected.sort();
        if actual != expected {
            return Err(missing());
        }
        for (from, edge) in incoming {
            let target_parameters = parameters(target)?;
            if edge.arguments.len() != target_parameters.len() {
                return Err(missing());
            }
            for column in [owner_column, evidence.source_slot, evidence.cursor_slot] {
                let expected = if from == preheader {
                    [
                        evidence.seed,
                        EntityId::Loan(evidence.source),
                        EntityId::Value(evidence.first),
                    ][[owner_column, evidence.source_slot, evidence.cursor_slot]
                        .iter()
                        .position(|actual| *actual == column)
                        .ok_or_else(missing)?]
                } else if from == body && column == evidence.cursor_slot {
                    EntityId::Value(evidence.next.ok_or_else(missing)?)
                } else {
                    *parameters(from)?.get(column).ok_or_else(missing)?
                };
                let actual = *target_parameters.get(column).ok_or_else(missing)?;
                if edge.arguments.get(column) != Some(&expected)
                    || function.entity(actual).map(|data| data.definition)
                        != Some(Definition::BlockParameter {
                            block: target,
                            index: column,
                        })
                    || function.entity(actual).map(|data| data.ty)
                        != function.entity(expected).map(|data| data.ty)
                {
                    return Err(missing());
                }
            }
        }
        nested::transport(
            function,
            *parameters(target)?.get(owner_column).ok_or_else(missing)?,
            evidence.seed,
            span,
        )?;
        nested::transport(
            function,
            *parameters(target)?
                .get(evidence.source_slot)
                .ok_or_else(missing)?,
            EntityId::Loan(evidence.source),
            span,
        )?;
    }
    Ok(())
}

/// Tie the two projected loans to the actual qualified source call before either
/// component is ended; the cursor advances only after the ordered End pair.
pub(super) fn consumer(
    lowerer: &ExpressionLowerer<'_>,
    evidence: carrier::Evidence<'_>,
    proof: &local::Proof,
) -> Result<(), LoweringError> {
    let span = evidence.span;
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let call = lowerer.typed.call(proof.consumer).ok_or_else(missing)?;
    let CallableTarget::Source(callee) = call.target() else {
        return Err(missing());
    };
    let callee = *lowerer
        .function_ids
        .get(&FunctionInstanceKey::new(callee, Vec::new()))
        .ok_or_else(missing)?;
    let body = evidence.blocks[2];
    let instructions = carrier::instructions(evidence.function, body).ok_or_else(missing)?;
    let (call_position, actual) =
        unique(instructions.iter().enumerate().filter(|(_, instruction)| {
            matches!(instruction.operation, Operation::DirectCall { .. })
        }))
        .ok_or_else(missing)?;
    if actual.origin != Origin::Source(lowerer.expression_span(proof.consumer)?)
        || actual.operation
            != (Operation::DirectCall {
                callee,
                receiver: None,
                arguments: vec![EntityId::Loan(evidence.key), EntityId::Loan(evidence.value)],
            })
    {
        return Err(missing());
    }
    let position = |operation| {
        unique(
            instructions
                .iter()
                .enumerate()
                .filter_map(|(position, instruction)| {
                    (instruction.operation == operation).then_some(position)
                }),
        )
        .ok_or_else(missing)
    };
    let parameters = &evidence
        .function
        .block(body)
        .ok_or_else(missing)?
        .parameters;
    let (source, cursor) = roles(parameters, evidence.source_slot, evidence.cursor_slot, span)?;
    let mut counts = [0; 3];
    for instruction in &instructions {
        match instruction.operation {
            Operation::MapIterationKeyLoan { .. } => counts[0] += 1,
            Operation::MapIterationValueLoan { .. } => counts[1] += 1,
            Operation::MapIterationNext { .. } => counts[2] += 1,
            _ => {}
        }
    }
    if counts != [1, 1, 1] {
        return Err(missing());
    }
    let key = position(Operation::MapIterationKeyLoan { source, cursor })?;
    let value = position(Operation::MapIterationValueLoan { source, cursor })?;
    let value_end = position(Operation::BorrowEnd {
        loan: evidence.value,
    })?;
    let key_end = position(Operation::BorrowEnd { loan: evidence.key })?;
    let next = position(Operation::MapIterationNext { source, cursor })?;
    if key >= call_position
        || value >= call_position
        || call_position >= value_end
        || value_end >= key_end
        || key_end >= next
        || instructions
            .iter()
            .any(|instruction| instruction.operation == Operation::BorrowEnd { loan: source })
        || lowerer
            .borrow_bindings
            .values()
            .any(|loan| *loan == evidence.key || *loan == evidence.value)
    {
        return Err(missing());
    }
    Ok(())
}

pub(super) fn path(
    lowerer: &ExpressionLowerer<'_>,
    path: local::Path,
    span: Span,
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let previous = unique(lowerer.map_iteration_state.locals.values().filter(|state| {
        state.statement == path.previous && state.proof.late_size == Some(path.size)
    }))
    .ok_or_else(missing)?;
    validate_consumption(lowerer.function, previous, span)?;
    let size = previous.size.ok_or_else(missing)?;
    let (control, condition, success, failure) = size_control(lowerer, path.size, size, span)?;
    if control != path.control
        || condition != path.condition
        || success != path.success
        || failure != path.failure
        || path.branch != 0
    {
        return Err(missing());
    }
    let (block, _, when_true, when_false) = control_edges(lowerer, control, condition, size, span)?;
    if when_true.target != lowerer.block
        || !matches!(
            carrier::terminator(lowerer.function, when_false.target),
            Some(TerminatorKind::Abort)
        )
    {
        return Err(missing());
    }
    let incoming = nested::incoming(lowerer.function, lowerer.block);
    if !matches!(incoming.as_slice(), [(from, edge)] if *from == block && *edge == when_true) {
        return Err(missing());
    }
    Ok(())
}

/// At function completion every later size result must drive the real comparison
/// and its real success/error arms; a dead MapSize instruction is not a use proof.
pub(super) fn size_use(
    lowerer: &ExpressionLowerer<'_>,
    state: &Carrier,
    size: ValueId,
    span: Span,
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let expression = state.proof.late_size.ok_or_else(missing)?;
    let (control, condition, _, _) = size_control(lowerer, expression, size, span)?;
    let (block, comparison, _, failure) = control_edges(lowerer, control, condition, size, span)?;
    let consumed = state.consumed.ok_or_else(missing)?;
    let comparison =
        carrier::definition(lowerer.function, EntityId::Value(comparison)).ok_or_else(missing)?;
    let read = carrier::definition(lowerer.function, EntityId::Value(size)).ok_or_else(missing)?;
    let instructions =
        carrier::instructions(lowerer.function, consumed.block).ok_or_else(missing)?;
    let read_position = instructions
        .iter()
        .position(|instruction| instruction.id == read.id)
        .ok_or_else(missing)?;
    let drop_position = instructions
        .iter()
        .position(|instruction| instruction.id == consumed.instruction)
        .ok_or_else(missing)?;
    let compare_position = instructions
        .iter()
        .position(|instruction| instruction.id == comparison.id)
        .ok_or_else(missing)?;
    if read.operation
        != (Operation::MapSize {
            owner: EntityId::Value(state.current),
        })
        || read.origin != Origin::Source(lowerer.expression_span(expression)?)
        || read.block != consumed.block
        || block != consumed.block
        || drop_position != read_position + 1
        || drop_position >= compare_position
        || !matches!(
            carrier::terminator(lowerer.function, failure.target),
            Some(TerminatorKind::Abort)
        )
    {
        return Err(missing());
    }
    Ok(())
}

fn size_control(
    lowerer: &ExpressionLowerer<'_>,
    size_expression: ExpressionId,
    size: ValueId,
    span: Span,
) -> Result<(ExpressionId, ExpressionId, StatementId, StatementId), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let (condition, _) = unique(lowerer.parsed.ast().expressions().iter().filter(|(_, node)| {
        matches!(node.payload(), Expression::Binary { left, operator: AstBinaryOperator::Equal, .. }
            if *left == size_expression)
    })).ok_or_else(missing)?;
    let (control, node) = unique(lowerer.parsed.ast().expressions().iter().filter(|(_, node)| {
        matches!(node.payload(), Expression::If { condition: actual, .. } if *actual == condition)
    })).ok_or_else(missing)?;
    let Expression::If {
        then_branch,
        else_branch: Some(failure),
        ..
    } = node.payload()
    else {
        return Err(missing());
    };
    let read = carrier::definition(lowerer.function, EntityId::Value(size)).ok_or_else(missing)?;
    if read.origin != Origin::Source(lowerer.expression_span(size_expression)?) {
        return Err(missing());
    }
    Ok((control, condition, *then_branch, *failure))
}

fn control_edges<'a>(
    lowerer: &'a ExpressionLowerer<'_>,
    control: ExpressionId,
    condition: ExpressionId,
    size: ValueId,
    span: Span,
) -> Result<(BlockId, ValueId, &'a Edge, &'a Edge), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let control_node = lowerer
        .parsed
        .ast()
        .expressions()
        .get(control)
        .map_err(|_| missing())?;
    let Expression::If {
        then_branch,
        else_branch: Some(else_branch),
        ..
    } = control_node.payload()
    else {
        return Err(missing());
    };
    let origin = Origin::Source(control_node.span());
    let block = unique(lowerer.function.blocks.iter().filter(|block| {
        block
            .terminator
            .as_ref()
            .is_some_and(|term| term.origin == origin)
    }))
    .ok_or_else(missing)?;
    let Some(TerminatorKind::Conditional {
        condition: result,
        when_true,
        when_false,
    }) = carrier::terminator(lowerer.function, block.id)
    else {
        return Err(missing());
    };
    let statement_origin = |statement| {
        lowerer
            .parsed
            .ast()
            .statements()
            .get(statement)
            .map(|node| Origin::Source(node.span()))
            .map_err(|_| missing())
    };
    if lowerer
        .function
        .block(when_true.target)
        .map(|block| &block.origin)
        != Some(&statement_origin(*then_branch)?)
        || lowerer
            .function
            .block(when_false.target)
            .map(|block| &block.origin)
            != Some(&statement_origin(*else_branch)?)
    {
        return Err(missing());
    }
    let comparison =
        carrier::definition(lowerer.function, EntityId::Value(*result)).ok_or_else(missing)?;
    let condition_node = lowerer
        .parsed
        .ast()
        .expressions()
        .get(condition)
        .map_err(|_| missing())?;
    let Expression::Binary {
        right,
        operator: AstBinaryOperator::Equal,
        ..
    } = condition_node.payload()
    else {
        return Err(missing());
    };
    let Operation::Compare {
        operator: ComparisonOperator::Equal,
        left,
        right: rhs,
    } = comparison.operation
    else {
        return Err(missing());
    };
    let constant =
        carrier::definition(lowerer.function, EntityId::Value(rhs)).ok_or_else(missing)?;
    if comparison.block != block.id
        || left != size
        || comparison.origin != Origin::Source(condition_node.span())
        || constant.block != block.id
        || constant.origin != Origin::Source(lowerer.expression_span(*right)?)
        || !matches!(constant.operation, Operation::Constant(_))
    {
        return Err(missing());
    }
    Ok((block.id, *result, when_true, when_false))
}
