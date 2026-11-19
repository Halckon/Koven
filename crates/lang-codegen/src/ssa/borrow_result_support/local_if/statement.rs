//! Proof for ordinary Unit-valued if statements and their scalar expression trees.
use super::*;
use lang_frontend::parser::StringPart;

/// Require one direct lexical scope whose every borrow binding has matching
/// published new-descriptor origin, continuation and exact scope-end facts.
pub(super) fn scope_for(
    parsed: &ParsedFile,
    marker: Span,
    include_control: bool,
    proven: impl Fn(Span, StatementId) -> bool,
) -> Option<StatementId> {
    parsed.ast().statements().iter().find_map(|(scope, node)| {
        let elements = match node.payload() {
            Statement::Block { elements } => elements,
            Statement::ControlBody { elements } if include_control => elements,
            _ => return None,
        };
        let markers = elements
            .iter()
            .filter_map(|statement| binding_marker(parsed, *statement))
            .collect::<Vec<_>>();
        (markers.contains(&marker) && markers.iter().all(|marker| proven(*marker, scope)))
            .then_some(scope)
    })
}

pub(in crate::ssa::borrow_result_support) fn allows(
    parsed: &ParsedFile,
    scope: StatementId,
    expression: ExpressionId,
    builtin: &impl Fn(ExpressionId) -> Option<BuiltinType>,
    owned_call: &impl Fn(ExpressionId) -> bool,
    returned: Option<ExpressionId>,
) -> bool {
    allows_tree(
        parsed, scope, expression, builtin, owned_call, returned, None,
    )
}

pub(super) fn allows_tree(
    parsed: &ParsedFile,
    scope: StatementId,
    expression: ExpressionId,
    builtin: &impl Fn(ExpressionId) -> Option<BuiltinType>,
    owned_call: &impl Fn(ExpressionId) -> bool,
    returned: Option<ExpressionId>,
    protected: Option<StatementId>,
) -> bool {
    let Ok(node) = parsed.ast().expressions().get(expression) else {
        return false;
    };
    let Expression::If {
        condition,
        then_branch,
        else_branch,
        ..
    } = node.payload()
    else {
        return false;
    };
    if builtin(*condition) != Some(BuiltinType::Boolean)
        || builtin(expression) != Some(BuiltinType::Unit)
    {
        return false;
    }
    let Ok(scope_node) = parsed.ast().statements().get(scope) else {
        return false;
    };
    let (Statement::Block { elements } | Statement::ControlBody { elements }) =
        scope_node.payload()
    else {
        return false;
    };
    // The selected if is a complete statement, never a Unit-valued argument,
    // initializer, control tail or loop body. Its bindings already exist.
    let is_statement = elements.iter().any(|statement| {
        parsed
            .ast()
            .statements()
            .get(*statement)
            .is_ok_and(|statement| {
                matches!(statement.payload(), Statement::Expression { expression: actual }
                if ungroup(parsed, *actual, statement.span()).ok() == Some(expression))
            })
    });
    if !is_statement
        || (returned.is_none()
            && elements
                .iter()
                .filter_map(|statement| binding_marker(parsed, *statement))
                .any(|marker| marker.end() > node.span().start()))
        || parsed.ast().statements().iter().any(|(_, statement)| {
            contains(statement.span(), node.span())
                && matches!(
                    statement.payload(),
                    Statement::While { .. } | Statement::For { .. } | Statement::Loop { .. }
                )
        })
        || parsed.ast().expressions().iter().any(|(_, parent)| {
            contains(parent.span(), node.span())
                && matches!(parent.payload(), Expression::Lambda { .. })
        })
    {
        return false;
    }
    let arm_allowed = |branch_id| {
        if Some(branch_id) == protected {
            return true;
        }
        let Ok(branch) = parsed.ast().statements().get(branch_id) else {
            return false;
        };
        let Statement::ControlBody { elements } = branch.payload() else {
            return matches!(branch.payload(), Statement::Expression { .. })
                && returned.is_some()
                && ordinary_local::bare_return(parsed, branch_id) == returned;
        };
        !elements.is_empty()
            && elements.iter().all(|statement| {
                if Some(*statement) == protected {
                    return true;
                }
                let Ok(statement) = parsed.ast().statements().get(*statement) else {
                    return false;
                };
                let Statement::Expression { expression } = statement.payload() else {
                    return false;
                };
                let Ok(expression) = ungroup(parsed, *expression, statement.span()) else {
                    return false;
                };
                Some(expression) == returned
                    || (builtin(expression) == Some(BuiltinType::Unit)
                        && owned_call(expression)
                        && parsed
                            .ast()
                            .expressions()
                            .get(expression)
                            .is_ok_and(|node| matches!(node.payload(), Expression::Call { .. })))
            })
    };
    // A missing else has no source arm to validate.
    if !std::iter::once(*then_branch)
        .chain(*else_branch)
        .all(arm_allowed)
    {
        return false;
    }
    let protected_span = protected.and_then(|id| {
        parsed
            .ast()
            .statements()
            .get(id)
            .ok()
            .map(|node| node.span())
    });
    normal_tree(parsed, node.span(), builtin, owned_call, |id, span| {
        id == expression
            || Some(id) == returned
            || protected_span.is_some_and(|block| contains(block, span))
    })
}

pub(super) fn normal_tree(
    parsed: &ParsedFile,
    root: Span,
    builtin: &impl Fn(ExpressionId) -> Option<BuiltinType>,
    owned_call: &impl Fn(ExpressionId) -> bool,
    excluded: impl Fn(ExpressionId, Span) -> bool,
) -> bool {
    parsed.ast().expressions().iter().all(|(id, child)| {
        if !contains(root, child.span()) || excluded(id, child.span()) {
            return true;
        }
        if builtin(id) == Some(BuiltinType::Nothing) {
            return false;
        }
        match child.payload() {
            Expression::Name
            | Expression::Literal(_)
            | Expression::Group { .. }
            | Expression::Prefix { .. }
            | Expression::Binary { .. }
            | Expression::Member { safe: false, .. } => true,
            Expression::String { parts } => {
                parts.iter().all(|part| matches!(part, StringPart::Text(_)))
            }
            Expression::Call { .. } => {
                owned_call(id)
                    && matches!(
                        builtin(id),
                        Some(BuiltinType::Int | BuiltinType::Boolean | BuiltinType::Unit)
                    )
            }
            // Hidden nullable, propagation, indexing, capture and interpolation
            // control edges are not proved by the normal-arm selection.
            _ => false,
        }
    })
}
