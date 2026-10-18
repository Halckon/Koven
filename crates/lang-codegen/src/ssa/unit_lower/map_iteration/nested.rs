//! A closed two-provider topology, qualified independently of descriptor enumeration order.
mod operand;
mod tail;
use super::*;
pub(super) use tail::{guard as qualify_guard, validate_cleanup as validate_tail_cleanup};

pub(super) fn pair(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    statement: UnitStatementId,
) -> Result<Option<QualifiedNested>, LoweringError> {
    let node = parsed
        .ast()
        .statements()
        .get(statement.statement())
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
    let span = node.span();
    let missing = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let mut enclosing = Vec::new();
    let mut descendants = Vec::new();
    for (id, candidate) in parsed.ast().statements().iter() {
        if id == statement.statement() {
            continue;
        }
        if matches!(
            candidate.payload(),
            Statement::For { .. } | Statement::While { .. } | Statement::Loop { .. }
        ) {
            if contains(candidate.span(), span) {
                enclosing.push(id);
            }
            if contains(span, candidate.span()) {
                descendants.push(id);
            }
        }
    }
    if enclosing.is_empty() && descendants.is_empty() {
        return Ok(None);
    }
    let outer = match enclosing.as_slice() {
        [] => statement.statement(),
        [outer] if descendants.is_empty() => *outer,
        _ => return Err(unsupported()),
    };
    let outer_node = parsed
        .ast()
        .statements()
        .get(outer)
        .map_err(|_| missing())?;
    let Statement::For { body, .. } = outer_node.payload() else {
        return Err(unsupported());
    };
    let Statement::Block { elements } = parsed
        .ast()
        .statements()
        .get(*body)
        .map_err(|_| missing())?
        .payload()
    else {
        return Err(unsupported());
    };
    let mut inner = None;
    for child in elements {
        let child_node = parsed
            .ast()
            .statements()
            .get(*child)
            .map_err(|_| missing())?;
        if let Statement::For { .. } = child_node.payload()
            && inner.replace(*child).is_some()
        {
            return Err(unsupported());
        }
    }
    let inner = inner.ok_or_else(unsupported)?;
    if statement.statement() != outer && statement.statement() != inner {
        return Err(unsupported());
    }
    // No enclosing loop/control expression is part of this finite capability.
    reject_nested(parsed, outer, outer_node.span()).map_err(|error| LoweringError {
        kind: error.kind,
        span: Some(span),
    })?;
    // Unacquired/dead-tail source-valid loops intentionally have no ownership plan.
    for member in [outer, inner] {
        let member = UnitStatementId::new(statement.source_unit(), member);
        if typed.map_iteration(member).is_none() || owned.map_iteration(member).is_none() {
            return Err(unsupported());
        }
    }
    let tail = tail::shape(
        parsed,
        names,
        typed,
        owned,
        [
            UnitStatementId::new(statement.source_unit(), outer),
            UnitStatementId::new(statement.source_unit(), inner),
        ],
        span,
    )?;
    if tail.is_none() {
        for child in elements {
            let node = parsed
                .ast()
                .statements()
                .get(*child)
                .map_err(|_| missing())?;
            if let Statement::For { body, .. } = node.payload() {
                straight_body(parsed, *body, span)?;
            } else {
                straight_body(parsed, *child, span)?;
            }
        }
    }
    Ok(Some(QualifiedNested {
        outer: UnitStatementId::new(statement.source_unit(), outer),
        inner: UnitStatementId::new(statement.source_unit(), inner),
        tail,
    }))
}

fn straight_body(
    parsed: &ParsedFile,
    statement: StatementId,
    span: Span,
) -> Result<(), LoweringError> {
    let node = parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
    match node.payload() {
        Statement::Block { elements } => {
            for child in elements {
                straight_body(parsed, *child, span)?;
            }
            Ok(())
        }
        Statement::Expression { expression } => straight_expression(parsed, *expression, span),
        _ => Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
    }
}

pub(super) fn qualify(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    descriptor: &UnitMapIterationDescriptor,
    pair: QualifiedNested,
) -> Result<QualifiedMap, LoweringError> {
    let span = parsed
        .ast()
        .statements()
        .get(descriptor.statement().statement())
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?
        .span();
    let missing = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let outer = typed.map_iteration(pair.outer).ok_or_else(unsupported)?;
    let inner = typed.map_iteration(pair.inner).ok_or_else(unsupported)?;
    reject_aborting_body(parsed, typed, outer, span)?;
    let qualify_member = |member| {
        qualify_one(parsed, names, typed, owned, member, Some(pair)).map_err(|error| {
            if error.kind == LoweringErrorKind::UnsupportedNode {
                unsupported()
            } else {
                error
            }
        })
    };
    let outer_qualified = qualify_member(outer)?;
    let inner_qualified = qualify_member(inner)?;
    if outer.kind() != inner.kind()
        || outer.source_type() != inner.source_type()
        || outer.key_type() != inner.key_type()
        || outer.value_type() != inner.value_type()
        || outer_qualified.source != inner_qualified.source
    {
        return Err(unsupported());
    }
    if outer.source() == inner.source()
        || outer
            .bindings()
            .iter()
            .any(|binding| inner.bindings().contains(binding))
    {
        return Err(missing());
    }
    for (descriptor, qualified) in [(outer, outer_qualified), (inner, inner_qualified)] {
        let plan = owned
            .map_iteration(descriptor.statement())
            .ok_or_else(missing)?;
        if plan.source_access() != UnitIterationSourceAccess::Shared
            || qualified.source_drop.is_some()
            || qualified.transfer_drop.is_some()
            || qualified.conditional.is_some()
            || (pair.tail.is_none() && qualified.body_exit != UnitIterationExitKind::Fallthrough)
        {
            return Err(unsupported());
        }
        if pair.tail.is_some() {
            continue;
        }
        let actions = owned
            .iteration_cleanup_at(qualified.body_point)
            .ok_or_else(missing)?;
        // Body-local owners are outside this capability, before checking its exact actions.
        if actions
            .iter()
            .any(|action| matches!(action, Action::Drop(_)))
        {
            return Err(unsupported());
        }
        if actions
            != [
                Action::EndBinding {
                    statement: descriptor.statement(),
                    symbol: descriptor.value_binding(),
                },
                Action::EndBinding {
                    statement: descriptor.statement(),
                    symbol: descriptor.key_binding(),
                },
            ]
        {
            return Err(missing());
        }
    }
    if descriptor.statement() == pair.outer {
        Ok(outer_qualified)
    } else if descriptor.statement() == pair.inner {
        Ok(inner_qualified)
    } else {
        Err(missing())
    }
}

/// Source-valid aborting calls are outside nested normal before its Fallthrough facts are read.
fn reject_aborting_body(
    parsed: &ParsedFile,
    typed: &CompilationUnitTypes,
    descriptor: &UnitMapIterationDescriptor,
    span: Span,
) -> Result<(), LoweringError> {
    let missing = || lowering_error(LoweringErrorKind::MissingFact, span);
    let node = parsed
        .ast()
        .statements()
        .get(descriptor.statement().statement())
        .map_err(|_| missing())?;
    let Statement::For { body, .. } = node.payload() else {
        return Err(missing());
    };
    let body_span = parsed
        .ast()
        .statements()
        .get(*body)
        .map_err(|_| missing())?
        .span();
    for (expression, node) in parsed.ast().expressions().iter() {
        if contains(body_span, node.span()) && matches!(node.payload(), Expression::Call { .. }) {
            let expression =
                UnitExpressionId::new(descriptor.statement().source_unit(), expression);
            let ty = typed.expression_type(expression).ok_or_else(missing)?;
            // Constructors have dedicated typed facts and need not publish an ordinary call.
            if builtin_type(typed, ty) == Some(BuiltinType::Nothing)
                || typed.call(expression).is_some_and(|call| {
                    call.aborts()
                        || builtin_type(typed, call.return_type()) == Some(BuiltinType::Nothing)
                })
            {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
        }
    }
    Ok(())
}
