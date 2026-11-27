//! Map operations that end before dispatch or borrowed-result delivery.
use lang_frontend::{
    ast::ExpressionId,
    type_checking::{UnitExpressionId, UnitMapRequireValueDescriptor},
};

use super::RefineInputs;

impl RefineInputs<'_> {
    /// A verified receiver exit precedes any Map transfer, loan or later operand.
    pub(super) fn non_completing_map_receiver(
        &self,
        expression: UnitExpressionId,
    ) -> Option<ExpressionId> {
        let (call, receiver) = if let Some(consume) = self.typed.map_consume(expression) {
            if consume.source().is_some() {
                return None;
            }
            (consume.expression(), consume.receiver())
        } else if let Some(remove) = self.typed.map_remove(expression) {
            if remove.key().source_unit() != self.source_unit {
                return None;
            }
            (remove.expression(), remove.receiver())
        } else {
            let with_value = self.typed.map_with_value(expression)?;
            if with_value.key().source_unit() != self.source_unit
                || with_value.action().source_unit() != self.source_unit
            {
                return None;
            }
            (with_value.expression(), with_value.receiver())
        };
        (call == expression
            && receiver.source_unit() == self.source_unit
            && self.typed.expression_falls_through(receiver) == Some(false))
        .then_some(receiver.expression())
    }

    pub(super) fn abandoned_map_require_value(
        &self,
        call: UnitExpressionId,
    ) -> Option<UnitMapRequireValueDescriptor> {
        let descriptor = *self.typed.map_require_value(call)?;
        (descriptor.expression() == call
            && call.source_unit() == self.source_unit
            && descriptor.receiver().source_unit() == self.source_unit
            && descriptor.key().source_unit() == self.source_unit
            && self.borrow_return.is_none()
            && self.carrier_return.is_none()
            && self.typed.expression_falls_through(call) == Some(false)
            && self.owned.call_may_dispatch(call) == Some(false))
        .then_some(descriptor)
    }
}
