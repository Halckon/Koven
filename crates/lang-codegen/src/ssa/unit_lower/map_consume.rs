//! Source-qualified owned-root delivery into a readonly Map owner.

use super::*;
use lang_frontend::type_checking::UnitCallReceiverOrigin;

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_delegated_call_receiver(
        &mut self,
        call: UnitExpressionId,
        receiver: Option<super::receiver::LoweredReceiver>,
        routes: &[crate::ssa::unit_plan::UnitDelegatedCallRoute],
        span: Span,
    ) -> Result<Option<super::receiver::LoweredReceiver>, LoweringError> {
        let Some(mut receiver) = receiver else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let fact = self
            .owned
            .receiver_fact(call)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if fact.source()
            == UnitCallReceiverOrigin::Expression(
                self.typed
                    .call(call)
                    .and_then(|d| d.receiver())
                    .map(|r| match r.origin() {
                        UnitCallReceiverOrigin::Expression(e) => e,
                        _ => call,
                    })
                    .unwrap_or(call),
            )
        {
            for route in routes {
                let delegate_target = *self
                    .type_ids
                    .get(&route.delegate_receiver())
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let field = *self
                    .field_indices
                    .get(&(route.outer_receiver(), route.field()))
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let EntityId::Loan(base) = receiver.entity else {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                };
                let (_, results) = self
                    .function
                    .append_instruction(
                        self.block,
                        Operation::SharedHeapFieldLoan { base, field },
                        vec![EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: delegate_target,
                        }],
                        Origin::Source(fact.begin_span()),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                let EntityId::Loan(delegate) = results[0] else {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                };
                receiver.entity = EntityId::Loan(delegate);
                receiver.created_loans.push((delegate, fact.end_span()));
            }
        }
        Ok(Some(receiver))
    }
}
