//! Branch-form conditions: `&&`/`||`/`!` short-circuit and null comparisons with proofs.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::UnitDropPoint,
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
    /// Short-circuit: the skipped exit consumes `BranchExit` 1 and the evaluated exits
    /// consume `BranchExit` 0 on both outcome edges. Proof views flow only to the true
    /// target; any other edge ends them.
    fn logical(
        &mut self,
        expression: ExpressionId,
        and: bool,
        (left, right): (ExpressionId, ExpressionId),
        when_true: BlockRef,
        when_false: BlockRef,
    ) -> Result<(), LoweringError> {
        let span = self.span(expression);
        let control = self.unit(expression);
        let middle = self.body.block();
        let skip = self.body.block();
        if and {
            self.condition(left, middle, skip)?;
        } else {
            self.condition(left, skip, middle)?;
        }
        let skip_target = if and { when_false } else { when_true };
        let skipped = self.collect_drops(UnitDropPoint::BranchExit { control, branch: 1 });
        let mark = self.enter(skip);
        self.emit_drops(&skipped)?;
        self.finish_drops(&skipped);
        self.route_views(mark, skip_target, when_true, span)?;
        self.goto(skip_target, span)?;

        let mark = self.enter(middle);
        let evaluated_true = self.body.block();
        let evaluated_false = self.body.block();
        self.condition(right, evaluated_true, evaluated_false)?;
        let left_views: Vec<_> = self.active_views.drain(mark..).collect();
        let evaluated = self.collect_drops(UnitDropPoint::BranchExit { control, branch: 0 });
        for (path, target) in [(evaluated_true, when_true), (evaluated_false, when_false)] {
            let mark = self.enter(path);
            self.active_views.extend(left_views.iter().copied());
            self.emit_drops(&evaluated)?;
            self.route_views(mark, target, when_true, span)?;
            self.goto(target, span)?;
        }
        self.finish_drops(&evaluated);
        Ok(())
    }

    /// Views above `mark` continue into `target` when it is the true target; otherwise
    /// they end here.
    fn route_views(
        &mut self,
        mark: usize,
        target: BlockRef,
        when_true: BlockRef,
        span: lang_frontend::source::Span,
    ) -> Result<(), LoweringError> {
        let views: Vec<_> = self.active_views.drain(mark..).collect();
        if target == when_true {
            self.pending_views
                .extend(views.into_iter().map(|view| super::View { target, ..view }));
            return Ok(());
        }
        for view in views {
            self.push(ROp::BorrowEnd { loan: view.slot }, Vec::new(), span)?;
        }
        Ok(())
    }

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
                return self.logical(
                    expression,
                    operator == BinaryOperator::LogicalAnd,
                    (left, right),
                    when_true,
                    when_false,
                );
            }
            _ => {}
        }
        if let Some(fact) = self.inputs.typed.null_comparison(self.unit(expression)) {
            let owner = match self.bindings.get(&fact.symbol()) {
                Some(owner) => *owner,
                // A Copyable Borrow parameter is tested on a copy read through its loan.
                None => {
                    let loan = *self
                        .borrowed
                        .get(&fact.symbol())
                        .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                    let EntityType::Loan { target, .. } = self.body.slots[loan.0] else {
                        return Err(error(LoweringErrorKind::InvalidModel, span));
                    };
                    self.value(ROp::ReadLoan { loan }, EntityType::Value(target), span)?
                }
            };
            // The operands are read by the branch itself; their drop points follow it on
            // both edges.
            let Expression::Binary { left, right, .. } = payload else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            let operand_drops: Vec<_> = [left, right]
                .into_iter()
                .flat_map(|operand| {
                    self.collect_drops(UnitDropPoint::AfterExpression(self.unit(operand)))
                })
                .collect();
            let drops_owner = operand_drops.iter().any(|(_, target, _)| {
                *target == lang_frontend::ownership_checking::UnitDropTarget::Named(fact.symbol())
            });
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
            if !self.move_only(inner) {
                let narrowed = self.value(
                    ROp::NullableTake { owner, proof: view },
                    EntityType::Value(inner_ssa),
                    span,
                )?;
                self.narrowed.insert(fact.symbol(), narrowed);
            } else if drops_owner {
                // The owner dies right after the test: its proof ends before the drop.
                self.push(ROp::BorrowEnd { loan: view }, Vec::new(), span)?;
            } else {
                // The view stays live in the branch it enters; Borrow uses read through it.
                self.pending_views.push(super::View {
                    symbol: fact.symbol(),
                    slot: view,
                    target: on_non_null,
                });
            }
            self.emit_drops(&operand_drops)?;
            self.goto(on_non_null, span)?;
            self.block = Some(null_block);
            self.emit_drops(&operand_drops)?;
            self.goto(on_null, span)?;
            self.finish_drops(&operand_drops);
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
