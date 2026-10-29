//! ADR-0034 Borrow function arguments: a borrowed concrete callable is re-borrowed as the
//! callee's callable view; a forwarded view parameter is passed on unchanged.
use super::*;
use crate::ssa::model::LoanId;

impl UnitExpressionLowerer<'_> {
    pub(super) fn callable_view_argument(
        &mut self,
        loan: LoanId,
        created: &mut Vec<LoanId>,
        function: UnitTypeId,
        span: Span,
    ) -> Result<LoanId, LoweringError> {
        let view = self
            .callable_abi
            .view(function)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Some(EntityType::Loan { target, .. }) = self
            .function
            .entity(EntityId::Loan(loan))
            .map(|entity| entity.ty)
        else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        if target == view {
            return Ok(loan);
        }
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::CallableViewOf { source: loan },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: view,
                }],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let [EntityId::Loan(view_loan)] = results.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        created.push(*view_loan);
        Ok(*view_loan)
    }
}
