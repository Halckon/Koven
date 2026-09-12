//! compilation-unit intrinsic `Rc<T>` construction 与 owner operation lowering。

use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::RcOwnershipEffectKind,
    source::Span,
    type_checking::{
        Copyability, ExpressionCategory, IntrinsicTypeConstructor, RcOperationKind,
        UnitConstructionTarget, UnitExpressionId, UnitTypeKind,
    },
};

use super::{LoweredValue, UnitExpressionLowerer, lowering_error, require_value};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{EntityId, EntityType, Operation, Origin, PlaceAccess},
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

    pub(super) fn lower_rc_operation(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
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
        if effects.next().is_some()
            || effect.receiver() != descriptor.receiver()
            || effect.payload_type() != descriptor.payload_type()
            || effect.kind() != expected
            || self.typed.expression_category(descriptor.receiver())
                != Some(ExpressionCategory::Place)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let LoweredValue::Value(owner) = self.lower(descriptor.receiver().expression())? else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        match descriptor.kind() {
            RcOperationKind::Share => {
                let result_type = self.expression_ssa_type(expression, span)?;
                let (_, results) = self
                    .function
                    .append_instruction(
                        self.block,
                        Operation::SharedRetain {
                            owner: EntityId::Value(owner),
                        },
                        vec![EntityType::Value(result_type)],
                        Origin::Source(span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                Ok(LoweredValue::Value(require_value(results[0], span)?))
            }
            RcOperationKind::Value => {
                if self.typed.copyability(descriptor.payload_type()) != Copyability::Copyable {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
                let payload = self.expression_ssa_type(expression, span)?;
                let (_, places) = self
                    .function
                    .append_instruction(
                        self.block,
                        Operation::SharedPayloadPlace {
                            owner: EntityId::Value(owner),
                        },
                        vec![EntityType::Place(payload)],
                        Origin::Source(span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                let EntityId::Place(place) = places[0] else {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                };
                let (_, results) = self
                    .function
                    .append_instruction(
                        self.block,
                        Operation::Read {
                            source: PlaceAccess::Place(place),
                        },
                        vec![EntityType::Value(payload)],
                        Origin::Source(span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                Ok(LoweredValue::Value(require_value(results[0], span)?))
            }
        }
    }
}
