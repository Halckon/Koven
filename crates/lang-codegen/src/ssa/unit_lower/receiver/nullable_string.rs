//! A narrowed text receiver borrows the existing If payload view, not its wrapper.
use super::*;
use crate::ssa::model::SsaTypeKind;
use lang_frontend::type_checking::{
    BuiltinType, CallableResultSource, UnitCallTarget, UnitCallableTarget,
};

impl UnitExpressionLowerer<'_> {
    /// The If proof owns this loan; the synchronous receiver frame must not end it.
    pub(in crate::ssa::unit_lower) fn nullable_string_receiver_loan(
        &self,
        call: UnitExpressionId,
        descriptor: &UnitCallDescriptor,
        span: Span,
    ) -> Result<Option<LoanId>, LoweringError> {
        let Some(receiver) = descriptor.receiver() else {
            return Ok(None);
        };
        let UnitCallTarget::Declaration(declaration) = descriptor.target() else {
            return Ok(None);
        };
        let UnitCallReceiverOrigin::Expression(expression) = receiver.origin() else {
            return Ok(None);
        };
        if receiver.mode() != ParameterMode::Borrow
            || super::super::builtin_type(self.typed, receiver.ty()) != Some(BuiltinType::String)
            || descriptor.result_source() != CallableResultSource::Owned
            || !self
                .typed
                .signatures()
                .declaration(declaration)
                .and_then(|signature| signature.callable())
                .and_then(|signature| signature.text_extension())
                .is_some_and(|binding| {
                    binding.callable() == UnitCallableTarget::Declaration(declaration)
                        && binding.receiver_type() == receiver.ty()
                })
        {
            return Ok(None);
        }
        let fact = self
            .owned
            .receiver_fact(call)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let UnitReceiverOwnershipTarget::Place(place) = fact.target() else {
            return Ok(None);
        };
        if !place.is_root() {
            return Ok(None);
        }
        let Some(proof) = self.nullable_proofs.get(&place.root()) else {
            return Ok(None);
        };
        if proof.if_scope().is_none()
            || self.ssa_types.get(proof.inner.index()) != Some(&SsaTypeKind::StringOwner)
        {
            return Ok(None);
        }
        if call.source_unit() != self.source_unit
            || descriptor.expression() != call
            || expression.source_unit() != self.source_unit
            || fact.call() != call
            || fact.source() != receiver.origin()
            || fact.receiver_type() != receiver.ty()
            || fact.kind() != UnitReceiverOwnershipKind::SharedLoan
            || fact.is_receiver_reservation()
            || fact.activation_point().is_some()
            || fact.end_span() != span
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.lower_nullable_root_place_borrow(expression.expression(), place, proof.inner, span)
    }
}
