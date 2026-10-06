//! Shared capture formation and thunk-local view cleanup share the ordinary CFG carriers.
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

    /// Follow children end before their parent slots; entry environment and parameters belong
    /// to the caller and are not ended here. IDs come from the current CFG state.
    pub(in crate::ssa::unit_lower) fn end_thunk_capture_views(
        &mut self,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(expression) = self.thunk_expression else {
            return Ok(());
        };
        let plan = self
            .callable_plans
            .get(&(self.closure_scope, expression))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        for (field, capture) in plan.captures.iter().enumerate().rev() {
            let loan = self
                .borrow_bindings
                .remove(&capture.symbol)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            self.function
                .append_instruction(
                    self.block,
                    Operation::BorrowEnd { loan },
                    Vec::new(),
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            if capture.mode == ClosureCaptureMode::Shared {
                let loan = self
                    .capture_loans
                    .remove(&(expression, field))
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                self.function
                    .append_instruction(
                        self.block,
                        Operation::BorrowEnd { loan },
                        Vec::new(),
                        Origin::Source(span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            }
        }
        Ok(())
    }
}
