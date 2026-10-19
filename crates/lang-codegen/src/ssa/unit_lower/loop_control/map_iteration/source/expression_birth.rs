//! The actual ordinary Conditional delivers two witnessed owners to its original result column.
use super::*;

pub(in super::super) fn anchor(
    lowerer: &UnitExpressionLowerer<'_>,
    expression: UnitExpressionId,
    span: Span,
) -> Result<EntityId, LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let storage = lowerer.typed.expression_type(expression).ok_or_else(bad)?;
    let proof = map_iteration::expression_birth::qualify(
        lowerer.parsed,
        lowerer.names,
        lowerer.typed,
        lowerer.owned,
        expression,
        storage,
        span,
    )?;
    if expression.source_unit() != lowerer.source_unit {
        return Err(bad());
    }
    let origin = lowerer
        .parsed
        .ast()
        .expressions()
        .get(expression.expression())
        .map_err(|_| bad())?
        .span();
    let headers = lowerer
        .function
        .blocks
        .iter()
        .filter(|block| {
            block.terminator.as_ref().is_some_and(|term| {
                term.origin == Origin::Source(origin)
                    && matches!(term.kind, TerminatorKind::Conditional { .. })
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
    }) = header.terminator.as_ref().map(|term| &term.kind)
    else {
        return Err(bad());
    };
    let condition_origin = lowerer
        .parsed
        .ast()
        .expressions()
        .get(proof.condition.expression())
        .map_err(|_| bad())?
        .span();
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
        .is_none_or(|data| {
            data.ty != EntityType::Value(condition_type)
                || data.origin != Origin::Source(condition_origin)
        })
    {
        return Err(bad());
    }

    let mut deliveries = Vec::new();
    for (index, edge) in [when_true, when_false].into_iter().enumerate() {
        let result = super::ordinary_birth(lowerer, proof.results[index], span)?;
        let call = carrier::instruction(lowerer.function, result, span)?;
        let block = lowerer.function.block(edge.target).ok_or_else(bad)?;
        let branch_origin = lowerer
            .parsed
            .ast()
            .statements()
            .get(proof.arms[index])
            .map_err(|_| bad())?
            .span();
        let incoming = transport::incoming(lowerer.function, block.id);
        if call.block != block.id
            || block.origin != Origin::Source(branch_origin)
            || edge.arguments.len() != block.parameters.len()
            || !matches!(incoming.as_slice(), [(from, actual)] if *from == header.id && *actual == edge)
        {
            return Err(bad());
        }
        let Some(term) = &block.terminator else {
            return Err(bad());
        };
        let TerminatorKind::Branch(delivery) = &term.kind else {
            return Err(bad());
        };
        if term.origin != Origin::Source(origin) || delivery.arguments.first() != Some(&result)
            || block.instructions.iter().any(|id| lowerer.function.instruction(*id).is_some_and(|instruction| {
                matches!(instruction.operation, Operation::Drop { owner } if EntityId::Value(owner) == result)
            }))
        { return Err(bad()); }
        deliveries.push((block.id, delivery));
    }
    let [(left, left_edge), (right, right_edge)] = deliveries.as_slice() else {
        return Err(bad());
    };
    if left == right || left_edge.target != right_edge.target {
        return Err(bad());
    }
    let join = lowerer.function.block(left_edge.target).ok_or_else(bad)?;
    let incoming = transport::incoming(lowerer.function, join.id);
    if join.origin != Origin::Source(origin)
        || incoming.len() != 2
        || incoming
            .iter()
            .filter(|(from, edge)| from == left && *edge == *left_edge)
            .count()
            != 1
        || incoming
            .iter()
            .filter(|(from, edge)| from == right && *edge == *right_edge)
            .count()
            != 1
        || incoming
            .iter()
            .any(|(_, edge)| edge.arguments.len() != join.parameters.len())
    {
        return Err(bad());
    }
    let result = transport::parameter(lowerer.function, join.id, 0, span)?;
    let storage = lowerer.iteration_type(storage, span)?;
    if !matches!(result, EntityId::Value(_))
        || lowerer.function.entity(result).is_none_or(|data| {
            data.ty != EntityType::Value(storage) || data.origin != Origin::Source(origin)
        })
    {
        return Err(bad());
    }
    Ok(result)
}
