//! Real nullable When entry, two independent successors and ordinary CFG merge.
mod source;
mod value;

use super::control::ControlResult;
use super::*;
use crate::ssa::model::SsaTypeKind;
use lang_frontend::{
    ownership_checking::{UnitDropTarget, UnitLoanTarget},
    parser::{WhenCondition, WhenEntry},
    type_checking::{
        NullableWhenSubjectCategory, UnitNullableWhenDescriptor, UnitNullableWhenDomain,
        UnitNullableWhenUseWitness,
    },
};

/// The source version is frozen by the typed subject, never sampled in the body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NullableWhenScope {
    control: UnitExpressionId,
    entry: usize,
    generation: u64,
    loop_depth: usize,
}

struct NullableWhenSource {
    operands: Vec<UnitExpressionId>,
    root: Option<UnitSymbolId>,
    inner: SsaTypeId,
    nullable: SsaTypeId,
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_nullable_when(
        &mut self,
        control: UnitExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .typed
            .nullable_when(control)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
            .clone();
        let source = self.prepare_nullable_when_source(&descriptor, span)?;
        let result_plan = self.control_result(control.expression(), None, span)?;
        let result_type = self
            .typed
            .expression_type(control)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if self.typed.copyability(result_type) == Copyability::MoveOnly {
            // This slice does not turn a When tail into an implicit owned extraction.
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let checkpoint = self.loop_state();
        let result = self.lower_nullable_when_source(&descriptor, &source, result_plan, span);
        if result.is_err() {
            // No failed entry leaves an installed proof or half-retired binding.
            // The enclosing lowering error discards the unpublished SSA function.
            self.restore_loop_state(&checkpoint);
        }
        result
    }

    fn lower_nullable_when_source(
        &mut self,
        descriptor: &UnitNullableWhenDescriptor,
        source: &NullableWhenSource,
        result_plan: ControlResult,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let owner = match self.evaluate_nullable_when_subject(descriptor, source, span)? {
            LoweredValue::Value(owner) => owner,
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Unit => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        let baseline = self.loop_state();
        let mut carried = self.carried_bindings(&baseline.bindings, span)?;
        let mut loans = self.carried_loans(&baseline.borrow_bindings, span)?;
        self.carry_pending_operands(&mut carried, &mut loans, span)?;
        let column = match source.root {
            Some(root) => self.nullable_owner_column(&carried, root, owner, span)?,
            None => {
                self.nullable_temporary_owner_column(&carried, descriptor.subject(), owner, span)?
            }
        };
        let null =
            self.add_carried_control_block(&carried, &loans, descriptor.entries()[0].span())?;
        let (non_null, view) = self.add_map_borrow_block(
            &carried,
            &loans,
            source.inner,
            descriptor.entries()[1].span(),
        )?;
        let header = self.block;
        self.function
            .set_terminator(
                header,
                TerminatorKind::NullableBranch {
                    owner,
                    when_null: cfg::carried_control_edge(null, &carried, &loans),
                    when_non_null: cfg::carried_control_edge(non_null, &carried, &loans),
                    view,
                },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let mut exits = Vec::with_capacity(2);
        for (index, block) in [(0, null), (1, non_null)] {
            // Never start the second arm with the first arm's state or transfer effects.
            self.restore_loop_state(&baseline);
            self.block = block;
            if index == 1 {
                self.rebind_map_borrow_control(
                    &baseline.bindings,
                    block,
                    &carried,
                    &loans,
                    view,
                    source.inner,
                    span,
                )?;
                if let Some(root) = source.root {
                    let generation = descriptor
                        .source_generation()
                        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                    self.introduce_nullable_proof(
                        header,
                        column,
                        root,
                        source.inner,
                        nullable_proof::NullableProofScope::When(NullableWhenScope {
                            control: descriptor.expression(),
                            entry: index,
                            generation,
                            loop_depth: self.loops.len(),
                        }),
                        span,
                    )?;
                } else {
                    let (actual_owner, actual_view) =
                        self.nullable_branch_view_owner(header, column, source.inner, span)?;
                    if actual_view != view
                        || self.temporaries.get(&descriptor.subject()) != Some(&actual_owner)
                    {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    }
                    // No source symbol/use exists for a temporary subject. Its unused
                    // view can end now, while the real owner lives to the When boundary.
                    self.function
                        .append_instruction(
                            block,
                            Operation::BorrowEnd { loan: actual_view },
                            Vec::new(),
                            Origin::Source(span),
                        )
                        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                }
            } else {
                self.bindings =
                    self.rebind_carried_control(&baseline.bindings, block, &carried, &loans, span)?;
                self.borrow_bindings =
                    self.rebind_carried_loans(block, carried.len(), &loans, span)?;
            }
            self.emit_nullable_when_operand_drops(&source.operands)?;
            let entry = &descriptor.entries()[index];
            let before = self.current_loop_exit_counts();
            let branch = self.lower_if_branch(
                block,
                entry.body().statement(),
                self.current_receiver,
                self.consumed_receiver,
                self.bindings.clone(),
                self.borrow_bindings.clone(),
                self.nullable_proofs.clone(),
                self.pending_operands.clone(),
                self.pending_call_frames.clone(),
                self.temporaries.clone(),
                UnitDropPoint::BranchExit {
                    control: descriptor.expression(),
                    branch: index,
                },
                result_plan,
            )?;
            let owned_entry = self
                .owned
                .nullable_when(descriptor.expression())
                .and_then(|plan| plan.entries().get(index))
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let after = self.current_loop_exit_counts();
            if branch.is_some() != owned_entry.falls_through()
                || (after.0 > before.0) != owned_entry.breaks()
                || (after.1 > before.1) != owned_entry.continues()
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, entry.span()));
            }
            if let Some(mut exit) = branch {
                self.restore_loop_state(&exit);
                self.retire_nullable_when_scope(descriptor.expression(), span)?;
                exit.nullable_proofs.clone_from(&self.nullable_proofs);
                exits.push(exit);
            }
        }
        // Empty exits use the existing double-escaping cleanup; no synthetic merge.
        self.merge_unit_exits(exits, span)
    }

    pub(super) fn valid_nullable_when_scope(
        &self,
        root: UnitSymbolId,
        scope: NullableWhenScope,
        inner: SsaTypeId,
    ) -> bool {
        let Some(descriptor) = self.typed.nullable_when(scope.control) else {
            return false;
        };
        descriptor.expression() == scope.control
            && scope.control.source_unit() == self.source_unit
            && descriptor.stable_symbol() == Some(root)
            && descriptor.source_generation() == Some(scope.generation)
            && descriptor.source_category() == NullableWhenSubjectCategory::OwnedRoot
            && self.typed.symbol_type(root) == Some(descriptor.nullable_type())
            && resolve_concrete_type(
                self.typed,
                descriptor.inner_type(),
                self.substitutions,
                self.static_self,
                self.function.origin.span(),
            )
            .is_ok_and(|ty| self.type_ids.get(&ty) == Some(&inner))
            && scope.loop_depth <= self.loops.len()
            && descriptor.entries().get(scope.entry).is_some_and(|entry| {
                entry.index() == scope.entry
                    && entry.body().source_unit() == self.source_unit
                    && entry.body_domain() == UnitNullableWhenDomain::NonNull
            })
            && self.owned.nullable_when(scope.control).is_some_and(|plan| {
                plan.descriptor() == descriptor
                    && plan
                        .source_place()
                        .is_some_and(|path| path.is_root() && path.root() == root)
            })
    }
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn emit_nullable_when_operand_drops(
        &mut self,
        operands: &[UnitExpressionId],
    ) -> Result<(), LoweringError> {
        for operand in operands.iter().rev() {
            self.emit_drops(UnitDropPoint::AfterExpression(*operand))?;
        }
        Ok(())
    }

    pub(super) fn validate_nullable_when_retirement(
        &self,
        root: UnitSymbolId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let proof = self
            .nullable_proofs
            .get(&root)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if proof.when_scope().is_none() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
        let aliases_owner = |entity| entity == EntityId::Value(proof.owner);
        let aliases_view = |entity| entity == EntityId::Loan(proof.loan);
        if self
            .pending_operands
            .iter()
            .copied()
            .any(|entity| aliases_owner(entity) || aliases_view(entity))
            || self.current_receiver.is_some_and(|receiver| {
                aliases_owner(receiver.entity) || aliases_view(receiver.entity)
            })
            || self
                .borrow_bindings
                .values()
                .any(|loan| *loan == proof.loan)
            || self.nullable_proofs.iter().any(|(other, candidate)| {
                *other != root && (candidate.loan == proof.loan || candidate.owner == proof.owner)
            })
            || self
                .bindings
                .iter()
                .any(|(other, value)| *other != root && *value == LoweredValue::Value(proof.owner))
            || self.pending_call_frames.iter().any(|frame| {
                self.pending_operands.len() > frame.pending_start
                    && frame.loan_arguments.iter().any(|(argument, _)| {
                        self.owned.loans().iter().any(|fact| {
                            fact.call() == frame.call
                                && fact.argument() == *argument
                                && matches!(fact.target(), UnitLoanTarget::Place(path)
                                    if path.root() == root)
                        })
                    })
            })
        {
            // This checks dependency, not global carrier emptiness. An outer call
            // prefix with a different owner/loan survives normal and loop exits.
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    pub(super) fn retire_nullable_when_scope(
        &mut self,
        control: UnitExpressionId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let roots = self
            .nullable_proofs
            .iter()
            .filter_map(|(&root, proof)| {
                proof
                    .when_scope()
                    .is_some_and(|scope| scope.control == control)
                    .then_some(root)
            })
            .collect::<Vec<_>>();
        for &root in &roots {
            self.validate_nullable_when_retirement(root, span)?;
        }
        for root in roots {
            self.end_nullable_proof(root, span)?;
        }
        Ok(())
    }

    /// Called after exiting call-frame children and exact ControlTransfer drops.
    /// Birth depth distinguishes a leaving When from any retained outer scope.
    pub(super) fn retire_nullable_when_transfers(
        &mut self,
        minimum_loop_depth: usize,
        span: Span,
    ) -> Result<(), LoweringError> {
        let roots = self
            .nullable_proofs
            .iter()
            .filter_map(|(&root, proof)| {
                proof
                    .when_scope()
                    .is_some_and(|scope| scope.loop_depth >= minimum_loop_depth)
                    .then_some(root)
            })
            .collect::<Vec<_>>();
        for &root in &roots {
            self.validate_nullable_when_retirement(root, span)?;
        }
        for root in roots {
            self.end_nullable_proof(root, span)?;
        }
        Ok(())
    }

    /// The producer's pure Name/Group Consume path only takes the named state.
    /// Check every original point, leaf first, before any SSA or owner mutation.
    /// Empty drop facts also make closure validation and late-Map drop dispatch
    /// structurally empty; empty end facts make end_result_bindings a no-op.
    pub(in crate::ssa::unit_lower) fn preflight_nullable_take_drops(
        &self,
        operands: &[UnitExpressionId],
        span: Span,
    ) -> Result<(), LoweringError> {
        for operand in operands.iter().rev() {
            let point = UnitDropPoint::AfterExpression(*operand);
            if self.owned.drops().iter().any(|fact| fact.point() == point)
                || self
                    .owned
                    .borrow_results()
                    .ends()
                    .iter()
                    .any(|fact| fact.point() == point)
                || self.owned.iteration_cleanup_at(point).is_some()
                || self
                    .owned
                    .conditional_receiver_drops()
                    .iter()
                    .any(|fact| fact.point() == point)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        Ok(())
    }
}
