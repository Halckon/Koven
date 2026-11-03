//! Branch-form conditions: `&&`/`||`/`!` short-circuit and null comparisons with proofs.
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    ownership_checking::UnitDropPoint,
    parser::{BinaryOperator, Expression, PrefixOperator, Statement},
    source::Span,
    type_checking::{ParameterMode, UnitTypeKind},
};

use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind},
    refine::body::{BlockRef, ROp, RTerm, Slot},
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
        views: bool,
    ) -> Result<(), LoweringError> {
        let views = views && and;
        let span = self.span(expression);
        let control = self.unit(expression);
        let middle = self.body.block();
        let skip = self.body.block();
        if and {
            self.condition_internal(left, middle, skip, views, None)?;
        } else {
            self.condition_internal(left, skip, middle, views, None)?;
        }
        let skip_target = if and { when_false } else { when_true };
        let skipped = self.collect_drops(UnitDropPoint::BranchExit { control, branch: 1 });
        let mark = self.enter(skip);
        self.sync_dead();
        let pre_skip_dead = self.dead.clone();
        self.emit_drops(&skipped)?;
        self.finish_drops(&skipped);
        self.route_views(mark, skip_target, when_true, span)?;
        self.goto(skip_target, span)?;

        self.dead = pre_skip_dead;
        self.dead_block = Some(middle);
        self.entry_dead
            .entry(middle)
            .or_default()
            .extend(self.dead.iter().copied());
        let mark = self.enter(middle);
        let evaluated_true = self.body.block();
        let evaluated_false = self.body.block();
        self.condition_internal(right, evaluated_true, evaluated_false, views, None)?;
        let mark = mark.min(self.active_views.len());
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
        let mark = mark.min(self.active_views.len());
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
        self.condition_internal(expression, when_true, when_false, true, None)
    }

    pub(super) fn condition_for_control(
        &mut self,
        expression: ExpressionId,
        when_true: BlockRef,
        when_false: BlockRef,
        control_span: Span,
    ) -> Result<(), LoweringError> {
        self.condition_internal(expression, when_true, when_false, true, Some(control_span))
    }

    pub(super) fn condition_with_views(
        &mut self,
        expression: ExpressionId,
        when_true: BlockRef,
        when_false: BlockRef,
        views: bool,
    ) -> Result<(), LoweringError> {
        self.condition_internal(expression, when_true, when_false, views, None)
    }

    fn condition_internal(
        &mut self,
        expression: ExpressionId,
        when_true: BlockRef,
        when_false: BlockRef,
        views: bool,
        control_span: Option<Span>,
    ) -> Result<(), LoweringError> {
        let (payload, span) = self.node(expression)?;
        match payload {
            Expression::Group { expression: inner } => {
                return self.condition_internal(inner, when_true, when_false, views, control_span);
            }
            Expression::Prefix {
                operator: PrefixOperator::Not,
                operand,
                ..
            } => {
                return self.condition_internal(
                    operand,
                    when_false,
                    when_true,
                    false,
                    control_span,
                );
            }
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
                    views,
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
            let mut operand_drops = Vec::new();
            for mut operand in [left, right] {
                loop {
                    if let Ok((Expression::Group { expression: inner }, _)) = self.node(operand) {
                        operand_drops.extend(
                            self.collect_drops(UnitDropPoint::AfterExpression(self.unit(operand))),
                        );
                        operand = inner;
                    } else {
                        operand_drops.extend(
                            self.collect_drops(UnitDropPoint::AfterExpression(self.unit(operand))),
                        );
                        break;
                    }
                }
            }
            operand_drops.extend(
                self.collect_drops(UnitDropPoint::AfterBinaryOperands(self.unit(expression))),
            );
            let drops_owner = operand_drops.iter().any(|(_, target, _)| {
                *target == lang_frontend::ownership_checking::UnitDropTarget::Named(fact.symbol())
            });
            let nullable_ty = self.c(fact.nullable_type());
            let nullable_ssa = self.ssa_type(nullable_ty, span)?;
            let Some(UnitTypeKind::Nullable(inner)) = self.inputs.typed.types().get(nullable_ty)
            else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            let inner = *inner;
            let inner_ssa = self.ssa_type(inner, span)?;
            let (on_non_null, on_null) = if fact.non_null_when_true() {
                (when_true, when_false)
            } else {
                (when_false, when_true)
            };
            let null_block = self.body.block();
            let non_null_block = self.body.block();
            if self.types.map_results.contains_key(&nullable_ssa) && !self.move_only(inner) {
                let bool_ty = self.ssa_type(self.expression_type(expression)?, span)?;
                let present = self.value(
                    ROp::AggregateProject {
                        aggregate: owner,
                        field: 0,
                    },
                    EntityType::Value(bool_ty),
                    span,
                )?;
                self.terminate(
                    RTerm::Branch {
                        condition: present,
                        when_true: non_null_block,
                        when_false: null_block,
                    },
                    span,
                )?;
                self.block = Some(non_null_block);
                let narrowed = self.value(
                    ROp::MapResultUnwrap { result: owner },
                    EntityType::Value(inner_ssa),
                    span,
                )?;
                self.narrowed.insert(fact.symbol(), narrowed);
                self.emit_drops(&operand_drops)?;
                self.entry_dead
                    .entry(on_non_null)
                    .or_default()
                    .extend(self.dead.iter().copied());
                self.goto(on_non_null, span)?;
                self.block = Some(null_block);
                self.emit_drops(&operand_drops)?;
                self.entry_dead
                    .entry(on_null)
                    .or_default()
                    .extend(self.dead.iter().copied());
                self.goto(on_null, span)?;
                self.finish_drops(&operand_drops);
                return Ok(());
            }
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
            self.block = Some(non_null_block);
            if !self.move_only(inner) {
                let narrowed = self.value(
                    ROp::NullableTake { owner, proof: view },
                    EntityType::Value(inner_ssa),
                    span,
                )?;
                self.narrowed.insert(fact.symbol(), narrowed);
            } else if !views || !fact.non_null_when_true() || drops_owner {
                // Outside a view-opening condition or when the owner dies right after the
                // test, its proof ends immediately before the drop.
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
            self.entry_dead
                .entry(on_non_null)
                .or_default()
                .extend(self.dead.iter().copied());
            self.goto(on_non_null, span)?;
            self.block = Some(null_block);
            self.emit_drops(&operand_drops)?;
            self.entry_dead
                .entry(on_null)
                .or_default()
                .extend(self.dead.iter().copied());
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
            control_span.unwrap_or(span),
        )
    }

    pub(super) fn if_expression(
        &mut self,
        expression: ExpressionId,
        condition: ExpressionId,
        then_branch: StatementId,
        else_branch: Option<StatementId>,
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let control = self.unit(expression);
        let ty = self.expression_type(expression)?;
        let result = if self.is_unit(ty) || self.is_nothing(ty) {
            None
        } else {
            Some(self.body.slot(EntityType::Value(self.ssa_type(ty, span)?)))
        };
        let when_true = self.body.block();
        let when_false = self.body.block();
        let join = self.body.block();
        self.condition_for_control(condition, when_true, when_false, span)?;
        let outer_views = self.active_views.clone();
        let outer_pending_loans = self.pending_call_loans.clone();
        let mut predecessors = Vec::new();
        let mut branch_exits = Vec::new();
        for (index, (block, branch)) in [(when_true, Some(then_branch)), (when_false, else_branch)]
            .into_iter()
            .enumerate()
        {
            self.active_views = outer_views.clone();
            self.pending_call_loans = outer_pending_loans.clone();
            let mark = self.enter(block);
            let value = match branch {
                Some(branch) => self.branch_value(branch)?,
                None => Some(None),
            };
            self.leave(mark, span)?;
            let Some(value) = value else {
                continue;
            };
            if let Some(result) = result {
                let (slot, source) =
                    value.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let slot = self.adapt(slot, source, ty)?;
                self.deliver(source);
                self.push(ROp::Alias { source: slot }, vec![result], span)?;
            }
            self.drops(UnitDropPoint::BranchExit {
                control,
                branch: index,
            })?;
            self.sync_dead();
            let exit_block = self.current(span)?;
            branch_exits.push((exit_block, self.this_dead));
            predecessors.push(super::when::PredecessorBranch {
                block: exit_block,
                views: self.active_views.clone(),
                dead: self.dead.clone(),
                this_dead: self.this_dead,
            });
        }
        if branch_exits.iter().any(|(_, dead)| *dead) {
            self.entry_this_dead.insert(join);
            if let Some(slot) = self.receiver
                && self
                    .inputs
                    .receiver
                    .is_some_and(|r| r.mode == ParameterMode::Value && self.move_only(r.ty))
            {
                for (i, &(exit_block, dead)) in branch_exits.iter().enumerate() {
                    if !dead {
                        self.body.blocks[exit_block.0].statements.push(
                            crate::ssa::refine::body::RStmt {
                                op: ROp::Drop { owner: slot },
                                results: Vec::new(),
                                span,
                            },
                        );
                        if let Some(pred) = predecessors.get_mut(i) {
                            pred.this_dead = true;
                        }
                    }
                }
            }
        }
        self.pending_call_loans = outer_pending_loans;
        let reached = self.join_views(predecessors, join, span)?;
        if !self.pending_views.is_empty() {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        if !reached {
            return Ok(Lowered::Diverged);
        }
        Ok(result.map_or(Lowered::Unit, Lowered::Value))
    }

    /// Lowers a branch body; `None` when it diverges, else its tail value and expression.
    pub(super) fn branch_value(
        &mut self,
        branch: StatementId,
    ) -> Result<Option<Option<(Slot, ExpressionId)>>, LoweringError> {
        let node = self
            .inputs
            .parsed
            .ast()
            .statements()
            .get(branch)
            .map_err(|_| error(LoweringErrorKind::MissingFact, self.inputs.span))?;
        let tail =
            match node.payload() {
                Statement::Expression { expression } => Some(*expression),
                Statement::ControlBody { elements } => elements.last().and_then(|last| match self
                    .inputs
                    .parsed
                    .ast()
                    .statements()
                    .get(*last)
                    .map(|n| n.payload())
                {
                    Ok(Statement::Expression { expression }) => Some(*expression),
                    _ => None,
                }),
                _ => None,
            };
        match self.statement(branch)? {
            Lowered::Diverged => Ok(None),
            Lowered::Value(slot) => Ok(Some(tail.map(|tail| (slot, tail)))),
            Lowered::Unit => Ok(Some(None)),
        }
    }
}
