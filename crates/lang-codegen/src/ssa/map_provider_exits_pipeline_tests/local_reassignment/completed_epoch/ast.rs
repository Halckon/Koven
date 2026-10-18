//! Original AST anchors for distinct completion and installation events.
use super::super::super::{
    fixture::{Failure, need},
    query_inputs::one,
};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{Expression, Item, NameMarker, ParsedFile, Statement},
    source::{SourceMap, Span},
};

pub(super) struct Site {
    pub(super) scan: Span,
    pub(super) binding: Span,
    pub(super) old: ExpressionId,
    pub(super) assignment: ExpressionId,
    pub(super) rhs: ExpressionId,
    pub(super) call: ExpressionId,
    pub(super) statement: StatementId,
    pub(super) source: ExpressionId,
    pub(super) body: StatementId,
}
pub(super) fn inside(at: Span, outer: Span) -> bool {
    at.source_id() == outer.source_id() && at.start() > outer.start() && at.end() < outer.end()
}
pub(super) fn span(parsed: &ParsedFile, id: ExpressionId) -> Span {
    parsed.ast().expressions().get(id).unwrap().span()
}
pub(super) fn sites(sources: &SourceMap, parsed: &ParsedFile) -> Result<Vec<Site>, Failure> {
    ["scan", "scanEmpty"]
        .into_iter()
        .map(|wanted| {
            let scan = one(
                parsed.roots().iter().filter_map(|id| {
                    let n = parsed.ast().items().get(*id).ok()?;
                    matches!(
                        n.payload(),
                        Item::Function { name: NameMarker::Present(name), .. }
                            if sources.slice(*name) == Ok(wanted)
                    )
                    .then_some(n.span())
                }),
                "completed epoch original scanner",
            )?;

            let (binding, old) = one(
                parsed
                    .ast()
                    .items()
                    .iter()
                    .filter_map(|(_, n)| match n.payload() {
                        Item::Variable {
                            name: NameMarker::Present(name),
                            initializer,
                            ..
                        } if inside(n.span(), scan) && sources.slice(*name) == Ok("source") => {
                            Some((*name, *initializer))
                        }
                        _ => None,
                    }),
                "completed epoch original local",
            )?;

            let (assignment, rhs) = one(
                parsed
                    .ast()
                    .expressions()
                    .iter()
                    .filter_map(|(id, n)| match n.payload() {
                        Expression::Assignment { target, value, .. }
                            if inside(n.span(), scan)
                                && sources.slice(span(parsed, *target)) == Ok("source") =>
                        {
                            Some((id, *value))
                        }
                        _ => None,
                    }),
                "completed epoch actual assignment",
            )?;

            need(
                assignment != rhs && span(parsed, assignment).end() == span(parsed, rhs).end(),
                "installation and complete RHS are distinct same-end events",
                (assignment, rhs),
            )?;

            let mut call = rhs;
            while let Expression::Group { expression } =
                parsed.ast().expressions().get(call).unwrap().payload()
            {
                call = *expression;
            }
            let (statement, source, body) = one(
                parsed
                    .ast()
                    .statements()
                    .iter()
                    .filter_map(|(id, n)| match n.payload() {
                        Statement::For { source, body, .. } if inside(n.span(), scan) => {
                            Some((id, *source, *body))
                        }
                        _ => None,
                    }),
                "old epoch original provider",
            )?;

            need(
                span(parsed, source).end() < span(parsed, assignment).start(),
                "provider precedes replacement of completed epoch",
                statement,
            )?;

            Ok(Site {
                scan,
                binding,
                old,
                assignment,
                rhs,
                call,
                statement,
                source,
                body,
            })
        })
        .collect()
}
