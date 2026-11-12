//! 首片 CFG 能力边界：不让尚未运输的结果/source loan 隐式跨块。
use std::collections::BTreeSet;

use super::{
    LoweringError, LoweringErrorKind, lowering_support::error,
    unit_lower::span_contains as contains,
};
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{BinaryOperator, Expression, FunctionBody, FunctionForm, Item, ParsedFile, Statement},
    source::Span,
    type_checking::BuiltinType,
};

pub(super) mod closed_while;
pub(super) mod invariant_while;
pub(super) mod local_if;
pub(super) use local_if::ordinary_local;
pub(super) mod no_carry;
#[cfg(test)]
pub(super) use local_if::stable_map;

mod short_circuit {
    //! Select only already-proven lexical range scopes and normal Boolean uses.
    use super::*;
    use lang_frontend::parser::{PrefixOperator, StringPart, VariableKind};

    pub(super) fn allows(
        parsed: &ParsedFile,
        scope: StatementId,
        expression: ExpressionId,
        builtin: &impl Fn(ExpressionId) -> Option<BuiltinType>,
        call_allowed: &impl Fn(ExpressionId, CallAccess) -> bool,
    ) -> bool {
        let Ok(node) = parsed.ast().expressions().get(expression) else {
            return false;
        };
        let Expression::Binary {
            operator: BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr,
            left,
            right,
            ..
        } = node.payload()
        else {
            return false;
        };
        if [expression, *left, *right]
            .into_iter()
            .any(|id| builtin(id) != Some(BuiltinType::Boolean))
        {
            return false;
        }
        let Ok(scope_node) = parsed.ast().statements().get(scope) else {
            return false;
        };
        let Statement::Block { elements } = scope_node.payload() else {
            return false;
        };
        if !contains(scope_node.span(), node.span())
            || elements
                .iter()
                .filter_map(|statement| local_if::binding_marker(parsed, *statement))
                .any(|marker| marker.end() > node.span().start())
            || parsed.ast().statements().iter().any(|(_, parent)| {
                contains(parent.span(), node.span())
                    && matches!(
                        parent.payload(),
                        Statement::For { .. } | Statement::While { .. } | Statement::Loop { .. }
                    )
            })
            || parsed.ast().expressions().iter().any(|(id, parent)| {
                id != expression
                    && contains(parent.span(), node.span())
                    && matches!(
                        parent.payload(),
                        Expression::Lambda { .. }
                            | Expression::Binary {
                                operator: BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr,
                                ..
                            }
                    )
            })
            || !normal_tree(parsed, expression, expression, builtin, call_allowed)
        {
            return false;
        }
        // Select an entire direct initializer/argument/if condition, not a nested
        // expression or a call-prefix exception that introduces another CFG.
        elements.iter().any(|statement| {
            let Ok(statement) = parsed.ast().statements().get(*statement) else {
                return false;
            };
            match statement.payload() {
                Statement::LocalVariable { declaration } => {
                    let Ok(variable) = parsed.ast().items().get(*declaration) else {
                        return false;
                    };
                    let Item::Variable {
                        kind: VariableKind::Val | VariableKind::Var,
                        initializer,
                        ..
                    } = variable.payload()
                    else {
                        return false;
                    };
                    ungroup(parsed, *initializer, variable.span()).ok() == Some(expression)
                }
                Statement::Expression { expression: outer } => {
                    let Ok(outer) = ungroup(parsed, *outer, statement.span()) else {
                        return false;
                    };
                    let Ok(outer_node) = parsed.ast().expressions().get(outer) else {
                        return false;
                    };
                    match outer_node.payload() {
                        Expression::Call { arguments, .. } => {
                            arguments.iter().any(|argument| {
                                ungroup(parsed, argument.value, argument.span).ok()
                                    == Some(expression)
                            }) && normal_tree(parsed, outer, expression, builtin, call_allowed)
                        }
                        Expression::If { condition, .. } => {
                            ungroup(parsed, *condition, node.span()).ok() == Some(expression)
                                && local_if::allows(
                                    parsed,
                                    scope,
                                    outer,
                                    builtin,
                                    &|id| call_allowed(id, CallAccess::OwnedResult),
                                    None,
                                )
                        }
                        _ => false,
                    }
                }
                _ => false,
            }
        })
    }

    fn normal_tree(
        parsed: &ParsedFile,
        root: ExpressionId,
        selected: ExpressionId,
        builtin: &impl Fn(ExpressionId) -> Option<BuiltinType>,
        call_allowed: &impl Fn(ExpressionId, CallAccess) -> bool,
    ) -> bool {
        let Ok(node) = parsed.ast().expressions().get(root) else {
            return false;
        };
        parsed.ast().expressions().iter().all(|(id, child)| {
            if !contains(node.span(), child.span()) || id == selected {
                return true;
            }
            if builtin(id) == Some(BuiltinType::Nothing) {
                return false;
            }
            match child.payload() {
                Expression::Name
                | Expression::Literal(_)
                | Expression::Group { .. }
                | Expression::Member { safe: false, .. } => true,
                Expression::Prefix {
                    operator: PrefixOperator::Not,
                    operand,
                    ..
                } => {
                    builtin(*operand) == Some(BuiltinType::Boolean)
                        && builtin(id) == Some(BuiltinType::Boolean)
                }
                Expression::String { parts } => {
                    parts.iter().all(|part| matches!(part, StringPart::Text(_)))
                }
                Expression::Binary {
                    operator:
                        BinaryOperator::Equal
                        | BinaryOperator::NotEqual
                        | BinaryOperator::Less
                        | BinaryOperator::LessEqual
                        | BinaryOperator::Greater
                        | BinaryOperator::GreaterEqual,
                    left,
                    right,
                    ..
                } => {
                    [*left, *right]
                        .into_iter()
                        .all(|operand| builtin(operand) == Some(BuiltinType::Int))
                        && builtin(id) == Some(BuiltinType::Boolean)
                }
                Expression::Call { .. } => {
                    call_allowed(id, CallAccess::ReadOnly)
                        && matches!(
                            builtin(id),
                            Some(BuiltinType::Int | BuiltinType::Boolean | BuiltinType::Unit)
                        )
                }
                _ => false,
            }
        })
    }
}

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

#[allow(clippy::too_many_arguments)]
pub(super) fn binding(
    parsed: &ParsedFile,
    marker: Span,
    range: bool,
    range_prefixes: &[Span],
    allowed_controls: Option<&BTreeSet<usize>>,
    scope: Option<StatementId>,
    builtin: impl Fn(ExpressionId) -> Option<BuiltinType>,
    call_allowed: impl Fn(ExpressionId, CallAccess) -> bool,
    iteration_allowed: impl Fn(StatementId) -> bool,
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
        if allowed_controls.is_some_and(|controls| controls.contains(&expression.index())) {
            continue;
        }
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
                local_if::allows(
                    parsed,
                    scope,
                    expression,
                    &builtin,
                    &|id| call_allowed(id, CallAccess::OwnedResult),
                    None,
                ) || short_circuit::allows(parsed, scope, expression, &builtin, &call_allowed)
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
    for (statement, node) in parsed.ast().statements().iter() {
        if contains(function, node.span())
            && (matches!(node.payload(), Statement::Loop { .. })
                || (matches!(node.payload(), Statement::While { .. })
                    && !iteration_allowed(statement))
                || (!range
                    && matches!(node.payload(), Statement::For { .. })
                    && !iteration_allowed(statement)))
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, node.span()));
        }
    }
    Ok(())
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

/// Reject checked unary operations in an ordinary result call's argument subtree.
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
