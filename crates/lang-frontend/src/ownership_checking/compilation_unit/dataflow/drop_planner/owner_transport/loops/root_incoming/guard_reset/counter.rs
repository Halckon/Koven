//! Private checked countdown tail. Completion is compiler evidence, not runtime induction.
use super::*;
use crate::parser::{BinaryOperator, LiteralKind};

#[derive(Clone, Debug)]
pub(in super::super) struct CounterTail {
    counter: UnitSymbolId,
    decrement: UnitExpressionId,
    reset: UnitExpressionId,
    next: u8,
}
impl CounterTail {
    pub(super) fn admit(
        checker: &Checker<'_>,
        statements: &[StatementId],
        guard: UnitSymbolId,
    ) -> Option<Self> {
        let [decrement, reset] = statements else {
            return None;
        };
        let (decrement, target, value) = assignment(checker, *decrement)?;
        let counter = name(checker, target)?;
        let (reset, reset_target, comparison) = assignment(checker, *reset)?;
        let ast = checker.parsed.ast();
        let Expression::Binary {
            left,
            operator: BinaryOperator::Subtract,
            right,
            ..
        } = ast.expressions().get(value).ok()?.payload()
        else {
            return None;
        };
        if counter == guard
            || name(checker, *left) != Some(counter)
            || ![target, *left, value]
                .into_iter()
                .all(|id| exact_type(checker, id, BuiltinType::Int))
            || !integer(checker, *right, 1)
        {
            return None;
        }
        let Expression::Binary {
            left,
            operator: BinaryOperator::Greater,
            right,
            ..
        } = ast.expressions().get(comparison).ok()?.payload()
        else {
            return None;
        };
        if name(checker, reset_target) != Some(guard)
            || name(checker, *left) != Some(counter)
            || !exact_type(checker, reset_target, BuiltinType::Boolean)
            || !exact_type(checker, comparison, BuiltinType::Boolean)
            || !exact_type(checker, *left, BuiltinType::Int)
            || !integer(checker, *right, 0)
        {
            return None;
        }
        Some(Self {
            counter,
            decrement: checker.unit_expression(decrement),
            reset: checker.unit_expression(reset),
            next: 0,
        })
    }
    pub(super) fn reset(&self) -> UnitExpressionId {
        self.reset
    }
    pub(in super::super) fn complete(
        &mut self,
        id: UnitExpressionId,
        roots_complete: bool,
    ) -> bool {
        if id != self.decrement && id != self.reset {
            return true;
        }
        let expected = match self.next {
            0 => self.decrement,
            1 => self.reset,
            _ => return false,
        };
        if !roots_complete || id != expected {
            return false;
        }
        self.next += 1;
        true
    }
    pub(in super::super) fn is_complete(&self) -> bool {
        self.next == 2
    }
}
fn assignment(
    checker: &Checker<'_>,
    statement: StatementId,
) -> Option<(ExpressionId, ExpressionId, ExpressionId)> {
    let ast = checker.parsed.ast();
    let Statement::Expression { expression } = ast.statements().get(statement).ok()?.payload()
    else {
        return None;
    };
    let Expression::Assignment {
        target,
        operator: crate::parser::AssignmentOperator::Assign,
        value,
        ..
    } = ast.expressions().get(*expression).ok()?.payload()
    else {
        return None;
    };
    Some((*expression, *target, *value))
}
fn name(checker: &Checker<'_>, id: ExpressionId) -> Option<UnitSymbolId> {
    let node = checker.parsed.ast().expressions().get(id).ok()?;
    matches!(node.payload(), Expression::Name)
        .then(|| checker.reference_symbol(node.span()))
        .flatten()
}
fn integer(checker: &Checker<'_>, id: ExpressionId, expected: u128) -> bool {
    let Ok(node) = checker.parsed.ast().expressions().get(id) else {
        return false;
    };
    let Expression::Literal(LiteralKind::Integer(kind)) = node.payload() else {
        return false;
    };
    exact_type(checker, id, BuiltinType::Int)
        && checker
            .sources
            .slice(node.span())
            .ok()
            .and_then(|text| crate::type_checking::integer_literal_magnitude(text, *kind))
            == Some(expected)
}

/// Only a top-level counted loop with a declaration-only prefix. The marker is
/// private; its enclosing collector must block ordinary Complete publication.
pub(in super::super) fn prefix(
    checker: &Checker<'_>,
    item: ItemId,
) -> Option<(UnitStatementId, UnitExpressionId)> {
    let ast = checker.parsed.ast();
    let Item::Function { form, .. } = ast.items().get(item).ok()?.payload() else {
        return None;
    };
    let block = match form {
        FunctionForm::ImplicitUnitBlock(body)
        | FunctionForm::Explicit {
            body: FunctionBody::Block(body),
            ..
        } => *body,
        _ => return None,
    };
    let Statement::Block { elements } = ast.statements().get(block).ok()?.payload() else {
        return None;
    };
    let mut initializers = Vec::new();
    for statement in elements {
        match ast.statements().get(*statement).ok()?.payload() {
            Statement::LocalVariable { declaration } => {
                let Item::Variable {
                    name, initializer, ..
                } = ast.items().get(*declaration).ok()?.payload()
                else {
                    return None;
                };
                initializers.push((*checker.marker_symbol(*name)?, *initializer));
            }
            Statement::While {
                condition, body, ..
            } => {
                let guard = name(checker, *condition)?;
                let Statement::Block { elements } = ast.statements().get(*body).ok()?.payload()
                else {
                    return None;
                };
                if elements.len() < 4 {
                    return None;
                }
                let tail = CounterTail::admit(checker, &elements[elements.len() - 2..], guard)?;
                let (_, initial) = initializers.iter().find(|(symbol, _)| *symbol == guard)?;
                // No intervening call/assignment/computation may change the counter
                // after the initial predicate. Existing owner tracking still validates
                // the actual names, moves, and String producers in this pure prefix.
                if !initializers.iter().all(|(_, id)| {
                    *id == *initial || {
                        ast.expressions()
                            .get(*id)
                            .ok()
                            .is_some_and(|node| match node.payload() {
                                Expression::Name | Expression::Literal(_) => true,
                                Expression::String { parts } => !parts.iter().any(|part| {
                                    matches!(part, crate::parser::StringPart::Interpolation { .. })
                                }),
                                _ => false,
                            })
                    }
                }) {
                    return None;
                }
                let Expression::Binary {
                    left,
                    operator: BinaryOperator::Greater,
                    right,
                    ..
                } = ast.expressions().get(*initial).ok()?.payload()
                else {
                    return None;
                };
                if name(checker, *left) != Some(tail.counter)
                    || !exact_type(checker, *left, BuiltinType::Int)
                    || !exact_type(checker, *initial, BuiltinType::Boolean)
                    || !integer(checker, *right, 0)
                {
                    return None;
                }
                return Some((
                    UnitStatementId::new(checker.source_unit, *statement),
                    checker.unit_expression(*initial),
                ));
            }
            _ => return None,
        }
    }
    None
}
