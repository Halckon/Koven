//! The finite pair is established independently of descriptor traversal order.
use super::*;
use lang_frontend::type_checking::MapIterationDescriptor;

pub(in super::super) fn pair(
    parsed: &ParsedFile,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    descriptor: MapIterationDescriptor,
) -> Result<Option<Pair>, LoweringError> {
    let node = parsed
        .ast()
        .statements()
        .get(descriptor.statement())
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
    let span = node.span();
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let enclosing = parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(id, node)| {
            (id != descriptor.statement()
                && contains(node.span(), span)
                && matches!(node.payload(), Statement::For { .. }))
            .then_some(id)
        })
        .collect::<Vec<_>>();
    let outer = match enclosing.as_slice() {
        [] => descriptor.statement(),
        [outer] => *outer,
        _ => return Err(unsupported()),
    };
    let outer_node = parsed
        .ast()
        .statements()
        .get(outer)
        .map_err(|_| missing())?;
    let Statement::For {
        body: outer_body, ..
    } = outer_node.payload()
    else {
        return Err(missing());
    };
    let body = parsed
        .ast()
        .statements()
        .get(*outer_body)
        .map_err(|_| missing())?;
    let Statement::Block { elements } = body.payload() else {
        return if enclosing.is_empty() {
            Ok(None)
        } else {
            Err(unsupported())
        };
    };
    let children = elements
        .iter()
        .filter_map(|&id| {
            parsed
                .ast()
                .statements()
                .get(id)
                .ok()
                .and_then(|node| matches!(node.payload(), Statement::For { .. }).then_some(id))
        })
        .collect::<Vec<_>>();
    let inner = match children.as_slice() {
        [] if enclosing.is_empty() => return Ok(None),
        [inner] => *inner,
        _ => return Err(unsupported()),
    };
    if (descriptor.statement() != outer && descriptor.statement() != inner)
        || super::super::qualify::has_enclosing_control(parsed, outer_node.span())
    {
        return Err(unsupported());
    }
    let outer_descriptor = typed.map_iteration(outer).ok_or_else(unsupported)?;
    let inner_descriptor = typed.map_iteration(inner).ok_or_else(unsupported)?;
    let inner_node = parsed
        .ast()
        .statements()
        .get(inner)
        .map_err(|_| missing())?;
    let Statement::For {
        body: inner_body, ..
    } = inner_node.payload()
    else {
        return Err(missing());
    };
    for &element in elements {
        if element != inner && !straight(parsed, typed, element)? {
            return Err(unsupported());
        }
    }
    if !straight(parsed, typed, *inner_body)? {
        return Err(unsupported());
    }
    reject_aborting_body(parsed, typed, *outer_body, span)?;
    let first = owned.map_iteration(outer).ok_or_else(unsupported)?;
    let second = owned.map_iteration(inner).ok_or_else(unsupported)?;
    if first.source_access() != MapIterationSourceAccess::Shared
        || second.source_access() != MapIterationSourceAccess::Shared
        || first.source() != second.source()
        || outer_descriptor.kind() != inner_descriptor.kind()
        || outer_descriptor.source_type() != inner_descriptor.source_type()
        || [
            outer_descriptor.key_type(),
            outer_descriptor.value_type(),
            inner_descriptor.key_type(),
            inner_descriptor.value_type(),
        ]
        .iter()
        .any(|&ty| builtin_type(typed, ty) != Some(BuiltinType::String))
    {
        return Err(unsupported());
    }
    if outer_descriptor.source() == inner_descriptor.source()
        || outer_descriptor
            .bindings()
            .iter()
            .any(|binding| inner_descriptor.bindings().contains(binding))
    {
        return Err(missing());
    }
    Ok(Some(Pair { outer, inner }))
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && outer.end() >= inner.end()
}

fn straight(
    parsed: &ParsedFile,
    typed: &TypedFile,
    statement: StatementId,
) -> Result<bool, LoweringError> {
    let node = parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
    match node.payload() {
        Statement::Block { elements } => {
            for &element in elements {
                if !straight(parsed, typed, element)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Statement::Expression { .. } => {
            super::super::qualify::straight_statement(parsed, typed, statement)
        }
        _ => Ok(false),
    }
}

pub(in super::super) fn cleanup(
    owned: &OwnershipCheckedFile,
    plan: &MapIterationOwnershipPlan,
    body: StatementId,
    span: Span,
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let descriptor = plan.descriptor();
    let statement = descriptor.statement();
    for (kind, point, actions) in [
        (
            IterationExitKind::Fallthrough,
            DropPoint::AfterStatement(body),
            vec![
                IterationCleanupAction::EndBinding {
                    statement,
                    symbol: descriptor.value_binding(),
                },
                IterationCleanupAction::EndBinding {
                    statement,
                    symbol: descriptor.key_binding(),
                },
            ],
        ),
        (
            IterationExitKind::Exhaustion,
            DropPoint::LoopExit(statement),
            vec![
                IterationCleanupAction::FinishProvider(statement),
                IterationCleanupAction::EndSource(statement),
            ],
        ),
    ] {
        let exit =
            unique(plan.exits().iter().filter(|exit| exit.kind() == kind)).ok_or_else(missing)?;
        if exit.point() != point
            || exit.actions() != actions
            || owned
                .cleanup_steps()
                .iter()
                .filter_map(|(p, action)| (*p == point).then_some(*action))
                .collect::<Vec<_>>()
                != actions
        {
            return Err(missing());
        }
    }
    Ok(())
}

/// Abort effect is independent of expression type. Dedicated construction calls
/// need not publish an ordinary CallDescriptor; only inspect it when present.
fn reject_aborting_body(
    parsed: &ParsedFile,
    typed: &TypedFile,
    body: StatementId,
    span: Span,
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let body_span = parsed
        .ast()
        .statements()
        .get(body)
        .map_err(|_| missing())?
        .span();
    for (expression, node) in parsed.ast().expressions().iter() {
        if contains(body_span, node.span()) && matches!(node.payload(), Expression::Call { .. }) {
            let ty = typed.expression_type(expression).ok_or_else(missing)?;
            if builtin_type(typed, ty) == Some(BuiltinType::Nothing)
                || typed.call(expression).is_some_and(|call| {
                    call.aborts()
                        || builtin_type(typed, call.return_type()) == Some(BuiltinType::Nothing)
                })
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
        }
    }
    Ok(())
}
