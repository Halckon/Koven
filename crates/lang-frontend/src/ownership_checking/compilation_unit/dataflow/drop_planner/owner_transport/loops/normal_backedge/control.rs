//! Pure Copy control admitted for the same finite owner header equation.
use super::*;
use crate::parser::{BinaryOperator, LiteralKind};

// None is the original Boolean control; Some identifies the only admitted Int RHS.
// Starting at zero and increasing by one while index < an immutable Int parameter
// means each taken step is representable, including limit == Int.MAX_VALUE.
// No call, short circuit, early transfer, alias, or resource operation is admitted.
pub(super) fn admit(
    checker: &Checker<'_>,
    parameters: &BTreeSet<UnitSymbolId>,
    control: UnitSymbolId,
    initializer: ExpressionId,
    condition: ExpressionId,
    target: ExpressionId,
    value: ExpressionId,
) -> Option<Option<ExpressionId>> {
    let ast = checker.parsed.ast();
    let name = |id| {
        let node = ast.expressions().get(id).ok()?;
        matches!(node.payload(), Expression::Name)
            .then(|| checker.reference_symbol(node.span()))
            .flatten()
    };
    if name(target) != Some(control)
        || !exact_type(checker, condition, BuiltinType::Boolean)
        || checker.ordinary_condition_outcomes(condition) != (true, true)
    {
        return None;
    }
    let symbol_type = checker
        .typed
        .symbol_type(control)
        .and_then(|ty| checker.typed.types().get(ty));
    if symbol_type == Some(&UnitTypeKind::Builtin(BuiltinType::Boolean)) {
        return (exact_type(checker, initializer, BuiltinType::Boolean)
            && name(initializer).is_some_and(|id| parameters.contains(&id))
            && name(condition) == Some(control)
            && exact_type(checker, target, BuiltinType::Boolean)
            && exact_type(checker, value, BuiltinType::Boolean)
            && matches!(
                ast.expressions().get(value).ok()?.payload(),
                Expression::Literal(LiteralKind::Boolean(false))
            ))
        .then_some(None);
    }
    let integer = |id, expected| {
        if !exact_type(checker, id, BuiltinType::Int) {
            return false;
        }
        let Ok(node) = ast.expressions().get(id) else {
            return false;
        };
        let Expression::Literal(LiteralKind::Integer(kind)) = node.payload() else {
            return false;
        };
        checker
            .sources
            .slice(node.span())
            .ok()
            .and_then(|text| crate::type_checking::integer_literal_magnitude(text, *kind))
            == Some(expected)
    };
    let Expression::Binary {
        left,
        operator: BinaryOperator::Less,
        right,
        ..
    } = ast.expressions().get(condition).ok()?.payload()
    else {
        return None;
    };
    let Expression::Binary {
        left: current,
        operator: BinaryOperator::Add,
        right: step,
        ..
    } = ast.expressions().get(value).ok()?.payload()
    else {
        return None;
    };
    (symbol_type == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
        && integer(initializer, 0)
        && integer(*step, 1)
        && name(*left) == Some(control)
        && name(*current) == Some(control)
        && name(*right).is_some_and(|id| parameters.contains(&id) && id != control)
        && [target, value, *left, *right, *current]
            .into_iter()
            .all(|id| exact_type(checker, id, BuiltinType::Int)))
    .then_some(Some(value))
}
