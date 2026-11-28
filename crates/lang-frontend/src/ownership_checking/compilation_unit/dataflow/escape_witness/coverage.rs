//! Explicit observation completeness, separate from borrowed-leaf witnesses.
use super::super::{Checker, OwnershipCheckingError, State};
use crate::{
    ast::{ExpressionId, StatementId},
    parser::{Expression, LiteralKind, Statement},
    type_checking::{BuiltinType, UnitTypeKind},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) enum Coverage {
    Known,
    Unknown,
}

impl Checker<'_> {
    pub(in super::super) fn record_escape_coverage(
        &self,
        id: ExpressionId,
        alias_covered: bool,
        state: &mut State,
    ) -> Result<(), OwnershipCheckingError> {
        let node = self.parsed.ast().expressions().get(id)?;
        if !self.escape_coverage_relevant(id, node.payload()) {
            return Ok(());
        }
        let known = if self.loop_spans.contains(self.parsed, node.span()) {
            false
        } else {
            match node.payload() {
                Expression::Literal(LiteralKind::Null) => true,
                Expression::Lambda { .. } => state
                    .closure_captures
                    .contains_key(&self.unit_expression(id)),
                Expression::Name => alias_covered,
                Expression::Group { expression } => self.escape_coverage_known(*expression, state),
                Expression::If {
                    then_branch,
                    else_branch: Some(other),
                    ..
                } => {
                    self.escape_tail_covered(*then_branch, state)?
                        && self.escape_tail_covered(*other, state)?
                }
                Expression::When { entries, .. } => {
                    let mut all = !entries.is_empty();
                    for entry in entries {
                        all &= self.escape_tail_covered(entry.body, state)?;
                    }
                    all
                }
                _ => false,
            }
        };
        state.escape_witnesses.coverage.insert(
            self.unit_expression(id),
            if known {
                Coverage::Known
            } else {
                Coverage::Unknown
            },
        );
        Ok(())
    }

    fn escape_coverage_relevant(&self, id: ExpressionId, payload: &Expression) -> bool {
        if matches!(payload, Expression::Literal(LiteralKind::Null)) {
            return true;
        }
        match self
            .typed
            .expression_type(self.unit_expression(id))
            .and_then(|ty| self.typed.types().get(ty))
        {
            Some(UnitTypeKind::Function { .. }) => true,
            Some(UnitTypeKind::Nullable(inner)) => matches!(
                self.typed.types().get(*inner),
                Some(UnitTypeKind::Function { .. } | UnitTypeKind::Builtin(BuiltinType::Nothing))
            ),
            _ => false,
        }
    }

    fn escape_coverage_known(&self, expression: ExpressionId, state: &State) -> bool {
        state
            .escape_witnesses
            .coverage
            .get(&self.unit_expression(expression))
            == Some(&Coverage::Known)
    }

    fn escape_tail_covered(
        &self,
        statement: StatementId,
        state: &State,
    ) -> Result<bool, OwnershipCheckingError> {
        Ok(
            match self.parsed.ast().statements().get(statement)?.payload() {
                Statement::Expression { expression } => {
                    self.escape_coverage_known(*expression, state)
                }
                Statement::ControlBody { elements } | Statement::LambdaBody { elements } => {
                    match elements.last() {
                        Some(tail) => self.escape_tail_covered(*tail, state)?,
                        None => false,
                    }
                }
                _ => false,
            },
        )
    }
}
