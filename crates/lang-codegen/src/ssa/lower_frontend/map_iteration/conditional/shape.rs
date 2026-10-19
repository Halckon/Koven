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
    // Every acquired provider uses the same full plan/body proof. A normal body
    // does not require a different lifetime or transport representation.
    Ok(Some(Shape {
        statement: descriptor.statement(),
        body,
        source,
    }))
}
