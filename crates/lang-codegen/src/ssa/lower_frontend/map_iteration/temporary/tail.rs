//! One lexical Resource local and the real ordinary Int return operand.
use super::*;
use lang_frontend::{name_resolution::Namespace, ownership_checking::ClosureCaptureSource};

pub(super) fn guard(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    body: StatementId,
    kind: IterationExitKind,
    span: Span,
) -> Result<nested::tail::Local, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let body_node = parsed.ast().statements().get(body).map_err(|_| missing())?;
    let Statement::Block { elements } = body_node.payload() else {
        return Err(unsupported());
    };
    let Some((_, prefix)) = elements.split_last() else {
        return Err(unsupported());
    };
    let Some((&first, rest)) = prefix.split_first() else {
        return Err(unsupported());
    };
    let guard = nested::tail::qualify_guard(parsed, names, typed, owned, first, span)?;
    for statement in rest {
        let node = parsed
            .ast()
            .statements()
            .get(*statement)
            .map_err(|_| missing())?;
        if !matches!(node.payload(), Statement::Expression { .. }) {
            return Err(unsupported());
        }
    }
    // Formation is historical; only the exact transfer Drop below proves current cleanup.
    // This local is a lexical sentinel, with no aliases, calls, moves or captures.
    if names
        .references()
        .iter()
        .any(|reference| reference.target() == &ReferenceTarget::Symbol(guard.symbol))
        || owned
            .captures()
            .iter()
            .any(|capture| capture.source() == ClosureCaptureSource::Symbol(guard.symbol))
    {
        return Err(unsupported());
    }
    for (expression, node) in parsed.ast().expressions().iter() {
        if contains(body_node.span(), node.span())
            && expression != guard.initializer
            && matches!(node.payload(), Expression::Call { .. })
        {
            let ty = typed.expression_type(expression).ok_or_else(missing)?;
            if typed.is_resource_type(ty).ok_or_else(missing)? {
                return Err(unsupported());
            }
        }
    }
    if let IterationExitKind::Return(expression) = kind {
        return_identity(parsed, names, typed, expression, span)?;
    }
    Ok(guard)
}

fn return_identity(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    expression: ExpressionId,
    span: Span,
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let node = parsed
        .ast()
        .expressions()
        .get(expression)
        .map_err(|_| missing())?;
    let Expression::Return {
        value: Some(operand),
        ..
    } = node.payload()
    else {
        return Err(missing());
    };
    let node = parsed
        .ast()
        .expressions()
        .get(*operand)
        .map_err(|_| missing())?;
    let Expression::Call { callee, .. } = node.payload() else {
        return Err(missing());
    };
    let callee = parsed
        .ast()
        .expressions()
        .get(*callee)
        .map_err(|_| missing())?;
    let call = typed.call(*operand).ok_or_else(missing)?;
    let CallableTarget::Source(target) = call.target() else {
        return Err(missing());
    };
    let signature = unique(
        typed
            .callables()
            .iter()
            .filter(|item| item.symbol() == target),
    )
    .ok_or_else(missing)?;
    let reference = unique(names.references().iter().filter(|reference| {
        reference.namespace() == Namespace::Value && reference.span() == callee.span()
    }))
    .ok_or_else(missing)?;
    let selected = match reference.target() {
        ReferenceTarget::Symbol(symbol) => *symbol == target,
        ReferenceTarget::OverloadSet(symbols) => symbols.contains(&target),
        _ => false,
    };
    if !selected
        || call.expression() != *operand
        || typed.expression_type(*operand) != Some(call.return_type())
        || typed.expression_category(*operand) != Some(ExpressionCategory::Temporary)
        || signature.return_type() != call.return_type()
        || signature.result_source() != call.result_source()
        || signature.result_source() != CallableResultSource::Owned
    {
        return Err(missing());
    }
    Ok(())
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}
