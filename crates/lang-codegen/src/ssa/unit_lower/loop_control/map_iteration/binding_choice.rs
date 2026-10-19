//! Named binding joins are proven from real branch operands, independently of If result columns.
use super::*;
use crate::ssa::model::Definition;
use map_iteration::binding_choice::Proof;

fn origin(
    lowerer: &UnitExpressionLowerer<'_>,
    expression: UnitExpressionId,
    span: Span,
) -> Result<Span, LoweringError> {
    if expression.source_unit() != lowerer.source_unit {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    }
    lowerer
        .parsed
        .ast()
        .expressions()
        .get(expression.expression())
        .map(|n| n.span())
        .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))
}
fn branches(
    lowerer: &UnitExpressionLowerer<'_>,
    proof: Proof,
    span: Span,
) -> Result<(BlockId, BlockId), LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let at = origin(lowerer, proof.control, span)?;
    let headers = lowerer
        .function
        .blocks
        .iter()
        .filter(|b| {
            b.terminator.as_ref().is_some_and(|t| {
                t.origin == Origin::Source(at)
                    && matches!(t.kind, TerminatorKind::Conditional { .. })
            })
        })
        .collect::<Vec<_>>();
    let [header] = headers.as_slice() else {
        return Err(bad());
    };
    let Some(TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    }) = header.terminator.as_ref().map(|t| &t.kind)
    else {
        return Err(bad());
    };
    let condition_origin = origin(lowerer, proof.condition, span)?;
    let condition_type = lowerer.iteration_type(
        lowerer
            .typed
            .expression_type(proof.condition)
            .ok_or_else(bad)?,
        span,
    )?;
    if lowerer
        .function
        .entity(EntityId::Value(*condition))
        .is_none_or(|e| {
            e.ty != EntityType::Value(condition_type)
                || e.origin != Origin::Source(condition_origin)
        })
    {
        return Err(bad());
    }
    let arm = lowerer
        .parsed
        .ast()
        .statements()
        .get(proof.arm)
        .map_err(|_| bad())?
        .span();
    for (edge, expected) in [(when_true, arm), (when_false, at)] {
        let block = lowerer.function.block(edge.target).ok_or_else(bad)?;
        let incoming = transport::incoming(lowerer.function, edge.target);
        if block.origin != Origin::Source(expected)
            || edge.arguments.len() != block.parameters.len()
            || !matches!(incoming.as_slice(), [(from, actual)] if *from == header.id && *actual == edge)
        {
            return Err(bad());
        }
    }
    Ok((when_true.target, when_false.target))
}
fn unaliased(
    lowerer: &UnitExpressionLowerer<'_>,
    proof: Proof,
    current: EntityId,
    anchor: EntityId,
    span: Span,
) -> Result<(), LoweringError> {
    let aliases = |entity: EntityId| entity == current || entity == anchor;
    if lowerer.borrow_bindings.contains_key(&proof.root)
        || lowerer.bindings.iter().any(|(root, value)| {
            *root != proof.root
                && matches!(value, LoweredValue::Value(v) if aliases(EntityId::Value(*v)))
        })
        || lowerer
            .temporaries
            .values()
            .any(|v| aliases(EntityId::Value(*v)))
        || lowerer.pending_operands.iter().copied().any(aliases)
    {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    }
    Ok(())
}
fn replacement_owner(
    lowerer: &UnitExpressionLowerer<'_>,
    proof: Proof,
    current: EntityId,
    span: Span,
) -> Result<(), LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let (yes, _) = branches(lowerer, proof, span)?;
    let old = source::ordinary_birth(lowerer, proof.initializer, span)?;
    let replacement = source::ordinary_birth(lowerer, proof.replacement, span)?;
    if lowerer.block != yes
        || carrier::instruction(lowerer.function, replacement, span)?.block != yes
        || old == replacement
        || !matches!(current, EntityId::Value(_))
    {
        return Err(bad());
    }
    transport::from_anchor(lowerer.function, current, old, span)?;
    unaliased(lowerer, proof, current, old, span)
}
fn joined(
    lowerer: &UnitExpressionLowerer<'_>,
    proof: Proof,
    span: Span,
) -> Result<EntityId, LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let (yes, no) = branches(lowerer, proof, span)?;
    let old = source::ordinary_birth(lowerer, proof.initializer, span)?;
    let replacement = source::ordinary_birth(lowerer, proof.replacement, span)?;
    let call = carrier::instruction(lowerer.function, replacement, span)?;
    if call.block != yes || old == replacement {
        return Err(bad());
    }
    let at = origin(lowerer, proof.control, span)?;
    let branch = |id| {
        let block = lowerer.function.block(id).ok_or_else(bad)?;
        let term = block.terminator.as_ref().ok_or_else(bad)?;
        match &term.kind {
            TerminatorKind::Branch(edge) if term.origin == Origin::Source(at) => Ok(edge),
            _ => Err(bad()),
        }
    };
    let yes_edge = branch(yes)?;
    let no_edge = branch(no)?;
    if yes_edge.target != no_edge.target {
        return Err(bad());
    }
    let columns = yes_edge
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(index, &v)| (v == replacement).then_some(index))
        .collect::<Vec<_>>();
    let [column] = columns.as_slice() else {
        return Err(bad());
    };
    let original = *no_edge.arguments.get(*column).ok_or_else(bad)?;
    transport::from_anchor(lowerer.function, original, old, span)?;
    let join = lowerer.function.block(yes_edge.target).ok_or_else(bad)?;
    let incoming = transport::incoming(lowerer.function, join.id);
    if join.origin != Origin::Source(at)
        || incoming.len() != 2
        || incoming
            .iter()
            .filter(|(from, edge)| *from == yes && *edge == yes_edge)
            .count()
            != 1
        || incoming
            .iter()
            .filter(|(from, edge)| *from == no && *edge == no_edge)
            .count()
            != 1
        || incoming
            .iter()
            .any(|(_, edge)| edge.arguments.len() != join.parameters.len())
    {
        return Err(bad());
    }
    let result = transport::parameter(lowerer.function, join.id, *column, span)?;
    let ty = lowerer.function.entity(old).ok_or_else(bad)?.ty;
    if !matches!(result, EntityId::Value(_))
        || lowerer
            .function
            .entity(result)
            .map(|e| (e.ty, e.definition))
            != Some((
                ty,
                Definition::BlockParameter {
                    block: join.id,
                    index: *column,
                },
            ))
        || lowerer.function.entity(replacement).map(|e| e.ty) != Some(ty)
    {
        return Err(bad());
    }
    let drops = lowerer
        .function
        .instructions
        .iter()
        .filter(|i| {
            i.block == yes
                && i.origin == Origin::Source(proof.marker)
                && matches!(i.operation, Operation::Drop { .. })
        })
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        return Err(bad());
    };
    let Operation::Drop { owner } = drop.operation else {
        return Err(bad());
    };
    transport::from_anchor(lowerer.function, EntityId::Value(owner), old, span)?;
    let instructions = &lowerer.function.block(yes).ok_or_else(bad)?.instructions;
    if instructions
        .iter()
        .position(|id| *id == call.id)
        .zip(instructions.iter().position(|id| *id == drop.id))
        .is_none_or(|(call, drop)| call >= drop)
        || lowerer.function.instructions.iter().any(|i| {
            i.block == no
                && i.origin == Origin::Source(proof.marker)
                && matches!(i.operation, Operation::Drop { .. })
        })
    {
        return Err(bad());
    }
    Ok(result)
}
pub(super) fn anchor(
    lowerer: &UnitExpressionLowerer<'_>,
    proof: Proof,
    current: EntityId,
    span: Span,
) -> Result<EntityId, LoweringError> {
    let anchor = if proof.before {
        joined(lowerer, proof, span)?
    } else {
        source::ordinary_birth(lowerer, proof.initializer, span)?
    };
    transport::from_anchor(lowerer.function, current, anchor, span)?;
    unaliased(lowerer, proof, current, anchor, span)?;
    Ok(anchor)
}
pub(super) fn validate_drop(
    lowerer: &UnitExpressionLowerer<'_>,
    proof: Proof,
    fact: lang_frontend::ownership_checking::UnitDropFact,
    owner: ValueId,
    span: Span,
) -> Result<(), LoweringError> {
    let [replaced, final_drop] = proof.drops(lowerer.owned, span)?;
    if fact == replaced {
        return replacement_owner(lowerer, proof, EntityId::Value(owner), span);
    }
    if fact != final_drop {
        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
    }
    let anchor = joined(lowerer, proof, span)?;
    transport::from_anchor(lowerer.function, EntityId::Value(owner), anchor, span)?;
    unaliased(lowerer, proof, EntityId::Value(owner), anchor, span)
}
pub(super) fn finish(
    lowerer: &UnitExpressionLowerer<'_>,
    proof: Proof,
    span: Span,
) -> Result<(), LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let anchor = joined(lowerer, proof, span)?;
    let UnitDropPoint::CallReturn(expression) = proof.drops(lowerer.owned, span)?[1].point() else {
        return Err(bad());
    };
    let at = origin(lowerer, expression, span)?;
    let descriptor = lowerer.typed.call(expression).ok_or_else(bad)?;
    let resolved = lowerer
        .source_plan
        .call_site(lowerer.source_token, expression)
        .ok_or_else(bad)?;
    let callee = *lowerer.function_ids.get(resolved.key()).ok_or_else(bad)?;
    let calls = lowerer
        .function
        .instructions
        .iter()
        .filter(|i| {
            i.origin == Origin::Source(at) && matches!(i.operation, Operation::DirectCall { .. })
        })
        .collect::<Vec<_>>();
    let [call] = calls.as_slice() else {
        return Err(bad());
    };
    let Operation::DirectCall {
        callee: actual,
        receiver: None,
        arguments,
    } = &call.operation
    else {
        return Err(bad());
    };
    let [argument] = arguments.as_slice() else {
        return Err(bad());
    };
    if *actual != callee
        || descriptor.arguments().len() != 1
        || lowerer.bindings.contains_key(&proof.root)
    {
        return Err(bad());
    }
    let loan = carrier::instruction(lowerer.function, *argument, span)?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = loan.operation
    else {
        return Err(bad());
    };
    let root = carrier::instruction(lowerer.function, EntityId::Place(place), span)?;
    let Operation::RootPlace { owner } = root.operation else {
        return Err(bad());
    };
    transport::from_anchor(lowerer.function, EntityId::Value(owner), anchor, span)?;
    let EntityId::Loan(source) = argument else {
        return Err(bad());
    };
    let ends = lowerer
        .function
        .instructions
        .iter()
        .filter(|i| i.operation == (Operation::BorrowEnd { loan: *source }))
        .collect::<Vec<_>>();
    let drops = lowerer
        .function
        .instructions
        .iter()
        .filter(|i| i.operation == (Operation::Drop { owner }))
        .collect::<Vec<_>>();
    let ([end], [drop]) = (ends.as_slice(), drops.as_slice()) else {
        return Err(bad());
    };
    let instructions = &lowerer
        .function
        .block(call.block)
        .ok_or_else(bad)?
        .instructions;
    let position = |id| {
        instructions
            .iter()
            .position(|actual| *actual == id)
            .ok_or_else(bad)
    };
    if [root, loan, end, drop]
        .iter()
        .any(|i| i.block != call.block)
        || end.origin != Origin::Source(at)
        || drop.origin != Origin::Source(proof.marker)
        || !(position(root.id)? < position(loan.id)?
            && position(loan.id)? < position(call.id)?
            && position(call.id)? < position(end.id)?
            && position(end.id)? < position(drop.id)?)
    {
        return Err(bad());
    }
    Ok(())
}
