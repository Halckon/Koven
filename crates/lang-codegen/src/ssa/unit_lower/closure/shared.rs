//! Shared capture formation and owner-drop loan retirement for the legacy lowering path.
use super::*;
use crate::ssa::model::LoanId;

impl UnitExpressionLowerer<'_> {
    pub(in crate::ssa::unit_lower) fn form_shared_capture(
        &mut self,
        expression: UnitExpressionId,
        field: usize,
        capture: &CapturePlan,
    ) -> Result<LoanId, LoweringError> {
        // A second simultaneously live owner of the same source lambda needs a distinct instance
        // carrier. Keep that unsupported boundary explicit rather than overwrite its dependency.
        if self.capture_loans.contains_key(&(expression, field)) {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                capture.span,
            ));
        }
        let operation = if let Some(&source) = self.borrow_bindings.get(&capture.symbol) {
            Operation::SharedReborrow { source }
        } else {
            let Some(LoweredValue::Value(owner)) = self.bindings.get(&capture.symbol).copied()
            else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, capture.span));
            };
            let (_, places) = self
                .function
                .append_instruction(
                    self.block,
                    Operation::RootPlace { owner },
                    vec![EntityType::Place(capture.ty)],
                    Origin::Source(capture.span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, capture.span))?;
            let EntityId::Place(place) = places[0] else {
                return Err(lowering_error(
                    LoweringErrorKind::InvalidModel,
                    capture.span,
                ));
            };
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            }
        };
        let (_, loans) = self
            .function
            .append_instruction(
                self.block,
                operation,
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: capture.ty,
                }],
                Origin::Source(capture.span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, capture.span))?;
        let EntityId::Loan(loan) = loans[0] else {
            return Err(lowering_error(
                LoweringErrorKind::InvalidModel,
                capture.span,
            ));
        };
        self.capture_loans.insert((expression, field), loan);
        Ok(loan)
    }

    /// SSA Drop releases shared closure dependencies; only retire their carrier records here.
    pub(in crate::ssa::unit_lower) fn release_owner_capture_loans(
        &mut self,
        owner: crate::ssa::model::ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(EntityType::Value(ty)) =
            self.function.entity(EntityId::Value(owner)).map(|e| e.ty)
        else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        let Some(plan) = self
            .callable_plans
            .values()
            .find(|plan| plan.callable == ty)
        else {
            return Ok(());
        };
        for (field, capture) in plan.captures.iter().enumerate().rev() {
            if capture.mode == ClosureCaptureMode::Shared {
                self.capture_loans
                    .remove(&(plan.expression, field))
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            }
        }
        Ok(())
    }
}
