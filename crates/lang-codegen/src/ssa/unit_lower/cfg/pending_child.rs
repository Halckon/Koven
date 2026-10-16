//! Pending child roles reuse the existing owner, proof and pending access columns.
#[cfg(test)]
mod guard_tests;

use super::super::call_lifetimes::{PendingCallFrame, nullable_child::PendingNullableChild};
use super::*;

#[derive(Clone)]
pub(super) struct CarriedNullableChild {
    child: PendingNullableChild,
    frames: Vec<PendingCallFrame>,
    owner_column: usize,
    parent_column: usize,
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn bind_pending_child_columns(
        &self,
        bindings: &[CarriedBinding],
        loans: &mut [CarriedAccess],
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_pending_nullable_children(
            &self.pending_call_frames,
            &self.pending_operands,
            &self.nullable_proofs,
            span,
        )?;
        for slot in loans.iter_mut() {
            slot.nullable_children.clear();
        }
        for frame in &self.pending_call_frames {
            for child in &frame.nullable_children {
                let proof = self
                    .nullable_proofs
                    .get(&child.root)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let owner_column =
                    self.nullable_owner_column(bindings, child.root, proof.owner, span)?;
                let parent_column = loans
                    .iter()
                    .position(|slot| {
                        slot.source == EntityId::Loan(proof.loan)
                            && slot
                                .nullable_proofs
                                .iter()
                                .any(|role| role.root == child.root && role.proof == *proof)
                    })
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let entity = self.pending_operands[child.slot];
                let slot = loans
                    .iter_mut()
                    .find(|slot| slot.source == entity && slot.pending.contains(&child.slot))
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                slot.nullable_children.push(CarriedNullableChild {
                    child: *child,
                    frames: self.pending_call_frames.clone(),
                    owner_column,
                    parent_column,
                });
            }
        }
        Ok(())
    }

    pub(super) fn validate_pending_child_edge_columns(
        &self,
        bindings: &[CarriedBinding],
        loans: &[CarriedAccess],
        state: &BranchExit,
        arguments: &[EntityId],
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_pending_nullable_children(
            &state.pending_call_frames,
            &state.pending_operands,
            &state.nullable_proofs,
            span,
        )?;
        let expected = state
            .pending_call_frames
            .iter()
            .map(|frame| frame.nullable_children.len())
            .sum::<usize>();
        let mut checked = BTreeSet::new();
        for (column, slot) in loans.iter().enumerate() {
            for role in &slot.nullable_children {
                let child = &role.child;
                let proof = state
                    .nullable_proofs
                    .get(&child.root)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                if role.frames != state.pending_call_frames
                    || !checked.insert(child.slot)
                    || proof.origin != child.parent_origin
                    || proof.inner != child.parent_inner
                    || bindings
                        .get(role.owner_column)
                        .is_none_or(|slot| slot.symbol != Some(child.root))
                    || loans.get(role.parent_column).is_none_or(|slot| {
                        !slot.nullable_proofs.iter().any(|role| {
                            role.root == child.root
                                && role.proof.origin == child.parent_origin
                                && role.proof.inner == child.parent_inner
                        })
                    })
                    || arguments.get(role.owner_column) != Some(&EntityId::Value(proof.owner))
                    || arguments.get(bindings.len() + role.parent_column)
                        != Some(&EntityId::Loan(proof.loan))
                    || arguments.get(bindings.len() + column)
                        != state.pending_operands.get(child.slot)
                    || !slot.pending.contains(&child.slot)
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
            }
        }
        if checked.len() != expected {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    pub(super) fn validate_rebound_pending_children(
        &self,
        block: BlockId,
        bindings: &BTreeMap<UnitSymbolId, LoweredValue>,
        carried: &[CarriedBinding],
        loans: &[CarriedAccess],
        span: Span,
    ) -> Result<(), LoweringError> {
        let parameters = &self
            .function
            .block(block)
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
            .parameters;
        let state = BranchExit {
            block,
            result: LoweredValue::Unit,
            receiver: self.current_receiver,
            consumed_receiver: self.consumed_receiver,
            bindings: bindings.clone(),
            borrow_bindings: self.borrow_bindings.clone(),
            nullable_proofs: self.nullable_proofs.clone(),
            pending_operands: self.pending_operands.clone(),
            pending_call_frames: self.pending_call_frames.clone(),
            temporaries: self.temporaries.clone(),
            closure_bindings: self.closure_bindings.clone(),
            capture_loans: self.capture_loans.clone(),
            result_source_loans: self.result_source_loans.clone(),
        };
        self.validate_pending_child_edge_columns(carried, loans, &state, parameters, span)
    }

    pub(super) fn validate_pending_child_exits(
        &self,
        exits: &[BranchExit],
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(first) = exits.first() else {
            return Ok(());
        };
        for exit in exits {
            self.validate_pending_nullable_children(
                &exit.pending_call_frames,
                &exit.pending_operands,
                &exit.nullable_proofs,
                span,
            )?;
            if exit.pending_call_frames != first.pending_call_frames {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn validate_pending_child_merge_columns(
        &self,
        exits: &[BranchExit],
        arguments: &[Vec<EntityId>],
        owners: &[UnitSymbolId],
        owner_offset: usize,
        pending_columns: &[usize],
        proof_columns: &[usize],
        span: Span,
    ) -> Result<(), LoweringError> {
        for (exit, arguments) in exits.iter().zip(arguments) {
            for frame in &exit.pending_call_frames {
                for child in &frame.nullable_children {
                    let proof = exit
                        .nullable_proofs
                        .get(&child.root)
                        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                    let owner_column = owners
                        .iter()
                        .position(|root| *root == child.root)
                        .map(|column| owner_offset + column);
                    let parent_column = exit
                        .nullable_proofs
                        .keys()
                        .position(|root| *root == child.root)
                        .and_then(|index| proof_columns.get(index));
                    let child_column = pending_columns.get(child.slot);
                    if owner_column.and_then(|column| arguments.get(column))
                        != Some(&EntityId::Value(proof.owner))
                        || parent_column.and_then(|column| arguments.get(*column))
                            != Some(&EntityId::Loan(proof.loan))
                        || child_column.and_then(|column| arguments.get(*column))
                            != exit.pending_operands.get(child.slot)
                    {
                        return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                    }
                }
            }
        }
        Ok(())
    }
}
