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

fn binding_marker(parsed: &ParsedFile, statement: StatementId) -> Option<Span> {
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
) -> bool {
    let Ok(node) = parsed.ast().expressions().get(expression) else {
        return false;
    };
    let Expression::If {
        condition,
        then_branch,
        else_branch: Some(else_branch),
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
        || elements
            .iter()
            .filter_map(|statement| binding_marker(parsed, *statement))
            .any(|marker| marker.end() > node.span().start())
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
    if ![*then_branch, *else_branch].into_iter().all(|branch| {
        let Ok(branch) = parsed.ast().statements().get(branch) else {
            return false;
        };
        let Statement::ControlBody { elements } = branch.payload() else {
            return false;
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
                builtin(expression) == Some(BuiltinType::Unit)
                    && owned_call(expression)
                    && parsed
                        .ast()
                        .expressions()
                        .get(expression)
                        .is_ok_and(|node| matches!(node.payload(), Expression::Call { .. }))
            })
    }) {
        return false;
    }
    // Check the entire candidate, including its condition. Existing call-prefix
    // exceptions must not smuggle an early exit into this new normal-arm slice.
    parsed.ast().expressions().iter().all(|(id, child)| {
        if !contains(node.span(), child.span()) || id == expression {
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
