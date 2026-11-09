//! Conservative condition outcomes from published completion facts and short-circuit structure.
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    ownership_checking::UnitShortCircuitRhs,
    parser::{BinaryOperator, Expression, PrefixOperator, Statement},
    type_checking::{BuiltinType, ParameterMode, UnitExpressionId, UnitStatementId, UnitTypeKind},
};

use super::Checker;

impl Checker<'_, '_> {
    /// Mirrors the builder: `!`, groups, `&&`, `||` and null comparisons are branch form
    /// (their own drop points are never emitted); other conditions are plain values.
    /// `views`: a MoveOnly null comparison may open a proof view only directly in an
    /// `if`/`while` condition (through groups and `&&`), on its true edge.
    pub(super) fn condition(&mut self, expression: ExpressionId, views: bool) -> bool {
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return false;
        };
        let id = self.id(expression);
        let boolean = self.expression_type(expression).is_some_and(|ty| {
            matches!(
                self.kind(ty),
                Some(UnitTypeKind::Builtin(BuiltinType::Boolean))
            )
        });
        if let Some(fact) = self.inputs.typed.null_comparison(id) {
            self.branch_form.insert(id);
            let nullable = self.c(fact.nullable_type());
            self.note_nullable(nullable);
            if let Some(supported) = self.borrowed_nullable_presence(fact.symbol(), nullable) {
                return boolean && supported && self.known.contains(&fact.symbol());
            }
            let inner_ok = match self.kind(nullable) {
                Some(UnitTypeKind::Nullable(inner))
                    if self.scalar(*inner)
                        || self.range_type(*inner)
                        || self.value_class(*inner) =>
                {
                    true
                }
                Some(UnitTypeKind::Nullable(inner)) if self.view_inner(*inner) => {
                    if views && fact.non_null_when_true() {
                        self.view_symbols.insert(fact.symbol());
                    }
                    true
                }
                _ => false,
            };
            return boolean && inner_ok && self.known.contains(&fact.symbol());
        }
        match node.payload().clone() {
            Expression::Group { expression: inner } => {
                self.branch_form.insert(id);
                boolean && self.plain(expression) && self.condition(inner, views)
            }
            Expression::Prefix {
                operator: PrefixOperator::Not,
                operand: inner,
                ..
            } => {
                self.branch_form.insert(id);
                boolean && self.plain(expression) && self.condition(inner, false)
            }
            Expression::Binary {
                left,
                operator: operator @ (BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr),
                right,
                ..
            } => {
                self.branch_form.insert(id);
                let views = views && operator == BinaryOperator::LogicalAnd;
                let saved = self.view_symbols.clone();
                let ok = boolean
                    && self.plain(expression)
                    && self.condition(left, views)
                    && (!self.logical_reaches_rhs(id, left, operator)
                        || self.condition(right, views));
                if !views {
                    self.view_symbols = saved;
                }
                ok
            }
            _ => self.expression(expression),
        }
    }

    pub(super) fn note_else_view(&mut self, condition: ExpressionId) {
        let root = self.ungrouped(condition);
        let id = self.id(root);
        if let Some(fact) = self.inputs.typed.null_comparison(id)
            && !fact.non_null_when_true()
            && self.parameter_mode(fact.symbol()) != Some(ParameterMode::Borrow)
            && self.known.contains(&fact.symbol())
            && let Some(UnitTypeKind::Nullable(inner)) = self.kind(self.c(fact.nullable_type()))
            && self.view_inner(*inner)
            && (self.scope_requires_refine()
                || self.map_kv(*inner).is_some()
                || self.is_map_remove_symbol(fact.symbol())
                || self.inputs.owned.loans().iter().any(|loan| {
                    matches!(
                        loan.target(),
                        lang_frontend::ownership_checking::UnitLoanTarget::Place(place)
                            if place.is_root() && place.root() == fact.symbol()
                    )
                }))
        {
            self.view_symbols.insert(fact.symbol());
        }
    }

    fn expression_may_continue(&self, expression: ExpressionId) -> bool {
        self.inputs
            .typed
            .expression_falls_through(self.id(expression))
            != Some(false)
    }

    /// A Boolean leaf may take either normal edge unless the frontend proved it exits.
    pub(super) fn condition_outcomes(&self, expression: ExpressionId) -> (bool, bool) {
        let expression = self.id(expression);
        if self.inputs.constant_owned.is_some() {
            self.inputs.typed.structural_boolean_outcomes(expression)
        } else {
            self.inputs.typed.ordinary_boolean_outcomes(expression)
        }
    }

    pub(super) fn logical_reaches_rhs(
        &self,
        control: UnitExpressionId,
        left: ExpressionId,
        operator: BinaryOperator,
    ) -> bool {
        if self
            .inputs
            .constant_owned
            .and_then(|owned| owned.short_circuit_at(control))
            .is_some_and(|plan| plan.rhs() == UnitShortCircuitRhs::Never)
        {
            return false;
        }
        let (yes, no) = self.condition_outcomes(left);
        if operator == BinaryOperator::LogicalAnd {
            yes
        } else {
            no
        }
    }

    pub(super) fn statement_may_continue(&self, statement: StatementId) -> bool {
        let Ok(node) = self.inputs.parsed.ast().statements().get(statement) else {
            return true;
        };
        match node.payload() {
            Statement::Block { elements }
            | Statement::ControlBody { elements }
            | Statement::LambdaBody { elements } => elements
                .iter()
                .all(|&element| self.statement_may_continue(element)),
            Statement::Loop { .. } | Statement::While { .. } => {
                self.inputs
                    .typed
                    .loop_falls_through(UnitStatementId::new(self.inputs.source_unit, statement))
                    != Some(false)
            }
            Statement::Expression { expression } => {
                if !self.expression_may_continue(*expression)
                    || self
                        .expression_type(*expression)
                        .is_some_and(|ty| self.nothing(ty))
                {
                    return false;
                }
                if let Ok(node) = self.inputs.parsed.ast().expressions().get(*expression)
                    && let Expression::If {
                        condition,
                        then_branch,
                        else_branch,
                        ..
                    } = node.payload()
                {
                    let (yes, no) = self.condition_outcomes(*condition);
                    return (yes && self.statement_may_continue(*then_branch))
                        || (no
                            && else_branch
                                .is_none_or(|branch| self.statement_may_continue(branch)));
                }
                true
            }
            _ => true,
        }
    }
}
