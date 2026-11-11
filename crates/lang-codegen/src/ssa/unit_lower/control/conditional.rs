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
            self.prepare_nullable_if_entry(control, condition, result_required, span)?;
        let condition_value = if nullable_if.is_none() {
            let Some(value) = self.lower_optional_condition_value(condition, span)? else {
                return Ok(LoweredValue::Diverged);
            };
            Some(value)
        } else {
            None
        };
        if ordinary_if && nullable_if.is_none() && self.constant_owned.is_none() {
            let (yes, no) = self
                .typed
                .ordinary_boolean_outcomes(UnitExpressionId::new(self.source_unit, condition));
            if yes != no {
                let (branch, index) = if yes {
                    (Some(then_branch), then_index)
                } else {
                    (else_branch, else_index)
                };
                let point = UnitDropPoint::BranchExit {
                    control,
                    branch: index,
                };
                let exit = if let Some(branch) = branch {
                    self.lower_current_if_branch(branch, point, result)?
                } else {
                    if result_required {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    }
                    self.emit_drops(point)?;
                    Some(self.loop_state())
                };
                let mut exits: Vec<_> = exit.into_iter().collect();
                self.retire_if_exit_proofs(control, &mut exits, span)?;
                return self.merge_unit_exits(exits, span);
            }
        }
        let baseline = self.bindings.clone();
        let baseline_borrows = self.borrow_bindings.clone();
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
        let then_pending = self.pending_operands.clone();
        let then_frames = self.pending_call_frames.clone();
        let then_temporaries = self.temporaries.clone();
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
        let else_pending = self.pending_operands.clone();
        let else_frames = self.pending_call_frames.clone();
        let else_temporaries = self.temporaries.clone();
        let else_receiver = self.current_receiver;
        let else_consumed_receiver = self.consumed_receiver;
        let mut exits = Vec::with_capacity(2);
        if let Some(exit) = self.lower_if_branch(
            then_block,
            then_branch,
            then_receiver,
            then_consumed_receiver,
            then_baseline,
            then_borrows,
            then_proofs,
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
        if let Some(else_branch) = else_branch {
            if let Some(exit) = self.lower_if_branch(
                else_block,
                else_branch,
                else_receiver,
                else_consumed_receiver,
                else_baseline,
                else_borrows,
                else_proofs,
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
                pending_operands: self.pending_operands.clone(),
                pending_call_frames: self.pending_call_frames.clone(),
                temporaries: self.temporaries.clone(),
            });
        }
        if ordinary_if {
            self.retire_if_exit_proofs(control, &mut exits, span)?;
        }
        self.merge_unit_exits(exits, span)
    }
}

pub(in crate::ssa::unit_lower) mod nullable_if {
    //! Bounded nullable entries: root Borrow and heap Ifs.
    #[cfg(test)]
    #[path = "nullable_if/field_tests.rs"]
    mod field_tests;
    #[path = "nullable_if/heap.rs"]
    mod heap;

    use super::super::super::unwrap_modified;
    use super::super::super::{
        ExpressionCategory, Statement, nullable_proof::NullableProofScope, span_key,
    };
    use super::super::*;
    use crate::ssa::model::{LoanId, SsaTypeKind};
    use lang_frontend::{
        ownership_checking::{LoanKind as FrontendLoanKind, UnitLoanTarget},
        parser::{BinaryOperator, Item},
        type_checking::{
            UnitCallableTarget, UnitExpressionId, UnitNullComparisonDescriptor, UnitTypeKind,
        },
    };

    #[derive(Clone, Copy)]
    pub(in super::super) struct NullableIfEntry {
        pub(in super::super) root: UnitSymbolId,
        pub(in super::super) owner: ValueId,
        pub(in super::super) inner: SsaTypeId,
        pub(in super::super) scope: UnitExpressionId,
        pub(in super::super) non_null_when_true: bool,
    }

    impl UnitExpressionLowerer<'_> {
        /// Reuse the proof entry only for a compatibility root Borrow heap If.
        pub(in super::super) fn prepare_nullable_if_entry(
            &self,
            control: UnitExpressionId,
            condition: ExpressionId,
            result_required: bool,
            span: Span,
        ) -> Result<Option<NullableIfEntry>, LoweringError> {
            if !self.nullable_proofs.is_empty() || result_required {
                return Ok(None);
            }
            // Nullable `String?` ifs lower only in `ssa::refine` (SPEC-0398); a mixed
            // compatibility body keeps its older inline-nullable handling.
            if !self.migrated_family_compatibility
                && self
                    .typed
                    .null_comparison(UnitExpressionId::new(self.source_unit, condition))
                    .is_some_and(|fact| {
                        matches!(
                            self.typed.types().get(fact.nullable_type()),
                            Some(UnitTypeKind::Nullable(inner))
                                if matches!(
                                    self.typed.types().get(*inner),
                                    Some(UnitTypeKind::Builtin(
                                        lang_frontend::type_checking::BuiltinType::String
                                    ))
                                )
                        )
                    })
            {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            if self.migrated_family_compatibility
                && let Some(entry) = self.prepare_heap_nullable_if_entry(control, condition, span)?
            {
                return Ok(Some(entry));
            }
            Ok(None)
        }

        pub(in crate::ssa::unit_lower) fn map_borrow_condition(
            &self,
            mut condition: ExpressionId,
            span: Span,
        ) -> Result<Option<UnitNullComparisonDescriptor>, LoweringError> {
            condition = self.ungroup_map_if_expression(condition, span)?;
            Ok(self
                .typed
                .null_comparison(UnitExpressionId::new(self.source_unit, condition)))
        }

        pub(in crate::ssa::unit_lower) fn ungroup_map_if_expression(
            &self,
            mut expression: ExpressionId,
            span: Span,
        ) -> Result<ExpressionId, LoweringError> {
            loop {
                let node = self
                    .parsed
                    .ast()
                    .expressions()
                    .get(expression)
                    .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
                match node.payload() {
                    Expression::Group { expression: inner } => expression = *inner,
                    _ => return Ok(expression),
                }
            }
        }

        pub(in crate::ssa::unit_lower) fn map_if_body_is_empty(
            &self,
            body: Option<StatementId>,
            span: Span,
        ) -> Result<bool, LoweringError> {
            let Some(body) = body else { return Ok(true) };
            let node = self
                .parsed
                .ast()
                .statements()
                .get(body)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            Ok(matches!(node.payload(), Statement::ControlBody { elements } if elements.is_empty()))
        }

        #[allow(clippy::too_many_arguments)]
        pub(in super::super) fn enter_nullable_if_branch(
            &mut self,
            entry: NullableIfEntry,
            header: BlockId,
            block: BlockId,
            view: LoanId,
            baseline: &BTreeMap<UnitSymbolId, LoweredValue>,
            carried: &[super::super::super::cfg::CarriedBinding],
            loans: &[super::super::super::cfg::CarriedAccess],
            condition: ExpressionId,
            span: Span,
        ) -> Result<(), LoweringError> {
            self.block = block;
            self.rebind_map_borrow_control(
                baseline,
                block,
                carried,
                loans,
                view,
                entry.inner,
                span,
            )?;
            let column = self.nullable_owner_column(carried, entry.root, entry.owner, span)?;
            self.introduce_nullable_proof(
                header,
                column,
                entry.root,
                entry.inner,
                NullableProofScope::If(entry.scope),
                span,
            )?;
            self.emit_direct_nullable_condition_drops(condition, span)
        }

        pub(in super::super::super) fn emit_direct_nullable_condition_drops(
            &mut self,
            condition: ExpressionId,
            span: Span,
        ) -> Result<(), LoweringError> {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(condition)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            match *node.payload() {
                Expression::Group { expression } => {
                    self.emit_direct_nullable_condition_drops(expression, span)?
                }
                Expression::Binary { left, right, .. } => {
                    self.emit_null_condition_drops(left)?;
                    self.emit_null_condition_drops(right)?;
                    self.emit_drops(UnitDropPoint::AfterBinaryOperands(UnitExpressionId::new(
                        self.source_unit,
                        condition,
                    )))?;
                }
                _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
            }
            self.emit_drops(UnitDropPoint::AfterExpression(UnitExpressionId::new(
                self.source_unit,
                condition,
            )))
        }

        /// Preflight every retirement against its own complete live exit before emission.
        /// Same-root origin disagreements remain errors, even if another root is absent.
        pub(in super::super) fn retire_if_exit_proofs(
            &mut self,
            control: UnitExpressionId,
            exits: &mut [BranchExit],
            span: Span,
        ) -> Result<(), LoweringError> {
            let mut identities = BTreeMap::new();
            for exit in exits.iter() {
                self.validate_nullable_proofs(&exit.bindings, &exit.nullable_proofs, span)?;
                for (&root, proof) in &exit.nullable_proofs {
                    if identities
                        .insert(root, (proof.origin, proof.inner))
                        .is_some_and(|previous| previous != (proof.origin, proof.inner))
                    {
                        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                    }
                }
            }
            let retirements = exits
                .iter()
                .map(|exit| {
                    exit.nullable_proofs
                        .iter()
                        .filter_map(|(&root, proof)| {
                            (proof.if_scope() == Some(control)
                                || exits
                                    .iter()
                                    .any(|other| !other.nullable_proofs.contains_key(&root)))
                            .then_some(root)
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            for (exit, roots) in exits.iter().zip(&retirements) {
                if !roots.is_empty()
                    && self
                        .function
                        .block(exit.block)
                        .is_none_or(|block| block.terminator.is_some())
                {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                }
                for root in roots {
                    let proof = exit
                        .nullable_proofs
                        .get(root)
                        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                    if proof.if_scope() != Some(control) {
                        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                    }
                }
                self.restore_loop_state(exit);
                for &root in roots {
                    self.validate_nullable_proof_retirement(root, span)?;
                }
            }
            for (exit, roots) in exits.iter_mut().zip(retirements) {
                self.restore_loop_state(exit);
                for root in roots {
                    self.end_nullable_proof(root, span)?;
                }
                // BorrowEnd changes only the proof map. Owners and all pending state stay exact.
                exit.nullable_proofs.clone_from(&self.nullable_proofs);
            }
            Ok(())
        }
    }
}
