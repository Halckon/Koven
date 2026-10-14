//! 首片 CFG 能力边界：不让尚未运输的结果/source loan 隐式跨块。
use super::{LoweringError, LoweringErrorKind, lowering_support::error};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{BinaryOperator, Expression, FunctionBody, FunctionForm, Item, ParsedFile, Statement},
    source::Span,
    type_checking::BuiltinType,
};

mod local_if;
mod short_circuit;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CallAccess {
    OwnedResult,
    ReadOnly,
}

pub(super) fn range_scope(
    parsed: &ParsedFile,
    marker: Span,
    proven: impl Fn(Span, StatementId) -> bool,
) -> Option<StatementId> {
    local_if::scope(parsed, marker, proven)
}

/// Only discard transparent syntax; callers still validate the published expression facts.
pub(super) fn ungroup(
    parsed: &ParsedFile,
    mut expression: ExpressionId,
    span: Span,
) -> Result<ExpressionId, LoweringError> {
    loop {
        let node = parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Group { expression: inner } = node.payload() else {
            return Ok(expression);
        };
        expression = *inner;
    }
}

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
    scope: Option<StatementId>,
    builtin: impl Fn(ExpressionId) -> Option<BuiltinType>,
    call_allowed: impl Fn(ExpressionId, CallAccess) -> bool,
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
    for (expression, node) in parsed.ast().expressions().iter() {
        // Checked Int arithmetic uses the existing success/Abort edges and their
        // explicit carrier/root/source-loan transport; other borrow bindings keep
        // the original CFG frontier. Syntax alone is not a scalar type proof.
        if range
            && let Expression::Binary {
                operator,
                left,
                right,
                ..
            } = node.payload()
            && [*left, *right]
                .into_iter()
                .all(|operand| builtin(operand) == Some(BuiltinType::Int))
            && match operator {
                BinaryOperator::Add
                | BinaryOperator::Subtract
                | BinaryOperator::Multiply
                | BinaryOperator::Divide
                | BinaryOperator::Remainder => builtin(expression) == Some(BuiltinType::Int),
                // Comparisons do not create CFG edges. Both operand types and
                // the distinct Boolean result must come from typed facts.
                BinaryOperator::Equal
                | BinaryOperator::NotEqual
                | BinaryOperator::Less
                | BinaryOperator::LessEqual
                | BinaryOperator::Greater
                | BinaryOperator::GreaterEqual => builtin(expression) == Some(BuiltinType::Boolean),
                _ => false,
            }
        {
            continue;
        }
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
        if range
            && scope.is_some_and(|scope| {
                local_if::allows(parsed, scope, expression, &builtin, &|id| {
                    call_allowed(id, CallAccess::OwnedResult)
                }) || short_circuit::allows(parsed, scope, expression, &builtin, &call_allowed)
            })
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

/// Declared parameter identity; receivers remain outside the ordinary result slice.
pub(super) fn source_index(
    origin: lang_frontend::type_checking::BorrowReturnOrigin,
    span: Span,
) -> Result<usize, LoweringError> {
    match origin {
        lang_frontend::type_checking::BorrowReturnOrigin::Parameter(index) => Ok(index),
        _ => Err(error(LoweringErrorKind::UnsupportedNode, span)),
    }
}

/// An ordinary result call does not transport its source chain through argument CFG.
/// Restrict only this argument subtree; unrelated earlier straight-line work is unchanged.
pub(super) fn argument(parsed: &ParsedFile, span: Span) -> Result<(), LoweringError> {
    for (_, node) in parsed.ast().expressions().iter() {
        if contains(span, node.span())
            && matches!(
                node.payload(),
                Expression::NonNullAssert { .. }
                    | Expression::Prefix {
                        operator: lang_frontend::parser::PrefixOperator::Minus,
                        ..
                    }
            )
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, node.span()));
        }
    }
    Ok(())
}
