//! Private proof identity, distinct from the nullable root's owned storage.
use super::*;
use crate::ssa::model::{Ownership, SsaTypeKind};

#[cfg(test)]
pub(super) mod guard_tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NullableProofScope {
    If(UnitExpressionId),
    While(UnitStatementId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NullableProofOrigin {
    scope: NullableProofScope,
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
    pub(super) fn while_scope(self) -> Option<UnitStatementId> {
        match self.origin.scope {
            NullableProofScope::While(scope) => Some(scope),
            NullableProofScope::If(_) => None,
        }
    }

    pub(super) fn if_scope(self) -> Option<UnitExpressionId> {
        match self.origin.scope {
            NullableProofScope::If(scope) => Some(scope),
            NullableProofScope::While(_) => None,
        }
    }
}

pub(super) type NullableProofs = BTreeMap<UnitSymbolId, NullableProof>;

impl UnitExpressionLowerer<'_> {
    /// If retests currently support only local reads and normal branch completion.
    pub(super) fn reject_live_if_proof(&self, span: Span) -> Result<(), LoweringError> {
        if self
            .nullable_proofs
            .values()
            .any(|proof| proof.if_scope().is_some())
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    /// Only the actual NullableBranch view and its owner column can mint a proof.
    pub(super) fn introduce_while_proof(
        &mut self,
        header: BlockId,
        owner_column: usize,
        entry: &loop_control::DirectNullableWhileEntry,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_direct_while_entry(entry, span)?;
        self.introduce_nullable_proof(
            header,
            owner_column,
            entry.root(),
            entry.inner(),
            NullableProofScope::While(entry.scope()),
            span,
        )
    }

    /// Both scopes use the same actual non-null successor and owner-column check.
    pub(super) fn introduce_nullable_proof(
        &mut self,
        header: BlockId,
        owner_column: usize,
        root: UnitSymbolId,
        inner: SsaTypeId,
        scope: NullableProofScope,
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
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if self
            .nullable_proofs
            .values()
            .any(|outer| outer.loan == *view || outer.origin.scope == scope)
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
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
        // Keep only the actual rebound outer candidate validated above. Never
        // reinstall the qualification's preheader owner/loan IDs at this birth.
        let mut proofs = self.nullable_proofs.clone();
        if proofs.insert(root, proof).is_some() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
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
            let valid_storage = match owner_type {
                Some(EntityType::Value(storage)) => match self.ssa_types.get(storage.index()) {
                    Some(SsaTypeKind::NullableHandle { inner }) => *inner == proof.inner,
                    Some(SsaTypeKind::Aggregate {
                        ownership: Ownership::MoveOnly,
                        ..
                    }) if proof.if_scope().is_some() => {
                        // Only registered nullable Map storage gains this ordinary If view.
                        self.nullable_maps.get(&storage) == Some(&proof.inner)
                            && matches!(
                                self.ssa_types.get(proof.inner.index()),
                                Some(SsaTypeKind::MapContainer { .. })
                            )
                    }
                    _ => false,
                },
                _ => false,
            };
            let identity = (proof.owner, proof.inner);
            if identities
                .insert(proof.loan, identity)
                .is_some_and(|old| old != identity)
                || root.source_unit() != self.source_unit
                || match proof.origin.scope {
                    NullableProofScope::If(scope) => scope.source_unit() != self.source_unit
                        || !self.parsed.ast().expressions().get(scope.expression())
                            .is_ok_and(|node| matches!(node.payload(), Expression::If { .. })),
                    NullableProofScope::While(scope) => scope.source_unit() != self.source_unit,
                }
                || !self.function.blocks.iter().any(|block| {
                    matches!(block.terminator.as_ref().map(|terminator| &terminator.kind),
                        Some(TerminatorKind::NullableBranch { view, .. }) if *view == proof.origin.birth)
                })
                || bindings.get(root) != Some(&LoweredValue::Value(proof.owner))
                || !valid_storage
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
