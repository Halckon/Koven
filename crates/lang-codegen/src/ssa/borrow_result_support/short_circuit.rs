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
                            ungroup(parsed, argument.value, argument.span).ok() == Some(expression)
                        }) && normal_tree(parsed, outer, expression, builtin, call_allowed)
                    }
                    Expression::If { condition, .. } => {
                        ungroup(parsed, *condition, node.span()).ok() == Some(expression)
                            && local_if::allows(parsed, scope, outer, builtin, &|id| {
                                call_allowed(id, CallAccess::OwnedResult)
                            })
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
