//! Private proof identity, distinct from the nullable root's owned storage.
use super::*;
use crate::ssa::model::SsaTypeKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NullableProofOrigin {
    scope: UnitStatementId,
    birth: LoanId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NullableProof {
    pub(super) loan: LoanId,
    pub(super) owner: ValueId,
    pub(super) inner: SsaTypeId,
    pub(super) origin: NullableProofOrigin,
}

impl NullableProof {
    pub(super) fn scope(self) -> UnitStatementId {
        self.origin.scope
    }
}

pub(super) type NullableProofs = BTreeMap<UnitSymbolId, NullableProof>;

impl UnitExpressionLowerer<'_> {
    /// Only the actual NullableBranch view and its owner column can mint a proof.
    pub(super) fn introduce_while_proof(
        &mut self,
        header: BlockId,
        root: UnitSymbolId,
        owner_column: usize,
        inner: SsaTypeId,
        scope: UnitStatementId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(TerminatorKind::NullableBranch {
            owner,
            when_null,
            when_non_null,
            view,
        }) = self
            .function
            .block(header)
            .and_then(|block| block.terminator.as_ref())
            .map(|terminator| &terminator.kind)
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let parameters = &self
            .function
            .block(self.block)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
            .parameters;
        let Some(EntityId::Value(body_owner)) = parameters.get(owner_column) else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if when_non_null.target != self.block
            || when_null.target == self.block
            || when_null.arguments.get(owner_column) != Some(&EntityId::Value(*owner))
            || when_non_null.arguments.get(owner_column) != Some(&EntityId::Value(*owner))
            || parameters.last() != Some(&EntityId::Loan(*view))
            || parameters.len() != when_non_null.arguments.len() + 1
            || !self.nullable_proofs.is_empty()
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let proof = NullableProof {
            loan: *view,
            owner: *body_owner,
            inner,
            origin: NullableProofOrigin {
                scope,
                birth: *view,
            },
        };
        let proofs = BTreeMap::from([(root, proof)]);
        self.validate_nullable_proofs(&self.bindings, &proofs, span)?;
        self.nullable_proofs = proofs;
        Ok(())
    }

    /// Check the current storage identity, not merely two compatible semantic types.
    /// Producers and transport validate provenance before installing these records.
    pub(super) fn validate_nullable_proofs(
        &self,
        bindings: &BTreeMap<UnitSymbolId, LoweredValue>,
        proofs: &NullableProofs,
        span: Span,
    ) -> Result<(), LoweringError> {
        let mut identities = BTreeMap::new();
        for (root, proof) in proofs {
            let owner_type = self
                .function
                .entity(EntityId::Value(proof.owner))
                .map(|entity| entity.ty);
            let identity = (proof.owner, proof.inner);
            if identities
                .insert(proof.loan, identity)
                .is_some_and(|old| old != identity)
                || root.source_unit() != self.source_unit
                || proof.scope().source_unit() != self.source_unit
                || !self.function.blocks.iter().any(|block| {
                    matches!(block.terminator.as_ref().map(|terminator| &terminator.kind),
                        Some(TerminatorKind::NullableBranch { view, .. }) if *view == proof.origin.birth)
                })
                || bindings.get(root) != Some(&LoweredValue::Value(proof.owner))
                || !matches!(owner_type, Some(EntityType::Value(ty))
                    if self.ssa_types.get(ty.index())
                        == Some(&SsaTypeKind::NullableHandle { inner: proof.inner }))
                || self
                    .function
                    .entity(EntityId::Loan(proof.loan))
                    .map(|entity| entity.ty)
                    != Some(EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: proof.inner,
                    })
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        Ok(())
    }

    pub(super) fn rebind_assertion_nullable_proofs(
        &mut self,
        proofs: &NullableProofs,
        entities: &BTreeMap<EntityId, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let mut rebound = NullableProofs::new();
        for (root, proof) in proofs {
            let Some(EntityId::Loan(loan)) = entities.get(&EntityId::Loan(proof.loan)) else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            let Some(EntityId::Value(owner)) = entities.get(&EntityId::Value(proof.owner)) else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            rebound.insert(
                *root,
                NullableProof {
                    loan: *loan,
                    owner: *owner,
                    ..*proof
                },
            );
        }
        self.validate_nullable_proofs(&self.bindings, &rebound, span)?;
        self.nullable_proofs = rebound;
        Ok(())
    }
}
