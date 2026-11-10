//! compilation-unit intrinsic `Rc<T>` construction lowering。

use lang_frontend::{
    ast::ExpressionId,
    source::Span,
    type_checking::{IntrinsicTypeConstructor, UnitConstructionTarget, UnitTypeKind},
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
        let Some(fields) = self.lower_construction_fields(&descriptor, span)? else {
            return Ok(LoweredValue::Diverged);
        };
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
}
