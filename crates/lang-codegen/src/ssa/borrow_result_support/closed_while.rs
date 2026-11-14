//! A following while may run only after this binding's direct lexical block ended.
//! This is not loop-carried borrow support and does not authorize control expressions.

use lang_frontend::{
    ast::StatementId,
    name_resolution::UnitSymbolId,
    ownership_checking::{
        BorrowBindingFact, CompilationUnitOwnership, UnitDropPoint, UnitLoanTarget,
    },
    parser::{Expression, ParsedFile, Statement},
    type_checking::{CompilationUnitTypes, UnitExpressionId},
};

use super::local_if::binding_marker;

type Binding = BorrowBindingFact<UnitExpressionId, UnitSymbolId, UnitLoanTarget>;

pub(in crate::ssa) fn following(
    file: &ParsedFile,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    fact: &Binding,
) -> Vec<StatementId> {
    let mut allowed = Vec::new();
    if fact.marker_span().source_id() != file.source_id()
        || super::ordinary_local::policy(file, typed, owned, fact)
            != Some(super::ordinary_local::Policy::Scoped)
    {
        return allowed;
    }
    let mut ends = owned
        .borrow_results()
        .ends()
        .iter()
        .filter(|end| end.binding() == fact.binding());
    let (Some(end), None) = (ends.next(), ends.next()) else {
        return allowed;
    };
    let UnitDropPoint::AfterStatement(end_statement) = end.point() else {
        return allowed;
    };
    if end_statement.source_unit() != fact.binding().source_unit() {
        return allowed;
    }
    // The actual close is the final call, not a synthetic end at the block boundary.
    let mut scopes = file.ast().statements().iter().filter_map(|(scope, node)| {
        let Statement::Block { elements } = node.payload() else {
            return None;
        };
        if elements.last() != Some(&end_statement.statement())
            || !elements
                .iter()
                .any(|id| binding_marker(file, *id) == Some(fact.marker_span()))
            || !elements.iter().all(|id| {
                binding_marker(file, *id).is_some() || ordinary_call(file, typed, fact, *id)
            })
            || !ordinary_call(file, typed, fact, end_statement.statement())
        {
            return None;
        }
        // No hidden branch, nested closure or transfer in initializer/argument subtrees.
        if file.ast().expressions().iter().any(|(_, expression)| {
            node.span().start() <= expression.span().start()
                && expression.span().end() <= node.span().end()
                && matches!(
                    expression.payload(),
                    Expression::If { .. }
                        | Expression::When { .. }
                        | Expression::Lambda { .. }
                        | Expression::Return { .. }
                        | Expression::Break { .. }
                        | Expression::Continue { .. }
                )
        }) {
            return None;
        }
        Some(scope)
    });
    let (Some(scope), None) = (scopes.next(), scopes.next()) else {
        return allowed;
    };
    // Direct siblings only: no ancestor lifting, branches, or inferred span ordering.
    for (_, parent) in file.ast().statements().iter() {
        let Statement::Block { elements } = parent.payload() else {
            continue;
        };
        let Some(index) = elements.iter().position(|statement| *statement == scope) else {
            continue;
        };
        for statement in &elements[index + 1..] {
            let Ok(node) = file.ast().statements().get(*statement) else {
                continue;
            };
            let Statement::While { condition, .. } = node.payload() else {
                continue;
            };
            if !file
                .ast()
                .expressions()
                .get(*condition)
                .is_ok_and(|condition| {
                    matches!(
                        condition.payload(),
                        Expression::Name | Expression::Literal(_)
                    )
                })
            {
                continue;
            }
            // Do not transfer this capability to a new binding or nested loop.
            if file.ast().statements().iter().any(|(child, child_node)| {
                child != *statement
                    && node.span().start() <= child_node.span().start()
                    && child_node.span().end() <= node.span().end()
                    && (binding_marker(file, child).is_some()
                        || matches!(
                            child_node.payload(),
                            Statement::While { .. }
                                | Statement::Loop { .. }
                                | Statement::For { .. }
                        ))
            }) {
                continue;
            }
            if !allowed.contains(statement) {
                allowed.push(*statement);
            }
        }
    }
    allowed
}

fn ordinary_call(
    file: &ParsedFile,
    typed: &CompilationUnitTypes,
    fact: &Binding,
    statement: StatementId,
) -> bool {
    let Ok(node) = file.ast().statements().get(statement) else {
        return false;
    };
    let Statement::Expression { expression } = node.payload() else {
        return false;
    };
    let Ok(call) = file.ast().expressions().get(*expression) else {
        return false;
    };
    let Expression::Call { arguments, .. } = call.payload() else {
        return false;
    };
    typed
        .call(UnitExpressionId::new(
            fact.binding().source_unit(),
            *expression,
        ))
        .is_some_and(|call| call.borrow_return().is_none())
        && arguments.iter().all(|argument| {
            file.ast()
                .expressions()
                .get(argument.value)
                .is_ok_and(|value| {
                    matches!(value.payload(), Expression::Name | Expression::Literal(_))
                })
        })
}
