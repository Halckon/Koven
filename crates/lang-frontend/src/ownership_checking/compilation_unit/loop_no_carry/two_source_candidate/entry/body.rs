//! Exact source entry shapes and declaration visibility; not general statement scheduling.
use super::*;
use crate::{
    ast::{ExpressionId, StatementId},
    parser::ParsedFile,
};
pub(super) struct EntryBody<'a> {
    pub span: Span,
    pub declarations: &'a [StatementId],
    pub expressions: [Option<ExpressionId>; 2],
    pub late: bool,
}
pub(super) fn extract<'a>(parsed: &'a ParsedFile, form: &FunctionForm) -> Option<EntryBody<'a>> {
    let (span, declarations, expressions) = match form {
        FunctionForm::ImplicitUnitBlock(body)
        | FunctionForm::Explicit {
            body: FunctionBody::Block(body),
            ..
        } => {
            let node = parsed.ast().statements().get(*body).ok()?;
            let Statement::Block { elements } = node.payload() else {
                return None;
            };
            if let [first, local, second] = elements.as_slice()
                && let Statement::Expression { expression: first } =
                    parsed.ast().statements().get(*first).ok()?.payload()
                && matches!(
                    parsed.ast().statements().get(*local).ok()?.payload(),
                    Statement::LocalVariable { .. }
                )
                && let Statement::Expression { expression: second } =
                    parsed.ast().statements().get(*second).ok()?.payload()
            {
                return Some(EntryBody {
                    span: node.span(),
                    declarations: &elements[1..2],
                    expressions: [Some(*first), Some(*second)],
                    late: true,
                });
            } else if let [local_a, local_b, first, second] = elements.as_slice()
                && [local_a, local_b].iter().all(|id| {
                    matches!(
                        parsed
                            .ast()
                            .statements()
                            .get(**id)
                            .ok()
                            .map(|n| n.payload()),
                        Some(Statement::LocalVariable { .. })
                    )
                })
                && let Statement::Expression { expression: first } =
                    parsed.ast().statements().get(*first).ok()?.payload()
                && let Statement::Expression { expression: second } =
                    parsed.ast().statements().get(*second).ok()?.payload()
            {
                (node.span(), &elements[..2], [Some(*first), Some(*second)])
            } else if let [local, first, second] = elements.as_slice()
                && matches!(
                    parsed.ast().statements().get(*local).ok()?.payload(),
                    Statement::LocalVariable { .. }
                )
                && let Statement::Expression { expression: first } =
                    parsed.ast().statements().get(*first).ok()?.payload()
                && let Statement::Expression { expression: second } =
                    parsed.ast().statements().get(*second).ok()?.payload()
            {
                (node.span(), &elements[..1], [Some(*first), Some(*second)])
            } else if let [first, second] = elements.as_slice()
                && let Statement::Expression { expression: first } =
                    parsed.ast().statements().get(*first).ok()?.payload()
                && let Statement::Expression { expression: second } =
                    parsed.ast().statements().get(*second).ok()?.payload()
            {
                (node.span(), &[][..], [Some(*first), Some(*second)])
            } else {
                let (last, declarations) = elements.split_last()?;
                let Statement::Expression { expression } =
                    parsed.ast().statements().get(*last).ok()?.payload()
                else {
                    return None;
                };
                (node.span(), declarations, [Some(*expression), None])
            }
        }
        FunctionForm::Explicit {
            body: FunctionBody::Expression { expression, .. },
            ..
        } => {
            let node = parsed.ast().expressions().get(*expression).ok()?;
            if !matches!(node.payload(), Expression::Call { .. }) {
                return None;
            }
            (node.span(), &[][..], [Some(*expression), None])
        }
        _ => return None,
    };
    Some(EntryBody {
        span,
        declarations,
        expressions,
        late: false,
    })
}
