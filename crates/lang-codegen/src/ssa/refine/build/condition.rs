//! Branch-form conditions: `&&`/`||`/`!` short-circuit and null comparisons with proofs.
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    ownership_checking::{UnitDropPoint, UnitShortCircuitRhs},
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
    /// Short-circuit: Basic numbers the evaluated exit 0 and the skip 1. Const
    /// carries its own true/false edge numbering. Proof views flow only to the true target.
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
        // A constant left operand fixes the plan: `Never` yields the left value and skips the
        // right operand; `Always` evaluates the left operand and yields the right value.
        let rhs_branch = if let Some(plan) = self
            .inputs
            .constant_owned
            .and_then(|owned| owned.short_circuit_at(control))
        {
            match plan.rhs() {
                UnitShortCircuitRhs::Never => {
                    self.unreachable_drop_scopes.push(self.span(right));
                    if self.required(left)?.is_some() {
                        let target = if and { when_false } else { when_true };
                        self.goto(target, span)?;
                    }
                    return Ok(());
                }
                UnitShortCircuitRhs::Always => {
                    if self.required(left)?.is_none() {
                        return Ok(());
                    }
                    return self.condition_internal(right, when_true, when_false, views, None);
                }
                UnitShortCircuitRhs::Conditional => plan.rhs_branch(),
            }
        } else {
            0
        };
        if self.inputs.typed.expression_falls_through(control) == Some(false) {
            // If the left side completes, the frontend proved the RHS is required
            // and has no normal exit. Do not synthesize a skipped predecessor.
            self.unreachable_drop_points.extend([
                UnitDropPoint::BranchExit { control, branch: 0 },
                UnitDropPoint::BranchExit { control, branch: 1 },
            ]);
            if self.required(left)?.is_none() {
                self.unreachable_drop_scopes.push(self.span(right));
            } else if self.required(right)?.is_some() {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            return Ok(());
        }
        let middle = self.body.block();
        let skip = self.body.block();
        let ordinary = self.inputs.constant_owned.is_none();
        if ordinary && self.inputs.typed.ordinary_rhs_bypassed(control) {
            if self.required(left)?.is_some() {
                self.goto(skip, span)?;
            }
        } else if ordinary && self.inputs.typed.ordinary_rhs_required(control) {
            if self.required(left)?.is_some() {
                self.goto(middle, span)?;
            }
        } else if and {
            self.condition_internal(left, middle, skip, views, None)?;
        } else {
            self.condition_internal(left, skip, middle, views, None)?;
        }
        // The plain branch that selects between the right operand and the skip edge is the
        // short circuit's own control transfer.
        for block in &mut self.body.blocks {
            if let Some((
                RTerm::Branch {
                    when_true,
                    when_false,
                    ..
                },
                origin,
            )) = &mut block.terminator
                && ((and && *when_true == middle && *when_false == skip)
                    || (!and && *when_true == skip && *when_false == middle))
            {
                *origin = span;
            }
        }
        let skip_target = if and { when_false } else { when_true };
        let skip_point = UnitDropPoint::BranchExit {
            control,
            branch: 1 - rhs_branch,
        };
        let rhs_point = UnitDropPoint::BranchExit {
            control,
            branch: rhs_branch,
        };
        let skipped = self.collect_drops(skip_point);
        let outer_temporaries = self.temporaries.clone();
        let outer_loans = self.pending_call_loans.clone();
        let outer_ranges = self.short_ranges.clone();
        let outer_views = self.active_views.clone();
        let mut skip_state = None;
        if self.body.blocks[skip.0].has_predecessor {
            let mark = self.enter(skip);
            self.sync_dead();
            let pre_skip_dead = self.dead.clone();
            self.emit_drops(&skipped)?;
            self.finish_drops(&skipped);
            self.route_views(mark, skip_target, when_true, span)?;
            self.goto(skip_target, span)?;
            skip_state = Some((
                pre_skip_dead,
                self.temporaries.clone(),
                self.active_views.clone(),
            ));
        } else {
            self.unreachable_drop_points.push(skip_point);
        }

        if !self.body.blocks[middle.0].has_predecessor {
            self.unreachable_drop_scopes.push(self.span(right));
            self.unreachable_drop_points.push(rhs_point);
            return Ok(());
        }
        if let Some((pre_skip_dead, _, _)) = &skip_state {
            self.dead.clone_from(pre_skip_dead);
            self.dead_block = Some(middle);
            self.entry_dead
                .entry(middle)
                .or_default()
                .extend(self.dead.iter().copied());
        }
        self.temporaries = outer_temporaries;
        self.active_views = outer_views;
        let mark = self.enter(middle);
        let evaluated_true = self.body.block();
        let evaluated_false = self.body.block();
        self.condition_internal(right, evaluated_true, evaluated_false, views, None)?;
        let mark = mark.min(self.active_views.len());
        let left_views: Vec<_> = self.active_views.drain(mark..).collect();
        let evaluated = self.collect_drops(rhs_point);
        let mut reached = false;
        for (path, target) in [(evaluated_true, when_true), (evaluated_false, when_false)] {
            if !self.body.blocks[path.0].has_predecessor {
                continue;
            }
            reached = true;
            let mark = self.enter(path);
            self.active_views.extend(left_views.iter().copied());
            self.emit_drops(&evaluated)?;
            self.route_views(mark, target, when_true, span)?;
            self.goto(target, span)?;
        }
        if reached {
            self.finish_drops(&evaluated);
        } else {
            self.unreachable_drop_points.push(rhs_point);
            if let Some((_, temporaries, views)) = skip_state {
                // An exit on the evaluated edge does not consume the skipped edge's state.
                self.temporaries = temporaries;
                self.pending_call_loans = outer_loans;
                self.short_ranges = outer_ranges;
                self.active_views = views;
            }
        }
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
            if self.move_only(nullable_ty)
                && let Some(&source) = self.borrowed.get(&fact.symbol())
            {
                let bool_ty = self.ssa_type(self.expression_type(expression)?, span)?;
                let is_null = self.value(
                    ROp::NullableLoanIsNull { source },
                    EntityType::Value(bool_ty),
                    span,
                )?;
                self.emit_drops(&operand_drops)?;
                self.finish_drops(&operand_drops);
                return self.terminate(
                    RTerm::Branch {
                        condition: is_null,
                        when_true: on_null,
                        when_false: on_non_null,
                    },
                    control_span.unwrap_or(span),
                );
            }
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
            let branch_span = if self.types.nullable_maps.contains_key(&nullable_ssa) {
                control_span.unwrap_or(span)
            } else {
                span
            };
            self.terminate(
                RTerm::NullBranch {
                    owner,
                    when_null: null_block,
                    when_non_null: non_null_block,
                },
                branch_span,
            )?;
            self.block = Some(non_null_block);
            if !self.move_only(inner) {
                let narrowed = self.value(
                    ROp::NullableTake { owner, proof: view },
                    EntityType::Value(inner_ssa),
                    span,
                )?;
                self.narrowed.insert(fact.symbol(), narrowed);
            } else if !views
                || (!fact.non_null_when_true() && control_span.is_none())
                || drops_owner
            {
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
        let condition = match self.expression(expression)? {
            Lowered::Value(condition) => condition,
            Lowered::Diverged => return Ok(()),
            Lowered::Unit => return Err(error(LoweringErrorKind::MissingFact, span)),
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
            let ssa_ty = if matches!(self.inputs.typed.types().get(ty), Some(UnitTypeKind::Nullable(inner)) if self.is_nothing(*inner))
            {
                self.expected_nullable
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
            } else {
                self.ssa_type(ty, span)?
            };
            Some(self.body.slot(EntityType::Value(ssa_ty)))
        };
        let when_true = self.body.block();
        let when_false = self.body.block();
        self.body.blocks[when_true.0].origin = Some(self.statement_span(then_branch)?);
        self.body.blocks[when_false.0].origin = Some(match else_branch {
            Some(branch) => self.statement_span(branch)?,
            None => span,
        });
        let mixed_split = self.mixed_header_split(
            control,
            self.unit(condition),
            lang_frontend::type_checking::UnitStatementId::new(
                self.inputs.source_unit,
                then_branch,
            ),
            else_branch.map(|b| {
                lang_frontend::type_checking::UnitStatementId::new(self.inputs.source_unit, b)
            }),
            [when_true, when_false],
            span,
        )?;
        let cb = self.cb_split(
            control,
            self.unit(condition),
            lang_frontend::type_checking::UnitStatementId::new(
                self.inputs.source_unit,
                then_branch,
            ),
            else_branch.is_none(),
            [when_true, when_false],
            span,
        )?;
        let entry = self.current(span)?;
        let branch_condition = self.unit(condition);
        self.body.branch_plans.split(
            control,
            branch_condition,
            entry,
            [when_true, when_false],
            &self.borrowed,
            span,
        )?;
        self.body.nested_plans.split(
            control,
            branch_condition,
            entry,
            [when_true, when_false],
            &self.borrowed,
            span,
        )?;
        self.body.bounded_plans.split(
            control,
            branch_condition,
            entry,
            [when_true, when_false],
            &self.borrowed,
            span,
        )?;
        self.body.single_plans.split(
            control,
            branch_condition,
            entry,
            [when_true, when_false],
            &self.borrowed,
            span,
        )?;
        self.body.multi_plans.split(
            control,
            branch_condition,
            entry,
            [when_true, when_false],
            &self.borrowed,
            span,
        )?;
        let cr = self.cr_split(
            control,
            self.unit(condition),
            lang_frontend::type_checking::UnitStatementId::new(
                self.inputs.source_unit,
                then_branch,
            ),
            else_branch.is_none(),
            [when_true, when_false],
            span,
        )?;
        self.condition_for_control(condition, when_true, when_false, span)?;
        self.cb_split_emitted(cb, span)?;
        self.cr_split_emitted(cr, span)?;
        self.mixed_header_split_emitted(mixed_split, span)?;
        let outer_views = self.active_views.clone();
        let outer_pending_loans = self.pending_call_loans.clone();
        let outer_short_ranges = self.short_ranges.clone();
        let outer_temporaries = self.temporaries.clone();
        let mut continuing_temporaries = None;
        let mut predecessors = Vec::new();
        let mut branch_exits = Vec::new();
        for (index, (block, branch)) in [(when_true, Some(then_branch)), (when_false, else_branch)]
            .into_iter()
            .enumerate()
        {
            let exit_point = UnitDropPoint::BranchExit {
                control,
                branch: index,
            };
            if !self.body.blocks[block.0].has_predecessor {
                self.unreachable_drop_points.push(exit_point);
                if let Some(branch) = branch {
                    let span = self.statement_span(branch)?;
                    self.unreachable_drop_scopes.push(span);
                }
                continue;
            }
            self.active_views = outer_views.clone();
            self.pending_call_loans = outer_pending_loans.clone();
            self.short_ranges = outer_short_ranges.clone();
            self.temporaries.clone_from(&outer_temporaries);
            let mark = self.enter(block);
            let saved_expected = self.expected_nullable;
            if let Some(res) = result
                && matches!(
                    self.inputs.typed.types().get(ty),
                    Some(UnitTypeKind::Nullable(_))
                )
                && let EntityType::Value(ssa_ty) = self.body.slots[res.0]
            {
                self.expected_nullable = Some(ssa_ty);
            }
            self.mixed_header_arm(control, index, block, span)?;
            self.cb_arm(control, index, block, span)?;
            self.cr_arm(control, index, block, span)?;
            let value = match branch {
                Some(branch) => self.branch_value(branch)?,
                None => Some(None),
            };
            self.expected_nullable = saved_expected;
            self.leave(mark, span)?;
            let Some(value) = value else {
                self.unreachable_drop_points.push(exit_point);
                continue;
            };
            if let Some(result) = result {
                let (slot, source) =
                    value.ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
                let slot = self.adapt(slot, source, ty)?;
                self.deliver(source);
                self.push(ROp::Alias { source: slot }, vec![result], span)?;
            }
            self.drops(exit_point)?;
            continuing_temporaries
                .get_or_insert_with(|| self.temporaries.clone())
                .retain(|id, slot| self.temporaries.get(id) == Some(slot));
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
        let join = self.body.block();
        self.body.blocks[join.0].origin = Some(span);
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
                                loan_tag: None,
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
        self.short_ranges = outer_short_ranges;
        self.temporaries = continuing_temporaries.unwrap_or_default();
        let branch_predecessors = predecessors.iter().map(|p| p.block).collect::<Vec<_>>();
        self.body
            .branch_plans
            .join(control, join, &branch_predecessors, span)?;
        self.body
            .nested_plans
            .join(control, join, &branch_predecessors, span)?;
        self.body
            .bounded_plans
            .join(control, join, &branch_predecessors, span)?;
        self.body
            .single_plans
            .join(control, join, &branch_predecessors, span)?;
        self.body
            .multi_plans
            .join(control, join, &branch_predecessors, span)?;
        let reached = self.join_views(predecessors, join, span)?;
        self.cb_join(control, join, reached, span)?;
        self.cr_join(control, join, reached, span)?;
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
