//! Branch-form conditions: `&&`/`||`/`!` short-circuit and null comparisons with proofs.
use lang_frontend::{
    ast::ExpressionId,
    parser::{BinaryOperator, Expression, PrefixOperator},
    type_checking::UnitTypeKind,
};

use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind},
    refine::body::{BlockRef, ROp, RTerm},
};

impl Builder<'_, '_> {
    /// Ends the current block with a branch to `when_true` / `when_false`.
    pub(super) fn condition(
        &mut self,
        expression: ExpressionId,
        when_true: BlockRef,
        when_false: BlockRef,
    ) -> Result<(), LoweringError> {
        let (payload, span) = self.node(expression)?;
        match payload {
            Expression::Group { expression: inner } => {
                return self.condition(inner, when_true, when_false);
            }
            Expression::Prefix {
                operator: PrefixOperator::Not,
                operand,
                ..
            } => return self.condition(operand, when_false, when_true),
            Expression::Binary {
                left,
                operator: operator @ (BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr),
                right,
                ..
            } => {
                let middle = self.body.block();
                if operator == BinaryOperator::LogicalAnd {
                    self.condition(left, middle, when_false)?;
                } else {
                    self.condition(left, when_true, middle)?;
                }
                self.block = Some(middle);
                return self.condition(right, when_true, when_false);
            }
            _ => {}
        }
        if let Some(fact) = self.inputs.typed.null_comparison(self.unit(expression)) {
            let owner = *self
                .bindings
                .get(&fact.symbol())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let Some(UnitTypeKind::Nullable(inner)) =
                self.inputs.typed.types().get(fact.nullable_type())
            else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            let inner = *inner;
            let inner_ssa = self.ssa_type(inner, span)?;
            let null_block = self.body.block();
            let non_null_block = self.body.block();
            let view = self.body.slot(EntityType::Loan {
                kind: LoanKind::Shared,
                target: inner_ssa,
            });
            self.body.blocks[non_null_block.0].view = Some(view);
            self.terminate(
                RTerm::NullBranch {
                    owner,
                    when_null: null_block,
                    when_non_null: non_null_block,
                },
                span,
            )?;
            let (on_non_null, on_null) = if fact.non_null_when_true() {
                (when_true, when_false)
            } else {
                (when_false, when_true)
            };
            self.block = Some(non_null_block);
            if self.move_only(inner) {
                // MoveOnly proofs are views; the migrated subset has no such use yet.
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let narrowed = self.value(
                ROp::NullableTake { owner, proof: view },
                EntityType::Value(inner_ssa),
                span,
            )?;
            self.narrowed.insert(fact.symbol(), narrowed);
            self.goto(on_non_null, span)?;
            self.block = Some(null_block);
            self.goto(on_null, span)?;
            return Ok(());
        }
        let Lowered::Value(condition) = self.expression(expression)? else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        self.terminate(
            RTerm::Branch {
                condition,
                when_true,
                when_false,
            },
            span,
        )
    }
}
