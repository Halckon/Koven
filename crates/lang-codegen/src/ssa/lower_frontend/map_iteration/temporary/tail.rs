//! One lexical Resource local; ordinary lowering owns return operand delivery.
use super::*;
use lang_frontend::ownership_checking::ClosureCaptureSource;

pub(super) fn guard(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    body: StatementId,
    _kind: IterationExitKind,
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
    Ok(guard)
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}
