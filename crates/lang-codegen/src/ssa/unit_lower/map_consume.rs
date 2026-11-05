//! Source-qualified owned-root delivery into a readonly Map owner.

pub(super) mod nullable;

use super::*;
use crate::ssa::model::{MapContainerKind, SsaTypeKind};
use lang_frontend::{
    ownership_checking::{UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget},
    type_checking::{IntrinsicTypeConstructor, UnitCallReceiverOrigin},
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_map_consume(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let descriptor = self
            .typed
            .map_consume(id)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let (root, storage) = descriptor
            .source()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if storage != descriptor.receiver_type() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&root).copied() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let delivered = self.lower(descriptor.receiver().expression())?;
        if delivered != LoweredValue::Value(owner) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.take_owned_binding(root, owner, span)?;
        self.bindings
            .retain(|_, binding| *binding != LoweredValue::Value(owner));
        self.temporaries.retain(|_, temporary| *temporary != owner);
        let map_type = self.expression_ssa_type(expression, span)?;
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::MapConsume { owner, map_type },
                vec![EntityType::Value(map_type)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let [EntityId::Value(result)] = results.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        self.emit_drops(UnitDropPoint::CallReturn(id))?;
        Ok(LoweredValue::Value(*result))
    }

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
