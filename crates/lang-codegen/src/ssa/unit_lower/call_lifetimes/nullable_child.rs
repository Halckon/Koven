//! Birth identity for a pending child; the pending slot owns its current SSA operand.
use super::super::nullable_proof::{NullableProofOrigin, NullableProofs};
use super::*;
use crate::ssa::model::SsaTypeKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in super::super) struct PendingNullableChild {
    pub(in super::super) call: UnitExpressionId,
    pub(in super::super) argument: UnitExpressionId,
    pub(in super::super) root: UnitSymbolId,
    pub(in super::super) parent_origin: NullableProofOrigin,
    pub(in super::super) parent_inner: SsaTypeId,
    pub(in super::super) field: usize,
    pub(in super::super) target: SsaTypeId,
    pub(in super::super) slot: usize,
}

pub(in super::super) struct CommittedNullableChild {
    identity: PendingNullableChild,
    loan: LoanId,
}

impl UnitExpressionLowerer<'_> {
    pub(in super::super) fn validate_pending_nullable_children(
        &self,
        frames: &[PendingCallFrame],
        pending: &[EntityId],
        proofs: &NullableProofs,
        span: Span,
    ) -> Result<(), LoweringError> {
        let mut slots = std::collections::BTreeSet::new();
        for frame in frames {
            for child in &frame.nullable_children {
                let Some(proof) = proofs.get(&child.root) else {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                };
                let Some(EntityId::Loan(loan)) = pending.get(child.slot).copied() else {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                };
                if child.call != frame.call
                    || child.call.source_unit() != self.source_unit
                    || child.argument.source_unit() != self.source_unit
                    || child.slot < frame.pending_start
                    || !slots.insert(child.slot)
                    || frame
                        .created_loans
                        .iter()
                        .filter(|slot| **slot == child.slot)
                        .count()
                        != 1
                    || frame
                        .loan_arguments
                        .iter()
                        .filter(|(argument, slots)| {
                            *argument == child.argument && slots.contains(&child.slot)
                        })
                        .count()
                        != 1
                    || !frame.shared_field_roots.contains(&child.root)
                    || proof.origin != child.parent_origin
                    || proof.inner != child.parent_inner
                    || loan == proof.loan
                    || self
                        .function
                        .entity(EntityId::Loan(loan))
                        .map(|entity| entity.ty)
                        != Some(EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: child.target,
                        })
                    || self.ssa_types.get(child.target.index()) != Some(&SsaTypeKind::StringOwner)
                    || !matches!(self.ssa_types.get(proof.inner.index()),
                        Some(SsaTypeKind::HeapOwner { payload: Some(payload), .. })
                        if self.heap_payloads.get(&proof.inner) == Some(payload)
                            && matches!(self.ssa_types.get(payload.index()),
                                Some(SsaTypeKind::Aggregate { fields, .. })
                                if fields.get(child.field) == Some(&child.target)))
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
            }
        }
        Ok(())
    }

    /// Only this one-child source prefix can enter a same-root readonly while.
    pub(in super::super) fn ensure_readonly_while_carriers(
        &self,
        root: UnitSymbolId,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.pending_operands.is_empty() {
            return self.ensure_no_while_proof_carriers(span);
        }
        self.ensure_while_proof_non_call_carriers(span)?;
        let [frame] = self.pending_call_frames.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let [child] = frame.nullable_children.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        if frame.receiver
            || frame.pending_start != 0
            || child.root != root
            || child.slot != 0
            || self.pending_operands.len() != 1
            || frame.created_loans.as_slice() != [child.slot]
            || frame.loan_arguments.as_slice() != [(child.argument, vec![child.slot])]
            || frame.shared_field_roots.as_slice() != [root]
            || !frame.abi_slots.is_empty()
            || frame.field_replace_owner.is_some()
            || !frame.exclusive_root_owners.is_empty()
            || !self.temporaries.is_empty()
            || !self.typed.call(frame.call).is_some_and(|call| {
                call.expression() == frame.call
                    && call.receiver().is_none()
                    && call.result_source()
                        == lang_frontend::type_checking::CallableResultSource::Owned
                    && matches!(
                        call.target(),
                        lang_frontend::type_checking::UnitCallTarget::Declaration(_)
                    )
            })
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
        self.validate_pending_nullable_children(
            &self.pending_call_frames,
            &self.pending_operands,
            &self.nullable_proofs,
            span,
        )
    }

    /// Capture current operands before the argument frame and its pending suffix disappear.
    pub(in super::super) fn commit_pending_nullable_children(
        &self,
        call: UnitExpressionId,
        arguments: &[EntityId],
        argument_loans: &BTreeMap<UnitExpressionId, Vec<LoanId>>,
        created_loans: &[(LoanId, Span)],
        span: Span,
    ) -> Result<Vec<CommittedNullableChild>, LoweringError> {
        self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
        self.validate_pending_nullable_children(
            &self.pending_call_frames,
            &self.pending_operands,
            &self.nullable_proofs,
            span,
        )?;
        let frame = self
            .pending_call_frames
            .last()
            .filter(|frame| frame.call == call && !frame.receiver)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        frame
            .nullable_children
            .iter()
            .map(|child| {
                let Some(EntityId::Loan(loan)) = self.pending_operands.get(child.slot).copied()
                else {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                };
                if arguments
                    .iter()
                    .filter(|entity| **entity == EntityId::Loan(loan))
                    .count()
                    != 1
                    || argument_loans.get(&child.argument).is_none_or(|loans| {
                        loans.iter().filter(|candidate| **candidate == loan).count() != 1
                    })
                    || created_loans
                        .iter()
                        .filter(|(candidate, _)| *candidate == loan)
                        .count()
                        != 1
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
                Ok(CommittedNullableChild {
                    identity: *child,
                    loan,
                })
            })
            .collect()
    }

    /// Consume the committed evidence at the existing call's actual BorrowEnd.
    pub(in super::super) fn end_committed_call_loans(
        &mut self,
        call: UnitExpressionId,
        created_loans: Vec<(LoanId, Span)>,
        argument_loans: BTreeMap<UnitExpressionId, Vec<LoanId>>,
        mut children: Vec<CommittedNullableChild>,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
        for child in &children {
            if child.identity.call != call
                || self
                    .nullable_proofs
                    .get(&child.identity.root)
                    .is_none_or(|proof| {
                        proof.origin != child.identity.parent_origin
                            || proof.inner != child.identity.parent_inner
                    })
                || argument_loans
                    .get(&child.identity.argument)
                    .is_none_or(|loans| {
                        loans.iter().filter(|loan| **loan == child.loan).count() != 1
                    })
                || created_loans
                    .iter()
                    .filter(|(loan, _)| *loan == child.loan)
                    .count()
                    != 1
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        for (loan, end_span) in created_loans.into_iter().rev() {
            self.end_short_call_loan(loan, end_span)?;
            if let Some(index) = children.iter().position(|child| child.loan == loan) {
                children.remove(index);
            }
        }
        if !children.is_empty() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }
}
