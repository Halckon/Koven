//! compilation-unit intrinsic `Rc<T>` construction 与 effect 事实校验。

use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::RcOwnershipEffectKind,
    source::Span,
    type_checking::{
        ExpressionCategory, IntrinsicTypeConstructor, RcOperationKind, UnitConstructionTarget,
        UnitExpressionId, UnitRcOperationDescriptor, UnitTypeKind,
    },
};

use super::{LoweredValue, UnitExpressionLowerer, lowering_error, require_value};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{EntityType, Operation, Origin},
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_rc_construction(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self.construction_descriptor(expression, span)?;
        if descriptor.target() != UnitConstructionTarget::IntrinsicRc {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let [argument] = descriptor.arguments() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let [type_argument] = descriptor.instance().type_arguments() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let Some(UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Rc,
            arguments: result_arguments,
        }) = self.typed.types().get(descriptor.result_type())
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let [result_payload] = result_arguments.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if *type_argument != argument.parameter_type()
            || *result_payload != argument.parameter_type()
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let fields = self.lower_construction_fields(&descriptor, span)?;
        let [payload] = fields.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let owner = self.expression_ssa_type(expression, span)?;
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::SharedAllocate {
                    owner,
                    payload: *payload,
                },
                vec![EntityType::Value(owner)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(LoweredValue::Value(require_value(results[0], span)?))
    }

    pub(super) fn validated_rc_operation(
        &self,
        id: UnitExpressionId,
        span: Span,
    ) -> Result<UnitRcOperationDescriptor, LoweringError> {
        let descriptor = self
            .typed
            .rc_operation(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let mut effects = self
            .owned
            .rc_effects()
            .iter()
            .copied()
            .filter(|effect| effect.expression() == id);
        let effect = effects
            .next()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let expected = match descriptor.kind() {
            RcOperationKind::Share => RcOwnershipEffectKind::Retain,
            RcOperationKind::Value => RcOwnershipEffectKind::BorrowPayload,
        };
        if id.source_unit() != self.source_unit
            || descriptor.expression() != id
            || descriptor.receiver().source_unit() != self.source_unit
            || effects.next().is_some()
            || effect.receiver() != descriptor.receiver()
            || effect.payload_type() != descriptor.payload_type()
            || effect.kind() != expected
            || self.typed.expression_category(descriptor.receiver())
                != Some(ExpressionCategory::Place)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(descriptor)
    }
}
