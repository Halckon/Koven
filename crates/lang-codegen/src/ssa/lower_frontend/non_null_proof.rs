//! Current-block identity for an existing nullable branch proof.
use std::collections::BTreeMap;

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::SymbolId,
    source::Span,
};

use super::control::BranchExit;
use super::{
    EntityId, EntityType, ExpressionLowerer, LoanId, LoanKind, LoweredValue, LoweringError,
    LoweringErrorKind, Operation, SsaTypeId, ValueId, error,
};
use crate::ssa::model::SsaTypeKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProofScope {
    If(ExpressionId),
    When(ExpressionId),
    While(StatementId),
    Assertion(ExpressionId),
}

/// Immutable branch identity, never a current SSA operand after rebinding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ProofOrigin {
    birth: LoanId,
    introduced_by: ProofScope,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NonNullProof {
    pub(super) loan: LoanId,
    pub(super) owner: ValueId,
    pub(super) inner: SsaTypeId,
    pub(super) origin: ProofOrigin,
}

impl NonNullProof {
    /// Call only with the real view and owner parameters of a NullableBranch successor.
    pub(super) fn introduced(
        loan: LoanId,
        owner: ValueId,
        inner: SsaTypeId,
        introduced_by: ProofScope,
    ) -> Self {
        Self {
            loan,
            owner,
            inner,
            origin: ProofOrigin {
                birth: loan,
                introduced_by,
            },
        }
    }

    pub(super) fn is_while_origin(self) -> bool {
        matches!(self.origin.introduced_by, ProofScope::While(_))
    }
}

impl ExpressionLowerer<'_> {
    pub(super) fn shared_owner_operand(
        &mut self,
        receiver: ExpressionId,
        span: Span,
    ) -> Result<EntityId, LoweringError> {
        if let Some(proof) = self.non_null_proof_for_expression(receiver, span)? {
            return Ok(EntityId::Loan(proof.loan));
        }
        self.require_value(receiver)
            .map(EntityId::Value)
            .map_err(|mut error| {
                error.span.get_or_insert(span);
                error
            })
    }

    pub(super) fn validate_non_null_proof_types(
        &self,
        proof: NonNullProof,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(EntityType::Value(nullable)) = self
            .function
            .entity(EntityId::Value(proof.owner))
            .map(|entity| entity.ty)
        else {
            return Err(error(LoweringErrorKind::MissingFact, span));
        };
        if !matches!(
            self.ssa_types.get(nullable.index()),
            Some(SsaTypeKind::NullableHandle { inner }) if *inner == proof.inner
        ) || self
            .function
            .entity(EntityId::Loan(proof.loan))
            .map(|entity| entity.ty)
            != Some(EntityType::Loan {
                kind: LoanKind::Shared,
                target: proof.inner,
            })
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    pub(super) fn validate_non_null_proof(
        &self,
        symbol: SymbolId,
        proof: NonNullProof,
        bindings: &BTreeMap<SymbolId, LoweredValue>,
        span: Span,
    ) -> Result<(), LoweringError> {
        if bindings.get(&symbol) != Some(&LoweredValue::Value(proof.owner)) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_non_null_proof_types(proof, span)
    }

    pub(super) fn checked_non_null_proof(
        &self,
        symbol: SymbolId,
        inner: SsaTypeId,
        span: Span,
    ) -> Result<Option<NonNullProof>, LoweringError> {
        let Some(proof) = self.non_null_bindings.get(&symbol).copied() else {
            return Ok(None);
        };
        self.validate_non_null_proof(symbol, proof, &self.bindings, span)?;
        if proof.inner != inner {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        Ok(Some(proof))
    }

    pub(super) fn end_non_null_proof(
        &mut self,
        symbol: SymbolId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(proof) = self.non_null_bindings.get(&symbol).copied() else {
            return Ok(());
        };
        self.validate_non_null_proof(symbol, proof, &self.bindings, span)?;
        if proof.is_while_origin() {
            self.ensure_no_unresolved_while_proof_carriers(span)?;
        }
        self.append(Operation::BorrowEnd { loan: proof.loan }, Vec::new(), span)?;
        self.non_null_bindings.remove(&symbol);
        Ok(())
    }

    pub(super) fn non_null_proof_for_expression(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<NonNullProof>, LoweringError> {
        let Some(non_null) = self.typed.non_null_use(expression) else {
            return Ok(None);
        };
        // Preserve the absent-proof fallback before requesting any concrete view type.
        if !self.non_null_bindings.contains_key(&non_null.symbol()) {
            return Ok(None);
        }
        self.checked_non_null_proof(
            non_null.symbol(),
            self.expression_ssa_type(expression, span)?,
            span,
        )
    }

    pub(super) fn validate_non_null_exit_proofs(
        &self,
        exits: &[BranchExit],
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(first) = exits.first() else {
            return Ok(());
        };
        for exit in exits {
            if exit.views.keys().ne(first.views.keys()) {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            for (&symbol, &proof) in &exit.views {
                self.validate_non_null_proof(symbol, proof, &exit.bindings, span)?;
                if proof.inner != first.views[&symbol].inner
                    || proof.origin != first.views[&symbol].origin
                {
                    return Err(error(LoweringErrorKind::MissingFact, span));
                }
            }
        }
        Ok(())
    }

    pub(super) fn merged_non_null_proofs(
        &self,
        bindings: &BTreeMap<SymbolId, LoweredValue>,
        parameters: Vec<(SymbolId, LoanId, NonNullProof)>,
        span: Span,
    ) -> Result<BTreeMap<SymbolId, NonNullProof>, LoweringError> {
        let mut views = BTreeMap::new();
        // Each incoming owner/proof pair was checked before constructing the merge edges.
        for (symbol, loan, source) in parameters {
            let Some(LoweredValue::Value(owner)) = bindings.get(&symbol).copied() else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            let proof = NonNullProof {
                loan,
                owner,
                ..source
            };
            self.validate_non_null_proof(symbol, proof, bindings, span)?;
            views.insert(symbol, proof);
        }
        Ok(views)
    }
}
