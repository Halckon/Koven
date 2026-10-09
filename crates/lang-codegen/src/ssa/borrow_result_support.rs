//! 首片 CFG 能力边界：不让尚未运输的结果/source loan 隐式跨块。
use super::{LoweringError, LoweringErrorKind, lowering_support::error};
use lang_frontend::{
    parser::{Expression, FunctionBody, FunctionForm, Item, ParsedFile, Statement},
    source::Span,
};

pub(super) fn declaration(parsed: &ParsedFile, marker: Span) -> Result<(), LoweringError> {
    let found = parsed.ast().items().iter().find(|(_, node)| {
        matches!(node.payload(), Item::Function { .. }) && contains(node.span(), marker)
    });
    let Some((_, node)) = found else {
        return Err(error(LoweringErrorKind::MissingFact, marker));
    };
    if !matches!(
        node.payload(),
        Item::Function {
            form: FunctionForm::Explicit {
                body: FunctionBody::Expression { .. },
                ..
            },
            ..
        }
    ) {
        return Err(error(LoweringErrorKind::UnsupportedNode, marker));
    }
    Ok(())
}

pub(super) fn binding(
    parsed: &ParsedFile,
    marker: Span,
    range: bool,
    range_prefixes: &[Span],
) -> Result<(), LoweringError> {
    let function = parsed
        .ast()
        .items()
        .iter()
        .filter(|(_, node)| {
            matches!(node.payload(), Item::Function { .. }) && contains(node.span(), marker)
        })
        .map(|(_, node)| node.span())
        .min_by_key(|span| span.end() - span.start())
        .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, marker))?;
    for (_, node) in parsed.ast().expressions().iter() {
        if range
            && matches!(
                node.payload(),
                Expression::If { .. } | Expression::Return { .. }
            )
            && range_prefixes
                .iter()
                .any(|prefix| contains(*prefix, node.span()))
        {
            continue;
        }
        if contains(function, node.span())
            && matches!(
                node.payload(),
                Expression::If { .. }
                    | Expression::When { .. }
                    | Expression::Return { .. }
                    | Expression::Binary { .. }
            )
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, node.span()));
        }
    }
    for (_, node) in parsed.ast().statements().iter() {
        if contains(function, node.span())
            && (matches!(
                node.payload(),
                Statement::While { .. } | Statement::Loop { .. }
            ) || (!range && matches!(node.payload(), Statement::For { .. })))
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, node.span()));
        }
    }
    Ok(())
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}
