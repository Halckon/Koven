//! Private persistent-proof state; this transport slice does not create proof records.
use super::*;
use crate::ssa::model::SsaTypeKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NullableProof {
    pub(super) loan: LoanId,
    pub(super) owner: ValueId,
    pub(super) inner: SsaTypeId,
    pub(super) scope: UnitStatementId,
}

pub(super) type NullableProofs = BTreeMap<UnitSymbolId, NullableProof>;

impl UnitExpressionLowerer<'_> {
    /// Check the current storage identity, not merely two compatible semantic types.
    /// Birth/provenance validation belongs to the future explicit proof producer.
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
                || proof.scope.source_unit() != self.source_unit
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
