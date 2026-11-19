//! Branch-form conditions: `&&`/`||`/`!` short-circuit and null comparisons with proofs.
use lang_frontend::{
    ast::{ExpressionId, StatementId},
    ownership_checking::UnitDropPoint,
    parser::{Expression, Statement},
    source::Span,
    type_checking::{BuiltinType, ParameterMode, UnitTypeKind},
};

use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{EntityType, LoanKind},
    refine::body::{BlockRef, ROp, RTerm, Slot},
};

mod logical;

use logical::{BorrowedProofEdges, ConditionInput, ConditionViews};

impl Builder<'_, '_> {
    /// Ends the current block with a branch to `when_true` / `when_false`.
    pub(super) fn condition(
        &mut self,
        expression: ExpressionId,
        when_true: BlockRef,
        when_false: BlockRef,
    ) -> Result<(), LoweringError> {
        self.condition_internal(
            expression,
            when_true,
            when_false,
            ConditionViews::normal(true),
            None,
            false,
        )
    }

    pub(super) fn condition_for_control(
        &mut self,
        expression: ExpressionId,
        when_true: BlockRef,
        when_false: BlockRef,
        control_span: Span,
    ) -> Result<(), LoweringError> {
        self.condition_internal(
            expression,
            when_true,
            when_false,
            ConditionViews {
                ordinary: true,
                borrowed: BorrowedProofEdges::Both,
            },
            Some(control_span),
            false,
        )
    }

    pub(super) fn condition_with_views(
        &mut self,
        expression: ExpressionId,
        when_true: BlockRef,
        when_false: BlockRef,
        views: bool,
    ) -> Result<(), LoweringError> {
        self.condition_internal(
            expression,
            when_true,
            when_false,
            ConditionViews::normal(views),
            None,
            false,
        )
    }

    pub(super) fn condition_for_selector(
        &mut self,
        expression: ExpressionId,
        matched: BlockRef,
        missed: BlockRef,
    ) -> Result<(), LoweringError> {
        self.condition_internal(
            expression,
            matched,
            missed,
            ConditionViews::normal(false),
            None,
            true,
        )
    }

    fn condition_internal(
        &mut self,
        expression: ExpressionId,
        when_true: BlockRef,
        when_false: BlockRef,
        views: ConditionViews,
        control_span: Option<Span>,
        fold_literal: bool,
    ) -> Result<(), LoweringError> {
        self.lower_condition_chain(ConditionInput {
            expression,
            when_true,
            when_false,
            views,
            control_span,
            fold_literal,
        })
    }

    fn condition_atom(
        &mut self,
        input: ConditionInput,
        payload: Expression,
        span: Span,
    ) -> Result<(), LoweringError> {
        let ConditionInput {
            expression,
            when_true,
            when_false,
            views,
            control_span,
            fold_literal,
        } = input;
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
                if views.borrowed.allows(fact.non_null_when_true())
                    && matches!(
                        self.body.slots[source.0],
                        EntityType::Loan { target, .. } if match self.types.ssa_types.get(target.index()) {
                            Some(crate::ssa::model::SsaTypeKind::InlineNullable { inner, niche: true }) =>
                                *inner == inner_ssa,
                            Some(crate::ssa::model::SsaTypeKind::NullableHandle { inner }) =>
                                *inner == inner_ssa && matches!(
                                    self.types.ssa_types.get(inner.index()),
                                    Some(crate::ssa::model::SsaTypeKind::SharedOwner { .. })
                                ),
                            _ => self.types.map_results.get(&target) == Some(&inner_ssa),
                        }
                    )
                    && self
                        .inputs
                        .typed
                        .non_null_uses()
                        .iter()
                        .any(|usage| usage.symbol() == fact.symbol())
                    && matches!(
                        self.inputs.typed.types().get(inner),
                        Some(
                            UnitTypeKind::Builtin(BuiltinType::String)
                                | UnitTypeKind::Intrinsic {
                                    constructor:
                                        lang_frontend::type_checking::IntrinsicTypeConstructor::Rc,
                                    ..
                                }
                        )
                    )
                {
                    let view = self.body.slot(EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: inner_ssa,
                    });
                    let present = self.body.block();
                    let absent = self.body.block();
                    self.body.blocks[present.0].view = Some(view);
                    self.terminate(
                        RTerm::NullBranch {
                            owner: source,
                            when_null: absent,
                            when_non_null: present,
                        },
                        control_span.unwrap_or(span),
                    )?;
                    for (edge, target) in [(present, on_non_null), (absent, on_null)] {
                        self.block = Some(edge);
                        self.emit_drops(&operand_drops)?;
                        self.entry_dead
                            .entry(target)
                            .or_default()
                            .extend(self.dead.iter().copied());
                        self.goto(target, span)?;
                    }
                    self.finish_drops(&operand_drops);
                    self.pending_views.push(super::View {
                        symbol: fact.symbol(),
                        slot: view,
                        target: on_non_null,
                        when_scope: None,
                    });
                    return Ok(());
                }
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
                self.record_narrowing(fact.symbol(), narrowed, span)?;
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
                self.record_narrowing(fact.symbol(), narrowed, span)?;
            } else if !views.ordinary
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
                    when_scope: None,
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
        self.fresh_loop_condition(self.unit(expression), condition, span)?;
        let condition_id = self.unit(expression);
        let condition_block = self.current(span)?;
        self.body
            .arm_loop_plans
            .condition(condition_id, condition_block, condition, span)?;
        if let Some(prefix) = self
            .nested_owner_body
            .as_mut()
            .and_then(|p| p.scalar_entry.as_mut())
        {
            prefix.condition(condition_id, condition_block, condition, span)?;
        }
        let origin = control_span.unwrap_or(span);
        // Short-circuit operands and paths carrying a Copyable proof need
        // actual reachability. Other controls retain their structural edges.
        let carries_proof = self
            .narrowed
            .get(&condition_block)
            .is_some_and(|symbols| !symbols.is_empty());
        if (fold_literal || carries_proof)
            && let Expression::Literal(lang_frontend::parser::LiteralKind::Boolean(value)) = payload
        {
            self.body.blocks[condition_block.0].folded_condition = true;
            return self.goto(if value { when_true } else { when_false }, origin);
        }
        self.terminate(
            RTerm::Branch {
                condition,
                when_true,
                when_false,
            },
            origin,
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
        let invariant = self.invariant_if_entry(expression, span)?;
        let arm_lease = self.arm_if_lease(expression, span)?;
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
                self.expression_ssa_type(expression, ty, span)?
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
        self.body
            .arm_loop_plans
            .split(control, [entry, when_true, when_false], span)?;
        if let Some(prefix) = self
            .nested_owner_body
            .as_mut()
            .and_then(|p| p.scalar_entry.as_mut())
        {
            prefix.split(control, [entry, when_true, when_false], span)?;
        }
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
        // A nested if cannot enter a sibling arm still pending in its enclosing control.
        let outer_pending_views = std::mem::take(&mut self.pending_views);
        self.condition_for_control(condition, when_true, when_false, span)?;
        self.invariant_while_boundary(invariant.as_ref(), span)?;
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
        // Each arm owns its incoming proofs while it builds nested control flow.
        let mut arm_views = std::mem::take(&mut self.pending_views);
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
            let (incoming_views, remaining_views): (Vec<_>, Vec<_>) =
                arm_views.into_iter().partition(|view| view.target == block);
            arm_views = remaining_views;
            self.pending_views.extend(incoming_views);
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
            if value.is_some() {
                self.leave_continuing_branch(&outer_views, span, true)?;
            } else {
                self.leave(mark, span)?;
            }
            if arm_lease.is_some() && value.is_none() {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            }
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
            self.invariant_while_boundary(invariant.as_ref(), span)?;
            self.arm_boundary(arm_lease.as_ref(), span)?;
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
                                loop_tag: None,
                                fresh_loop_tag: None,
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
            .arm_loop_plans
            .join(control, join, &branch_predecessors, span)?;
        if let Some(prefix) = self
            .nested_owner_body
            .as_mut()
            .and_then(|p| p.scalar_entry.as_mut())
        {
            prefix.join(control, join, &branch_predecessors, span)?;
        }
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
        self.coalesce_borrowed_proofs(&mut predecessors, span)?;
        let reached = self.join_views(predecessors, join, span)?;
        self.cb_join(control, join, reached, span)?;
        self.cr_join(control, join, reached, span)?;
        if !arm_views.is_empty() || !self.pending_views.is_empty() {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        self.pending_views = outer_pending_views;
        if !reached {
            return Ok(Lowered::Diverged);
        }
        Ok(result.map_or(Lowered::Unit, Lowered::Value))
    }

    /// Keep proofs needed after this control until the join compares its actual predecessors.
    pub(super) fn leave_continuing_branch(
        &mut self,
        inherited: &[super::View],
        span: Span,
        keep_owned_rc: bool,
    ) -> Result<(), LoweringError> {
        let mut views = std::mem::take(&mut self.active_views);
        let mut ending = Vec::new();
        views.retain(|view| {
            if inherited.iter().any(|outer| outer.slot == view.slot)
                || (self.shared_nullable_proof(view)
                    && self.inputs.borrowed_narrowing_used_after(view.symbol, span))
                || (keep_owned_rc
                    && view.when_scope.is_none()
                    && self.bindings.contains_key(&view.symbol)
                    && matches!(
                        self.body.slots[view.slot.0],
                        EntityType::Loan { kind: LoanKind::Shared, target }
                            if matches!(self.types.ssa_types.get(target.index()),
                                Some(crate::ssa::model::SsaTypeKind::SharedOwner { .. }))
                    )
                    && self.inputs.rc_narrowing_used_after(view.symbol, span))
            {
                true
            } else {
                ending.push(view.slot);
                false
            }
        });
        self.active_views = views;
        for loan in ending.into_iter().rev() {
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
        }
        Ok(())
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

#[cfg(test)]
#[path = "condition_receiver_ci_tests.rs"]
pub(in crate::ssa::refine) mod receiver_ci_tests;
