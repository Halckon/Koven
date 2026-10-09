//! Independent future uses for closed straight-line borrow-result scopes.
//! This does not use owned-value ASAP liveness or shorten call-scoped loans.
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ast::{ExpressionId, StatementId},
    parser::{BinaryOperator, Expression, Item, ParsedFile, Statement, StringPart, VariableKind},
};

use super::OwnershipCheckingError;

pub(crate) struct StraightLineBorrowUses<S> {
    after: BTreeMap<usize, BTreeSet<S>>,
}

impl<S: Copy + Ord> StraightLineBorrowUses<S> {
    /// Opaque control flow/capture makes the entire sequence conservative.
    pub(crate) fn build(
        parsed: &ParsedFile,
        elements: &[StatementId],
        mut root: impl FnMut(ExpressionId) -> Result<Option<S>, OwnershipCheckingError>,
    ) -> Result<Option<Self>, OwnershipCheckingError> {
        let mut declares_borrow = false;
        for &statement in elements {
            if let Statement::LocalVariable { declaration } =
                parsed.ast().statements().get(statement)?.payload()
                && matches!(
                    parsed.ast().items().get(*declaration)?.payload(),
                    Item::Variable {
                        kind: VariableKind::BorrowVal(_),
                        ..
                    }
                )
            {
                declares_borrow = true;
                break;
            }
        }
        if !declares_borrow {
            return Ok(None);
        }
        let mut after = BTreeMap::new();
        let mut future = BTreeSet::new();
        for &statement in elements.iter().rev() {
            let expression = match parsed.ast().statements().get(statement)?.payload() {
                Statement::Expression { expression } => *expression,
                Statement::LocalVariable { declaration } => {
                    let Item::Variable { initializer, .. } =
                        parsed.ast().items().get(*declaration)?.payload()
                    else {
                        return Ok(None);
                    };
                    *initializer
                }
                _ => return Ok(None),
            };
            after.insert(statement.index(), future.clone());
            let Some(uses) = expression_uses(parsed, expression, &mut root)? else {
                return Ok(None);
            };
            future.extend(uses);
        }
        Ok(Some(Self { after }))
    }

    pub(crate) fn after(&self, statement: StatementId) -> Option<&BTreeSet<S>> {
        self.after.get(&statement.index())
    }
}

/// A future child keeps every active parent alive; inherited bindings stay scoped.
pub(crate) fn dead_bindings<S: Copy + Ord>(
    active: &BTreeSet<S>,
    inherited: &BTreeSet<S>,
    future: &BTreeSet<S>,
    mut parent: impl FnMut(S) -> Option<S>,
) -> BTreeSet<S> {
    let mut live = active
        .intersection(future)
        .copied()
        .collect::<BTreeSet<_>>();
    live.extend(active.intersection(inherited));
    let mut pending = live.iter().copied().collect::<Vec<_>>();
    while let Some(binding) = pending.pop() {
        if let Some(parent) = parent(binding)
            && active.contains(&parent)
            && live.insert(parent)
        {
            pending.push(parent);
        }
    }
    active.difference(&live).copied().collect()
}

fn expression_uses<S: Copy + Ord>(
    parsed: &ParsedFile,
    expression: ExpressionId,
    root: &mut impl FnMut(ExpressionId) -> Result<Option<S>, OwnershipCheckingError>,
) -> Result<Option<BTreeSet<S>>, OwnershipCheckingError> {
    let mut uses = BTreeSet::new();
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        match parsed.ast().expressions().get(expression)?.payload() {
            Expression::Name => {
                if let Some(symbol) = root(expression)? {
                    uses.insert(symbol);
                }
            }
            Expression::Literal(_) | Expression::This | Expression::SuperMember { .. } => {}
            Expression::Group { expression } | Expression::TypeTest { expression, .. } => {
                pending.push(*expression)
            }
            Expression::Prefix { operand, .. } | Expression::NonNullAssert { operand, .. } => {
                pending.push(*operand);
            }
            Expression::String { parts } => {
                for part in parts {
                    match part {
                        StringPart::Interpolation { expression, .. } => pending.push(*expression),
                        StringPart::Text(_) => {}
                        StringPart::Error(_) => return Ok(None),
                    }
                }
            }
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } if !matches!(
                operator,
                BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr | BinaryOperator::Elvis
            ) =>
            {
                pending.extend([*left, *right]);
            }
            Expression::Assignment { target, value, .. } => pending.extend([*target, *value]),
            Expression::Member {
                receiver,
                safe: false,
                ..
            } => pending.push(*receiver),
            Expression::Call {
                callee, arguments, ..
            } => {
                pending.push(*callee);
                pending.extend(arguments.iter().map(|argument| argument.value));
            }
            Expression::Index { receiver, index } => pending.extend([*receiver, *index]),
            // Nested blocks, return/loop edges, conditional evaluation, captures and casts
            // have no independent proof here. Existing scope/return handling remains.
            _ => return Ok(None),
        }
    }
    Ok(Some(uses))
}
