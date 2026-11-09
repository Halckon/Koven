//! Narrow syntax selection only; ownership identity and termination come from facts.
use super::*;
use lang_frontend::{
    ast::StatementId,
    parser::{StringPart, VariableKind},
};

/// Require one direct lexical scope whose every borrow binding has matching
/// published new-descriptor origin, continuation and exact scope-end facts.
pub(super) fn scope(
    parsed: &ParsedFile,
    marker: Span,
    proven: impl Fn(Span, StatementId) -> bool,
) -> Option<StatementId> {
    parsed.ast().statements().iter().find_map(|(scope, node)| {
        let Statement::Block { elements } = node.payload() else {
            return None;
        };
        let markers = elements
            .iter()
            .filter_map(|statement| binding_marker(parsed, *statement))
            .collect::<Vec<_>>();
        (markers.contains(&marker) && markers.iter().all(|marker| proven(*marker, scope)))
            .then_some(scope)
    })
}

pub(super) fn binding_marker(parsed: &ParsedFile, statement: StatementId) -> Option<Span> {
    let Statement::LocalVariable { declaration } =
        parsed.ast().statements().get(statement).ok()?.payload()
    else {
        return None;
    };
    let Item::Variable {
        kind: VariableKind::BorrowVal(marker),
        ..
    } = parsed.ast().items().get(*declaration).ok()?.payload()
    else {
        return None;
    };
    Some(*marker)
}

pub(super) fn allows(
    parsed: &ParsedFile,
    scope: StatementId,
    expression: ExpressionId,
    builtin: &impl Fn(ExpressionId) -> Option<BuiltinType>,
    owned_call: &impl Fn(ExpressionId) -> bool,
    returned: Option<ExpressionId>,
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
    if (else_branch.is_none() && returned.is_none())
        || builtin(*condition) != Some(BuiltinType::Boolean)
        || builtin(expression) != Some(BuiltinType::Unit)
    {
        return false;
    }
    let Ok(scope_node) = parsed.ast().statements().get(scope) else {
        return false;
    };
    let Statement::Block { elements } = scope_node.payload() else {
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
    if !std::iter::once(*then_branch)
        .chain(*else_branch)
        .all(arm_allowed)
    {
        return false;
    }
    // Check the entire candidate, including its condition. Only the direct
    // return selected by the caller may terminate this branch.
    parsed.ast().expressions().iter().all(|(id, child)| {
        if !contains(node.span(), child.span()) || id == expression || Some(id) == returned {
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

/// A direct return statement or selected return arm needs an exact end fact
/// for this binding. A selected arm still uses the bounded if condition and body.
pub(in crate::ssa) fn return_controls(
    parsed: &ParsedFile,
    marker: Span,
    builtin: &impl Fn(ExpressionId) -> Option<BuiltinType>,
    owned_call: &impl Fn(ExpressionId) -> bool,
    ends_on_return: impl Fn(ExpressionId) -> bool,
    allowed: &mut BTreeSet<usize>,
) {
    let Some(scope) = scope(parsed, marker, |_, _| true) else {
        return;
    };
    let Ok(node) = parsed.ast().statements().get(scope) else {
        return;
    };
    let Statement::Block { elements } = node.payload() else {
        return;
    };
    for statement in elements {
        let Ok(node) = parsed.ast().statements().get(*statement) else {
            continue;
        };
        let Statement::Expression { expression } = node.payload() else {
            continue;
        };
        let Ok(control) = ungroup(parsed, *expression, node.span()) else {
            continue;
        };
        let Ok(node) = parsed.ast().expressions().get(control) else {
            continue;
        };
        if marker.end() > node.span().start() {
            continue;
        }
        if matches!(node.payload(), Expression::Return { .. }) {
            if ends_on_return(control) {
                allowed.insert(control.index());
            }
            continue;
        }
        let Expression::If {
            then_branch,
            else_branch,
            ..
        } = node.payload()
        else {
            continue;
        };
        let Some(returned) = std::iter::once(*then_branch)
            .chain(*else_branch)
            .find_map(|branch| ordinary_local::bare_return(parsed, branch))
        else {
            continue;
        };
        if ends_on_return(returned)
            && allows(parsed, scope, control, builtin, owned_call, Some(returned))
        {
            allowed.insert(control.index());
            allowed.insert(returned.index());
        }
    }
}
