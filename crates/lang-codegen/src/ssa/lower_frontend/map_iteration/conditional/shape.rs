//! Bind capability to the actual For/body/source, never a number or spelling of arms.
use super::*;
use lang_frontend::type_checking::MapIterationDescriptor;

pub(in super::super) fn shape(
    parsed: &ParsedFile,
    _names: &NameResolution,
    _typed: &TypedFile,
    descriptor: MapIterationDescriptor,
    body: StatementId,
    source: SourceIdentity,
    span: Span,
) -> Result<Option<Shape>, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let node = parsed
        .ast()
        .statements()
        .get(descriptor.statement())
        .map_err(|_| missing())?;
    if !matches!(node.payload(), Statement::For { source, body: actual, .. }
        if *source == descriptor.source() && *actual == body)
    {
        return Err(missing());
    }
    let body_span = parsed
        .ast()
        .statements()
        .get(body)
        .map_err(|_| missing())?
        .span();
    let inside = |outer: Span, inner: Span| {
        outer.source_id() == inner.source_id()
            && outer.start() <= inner.start()
            && inner.end() <= outer.end()
    };
    let has_control = parsed.ast().expressions().iter().any(|(_, candidate)| {
        inside(body_span, candidate.span())
            && matches!(
                candidate.payload(),
                Expression::If { .. } | Expression::When { .. }
            )
            && !parsed.ast().expressions().iter().any(|(_, owner)| {
                matches!(owner.payload(), Expression::Lambda { .. })
                    && inside(body_span, owner.span())
                    && inside(owner.span(), candidate.span())
            })
            && !parsed.ast().statements().iter().any(|(id, owner)| {
                id != descriptor.statement()
                    && matches!(owner.payload(), Statement::For { .. })
                    && inside(body_span, owner.span())
                    && inside(owner.span(), candidate.span())
            })
    });
    let has_provider = parsed.ast().statements().iter().any(|(id, candidate)| {
        id != descriptor.statement()
            && inside(body_span, candidate.span())
            && matches!(candidate.payload(), Statement::For { .. })
    });
    let has_parent = parsed.ast().statements().iter().any(|(id, parent)| {
        let Statement::For { body, .. } = parent.payload() else {
            return false;
        };
        id != descriptor.statement()
            && parsed
                .ast()
                .statements()
                .get(*body)
                .is_ok_and(|body| inside(body.span(), node.span()))
    });
    Ok(
        (has_control || has_provider || has_parent).then_some(Shape {
            statement: descriptor.statement(),
            body,
            source,
        }),
    )
}
