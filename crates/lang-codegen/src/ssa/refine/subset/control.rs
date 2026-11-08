//! Conservative condition outcomes from published completion facts and short-circuit structure.
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    parser::{BinaryOperator, Expression, PrefixOperator, Statement},
};

use super::Checker;

impl Checker<'_, '_> {
    fn expression_may_continue(&self, expression: ExpressionId) -> bool {
        self.inputs
            .typed
            .expression_falls_through(self.id(expression))
            != Some(false)
    }

    /// A Boolean leaf may take either normal edge unless the frontend proved it exits.
    pub(super) fn condition_outcomes(&self, expression: ExpressionId) -> (bool, bool) {
        if !self.expression_may_continue(expression) {
            return (false, false);
        }
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return (true, true);
        };
        match node.payload() {
            Expression::Group { expression } => self.condition_outcomes(*expression),
            Expression::Prefix {
                operator: PrefixOperator::Not,
                operand,
                ..
            } => {
                let (yes, no) = self.condition_outcomes(*operand);
                (no, yes)
            }
            Expression::Binary {
                left,
                operator: BinaryOperator::LogicalAnd,
                right,
                ..
            } => {
                let (left_true, left_false) = self.condition_outcomes(*left);
                let (right_true, right_false) = self.condition_outcomes(*right);
                (
                    left_true && right_true,
                    left_false || (left_true && right_false),
                )
            }
            Expression::Binary {
                left,
                operator: BinaryOperator::LogicalOr,
                right,
                ..
            } => {
                let (left_true, left_false) = self.condition_outcomes(*left);
                let (right_true, right_false) = self.condition_outcomes(*right);
                (
                    left_true || (left_false && right_true),
                    left_false && right_false,
                )
            }
            _ => (true, true),
        }
    }

    pub(super) fn logical_reaches_rhs(&self, left: ExpressionId, operator: BinaryOperator) -> bool {
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
            Statement::While { condition, .. } => {
                let (yes, no) = self.condition_outcomes(*condition);
                yes || no
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
