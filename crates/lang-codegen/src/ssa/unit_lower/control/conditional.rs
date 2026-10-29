//! Shared conditional CFG; only ordinary if opts into bounded proof retirement.
use super::*;

impl UnitExpressionLowerer<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_conditional(
        &mut self,
        expression: ExpressionId,
        condition: ExpressionId,
        then_branch: StatementId,
        else_branch: Option<StatementId>,
        then_index: usize,
        else_index: usize,
        span: Span,
        result: ControlResult,
    ) -> Result<LoweredValue, LoweringError> {
        let control = UnitExpressionId::new(self.source_unit, expression);
        if result.control != control {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let result_required = result.required;
        let ordinary_if = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .is_ok_and(|node| matches!(node.payload(), Expression::If { .. }));
        let nullable_if =
            match self.prepare_nullable_if_entry(control, condition, result_required, span)? {
                Some(entry) => Some(entry),
                None => {
                    self.prepare_map_nullable_if_entry(control, condition, result_required, span)?
                }
            };
        let guard = self.map_borrow_condition(condition, span)?;
        let baseline_guards = self.map_borrow_guards.clone();
        let condition_value = nullable_if
            .is_none()
            .then(|| self.require_expression_value(condition))
            .transpose()?;
        let map_checkpoint =
            self.checkpoint_map_conditional(control, result_required, else_branch.is_some(), span)?;
        let baseline = self.bindings.clone();
        let baseline_borrows = self.borrow_bindings.clone();
        let baseline_closures = self.closure_bindings.clone();
        let baseline_frames = self.pending_call_frames.clone();
        let baseline_receiver = self.current_receiver;
        let baseline_consumed_receiver = self.consumed_receiver;
        let mut carried = self.carried_bindings(&baseline, span)?;
        let mut carried_loans = self.carried_loans(&baseline_borrows, span)?;
        self.carry_pending_operands(&mut carried, &mut carried_loans, span)?;
        let then_span = self.statement_span(then_branch)?;
        let else_origin = else_branch
            .map(|branch| self.statement_span(branch))
            .transpose()?
            .unwrap_or(span);
        let (then_block, else_block, view) = if let Some(entry) = nullable_if {
            let (non_null, view) = self.add_map_borrow_block(
                &carried,
                &carried_loans,
                entry.inner,
                if entry.non_null_when_true {
                    then_span
                } else {
                    else_origin
                },
            )?;
            let null = self.add_carried_control_block(
                &carried,
                &carried_loans,
                if entry.non_null_when_true {
                    else_origin
                } else {
                    then_span
                },
            )?;
            if entry.non_null_when_true {
                (non_null, null, Some(view))
            } else {
                (null, non_null, Some(view))
            }
        } else {
            (
                self.add_carried_control_block(&carried, &carried_loans, then_span)?,
                self.add_carried_control_block(&carried, &carried_loans, else_origin)?,
                None,
            )
        };
        let header = self.block;
        let terminator = if let (Some(entry), Some(view)) = (nullable_if, view) {
            TerminatorKind::NullableBranch {
                owner: entry.owner,
                when_null: carried_control_edge(
                    if entry.non_null_when_true {
                        else_block
                    } else {
                        then_block
                    },
                    &carried,
                    &carried_loans,
                ),
                when_non_null: carried_control_edge(
                    if entry.non_null_when_true {
                        then_block
                    } else {
                        else_block
                    },
                    &carried,
                    &carried_loans,
                ),
                view,
            }
        } else {
            TerminatorKind::Conditional {
                condition: condition_value
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?,
                when_true: carried_control_edge(then_block, &carried, &carried_loans),
                when_false: carried_control_edge(else_block, &carried, &carried_loans),
            }
        };
        self.function
            .set_terminator(self.block, terminator, Origin::Source(span))
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;

        self.current_receiver = baseline_receiver;
        self.consumed_receiver = baseline_consumed_receiver;
        self.pending_call_frames = baseline_frames.clone();
        let (mut then_baseline, mut then_borrows) = if let Some((entry, view)) = nullable_if
            .zip(view)
            .filter(|(entry, _)| entry.non_null_when_true)
        {
            self.enter_nullable_if_branch(
                entry,
                header,
                then_block,
                view,
                &baseline,
                &carried,
                &carried_loans,
                condition,
                span,
            )?;
            (self.bindings.clone(), self.borrow_bindings.clone())
        } else {
            (
                self.rebind_carried_control(&baseline, then_block, &carried, &carried_loans, span)?,
                self.rebind_carried_loans(then_block, carried.len(), &carried_loans, span)?,
            )
        };
        if nullable_if.is_some_and(|entry| !entry.non_null_when_true) {
            self.block = then_block;
            self.bindings = then_baseline;
            self.borrow_bindings = then_borrows;
            self.emit_direct_nullable_condition_drops(condition, span)?;
            then_baseline = self.bindings.clone();
            then_borrows = self.borrow_bindings.clone();
        }
        let then_proofs = self.nullable_proofs.clone();
        let then_captures = self.capture_loans.clone();
        let then_pending = self.pending_operands.clone();
        let then_frames = self.pending_call_frames.clone();
        let then_temporaries = self.temporaries.clone();
        let then_sources = self.result_source_loans.clone();
        let then_receiver = self.current_receiver;
        let then_consumed_receiver = self.consumed_receiver;
        self.current_receiver = baseline_receiver;
        self.consumed_receiver = baseline_consumed_receiver;
        self.pending_call_frames = baseline_frames;
        let (mut else_baseline, mut else_borrows) = if let Some((entry, view)) = nullable_if
            .zip(view)
            .filter(|(entry, _)| !entry.non_null_when_true)
        {
            self.enter_nullable_if_branch(
                entry,
                header,
                else_block,
                view,
                &baseline,
                &carried,
                &carried_loans,
                condition,
                span,
            )?;
            (self.bindings.clone(), self.borrow_bindings.clone())
        } else {
            (
                self.rebind_carried_control(&baseline, else_block, &carried, &carried_loans, span)?,
                self.rebind_carried_loans(else_block, carried.len(), &carried_loans, span)?,
            )
        };
        if nullable_if.is_some_and(|entry| entry.non_null_when_true) {
            self.block = else_block;
            self.bindings = else_baseline;
            self.borrow_bindings = else_borrows;
            self.emit_direct_nullable_condition_drops(condition, span)?;
            else_baseline = self.bindings.clone();
            else_borrows = self.borrow_bindings.clone();
        }
        let else_proofs = self.nullable_proofs.clone();
        let else_captures = self.capture_loans.clone();
        let else_pending = self.pending_operands.clone();
        let else_frames = self.pending_call_frames.clone();
        let else_temporaries = self.temporaries.clone();
        let else_sources = self.result_source_loans.clone();
        let else_receiver = self.current_receiver;
        let else_consumed_receiver = self.consumed_receiver;
        let mut exits = Vec::with_capacity(2);
        if let Some(guard) = guard.filter(|guard| guard.non_null_when_true()) {
            self.map_borrow_guards.insert(guard.symbol(), guard);
        }
        self.enter_map_conditional_arm(&map_checkpoint, then_block, span)?;
        if let Some(exit) = self.lower_if_branch(
            then_block,
            then_branch,
            then_receiver,
            then_consumed_receiver,
            then_baseline,
            then_borrows,
            then_proofs,
            then_sources,
            baseline_closures.clone(),
            then_captures,
            then_pending,
            then_frames,
            then_temporaries,
            UnitDropPoint::BranchExit {
                control,
                branch: then_index,
            },
            result,
        )? {
            exits.push(exit);
        }
        self.map_borrow_guards.clone_from(&baseline_guards);
        if let Some(guard) = guard.filter(|guard| !guard.non_null_when_true()) {
            self.map_borrow_guards.insert(guard.symbol(), guard);
        }
        self.enter_map_conditional_arm(&map_checkpoint, else_block, span)?;
        if let Some(else_branch) = else_branch {
            if let Some(exit) = self.lower_if_branch(
                else_block,
                else_branch,
                else_receiver,
                else_consumed_receiver,
                else_baseline,
                else_borrows,
                else_proofs,
                else_sources.clone(),
                baseline_closures.clone(),
                else_captures,
                else_pending,
                else_frames,
                else_temporaries,
                UnitDropPoint::BranchExit {
                    control,
                    branch: else_index,
                },
                result,
            )? {
                exits.push(exit);
            }
        } else {
            if result_required {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            self.block = else_block;
            self.current_receiver = else_receiver;
            self.consumed_receiver = else_consumed_receiver;
            self.bindings = else_baseline;
            self.borrow_bindings = else_borrows;
            self.nullable_proofs = else_proofs;
            self.result_source_loans = else_sources;
            self.closure_bindings = baseline_closures;
            self.capture_loans = else_captures;
            self.pending_operands = else_pending;
            self.pending_call_frames = else_frames;
            self.temporaries = else_temporaries;
            self.emit_drops(UnitDropPoint::BranchExit {
                control,
                branch: else_index,
            })?;
            exits.push(BranchExit {
                block: else_block,
                result: LoweredValue::Unit,
                receiver: self.current_receiver,
                consumed_receiver: self.consumed_receiver,
                bindings: self.bindings.clone(),
                borrow_bindings: self.borrow_bindings.clone(),
                nullable_proofs: self.nullable_proofs.clone(),
                closure_bindings: self.closure_bindings.clone(),
                capture_loans: self.capture_loans.clone(),
                result_source_loans: self.result_source_loans.clone(),
                pending_operands: self.pending_operands.clone(),
                pending_call_frames: self.pending_call_frames.clone(),
                temporaries: self.temporaries.clone(),
            });
        }
        self.map_borrow_guards = baseline_guards;
        if ordinary_if {
            self.retire_if_exit_proofs(control, &mut exits, span)?;
        }
        let map_exits = self.map_conditional_exits(&map_checkpoint, &exits, span)?;
        let result = self.merge_unit_exits(exits, span)?;
        self.finish_map_conditional_merge(&map_checkpoint, map_exits, result, span)?;
        Ok(result)
    }
}
