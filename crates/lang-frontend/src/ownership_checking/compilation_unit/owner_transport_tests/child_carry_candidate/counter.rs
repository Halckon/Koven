//! Source-level counter relationships only; no CFG proof.
use super::*;
use crate::name_resolution::{ReferenceTarget, SymbolKind};
use crate::parser::{
    AssignmentOperator, BinaryOperator, Expression, IntegerLiteralKind, LiteralKind,
};

pub(super) fn matches(
    fixture: &Fixture,
    root: UnitSymbolId,
    initializer: crate::ast::ExpressionId,
    loop_statement: crate::ast::StatementId,
    step_statement: crate::ast::StatementId,
) -> Option<UnitSymbolId> {
    let source = fixture.source(0);
    let ast = fixture.parsed[0].ast();
    let resolution = fixture.names.names().source_units()[source.index()].resolution();
    let name = |id| {
        let node = ast.expressions().get(id).ok()?;
        if !matches!(node.payload(), Expression::Name) {
            return None;
        }
        let references = resolution
            .references()
            .iter()
            .filter(|reference| reference.span() == node.span())
            .collect::<Vec<_>>();
        let [reference] = references.as_slice() else {
            return None;
        };
        let ReferenceTarget::Symbol(symbol) = reference.target() else {
            return None;
        };
        Some(UnitSymbolId::new(source, *symbol))
    };
    let literal = |id, value| {
        let node = ast.expressions().get(id).ok()?;
        (matches!(
            node.payload(),
            Expression::Literal(LiteralKind::Integer(IntegerLiteralKind::Unsuffixed))
        ) && fixture.sources.slice(node.span()).ok()? == value)
            .then_some(())
    };
    let parameter = name(initializer)?;
    if resolution.symbols().get(parameter.symbol().index())?.kind() != SymbolKind::ValueParameter {
        return None;
    }
    let Statement::While { condition, .. } = ast.statements().get(loop_statement).ok()?.payload()
    else {
        return None;
    };
    let Expression::Binary {
        left,
        operator: BinaryOperator::Greater,
        right,
        ..
    } = ast.expressions().get(*condition).ok()?.payload()
    else {
        return None;
    };
    if name(*left)? != root {
        return None;
    }
    literal(*right, "0")?;
    let Statement::Expression { expression } = ast.statements().get(step_statement).ok()?.payload()
    else {
        return None;
    };
    let Expression::Assignment {
        target,
        operator: AssignmentOperator::Assign,
        value,
        ..
    } = ast.expressions().get(*expression).ok()?.payload()
    else {
        return None;
    };
    if name(*target)? != root {
        return None;
    }
    let Expression::Binary {
        left,
        operator: BinaryOperator::Subtract,
        right,
        ..
    } = ast.expressions().get(*value).ok()?.payload()
    else {
        return None;
    };
    if name(*left)? != root {
        return None;
    }
    literal(*right, "1")?;
    Some(parameter)
}
