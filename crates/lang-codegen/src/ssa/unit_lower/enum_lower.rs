//! concrete non-generic enum construction 与 smart-cast payload projection lowering。

use lang_frontend::{
    ast::ExpressionId,
    source::Span,
    type_checking::{
        NominalKind, UnitAggregateProjectionDescriptor, UnitConstructionDescriptor,
        UnitConstructionTarget, UnitExpressionId, UnitTypeKind,
    },
};

use super::{LoweredValue, UnitExpressionLowerer, lowering_error, require_value};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{EntityId, EntityType, Operation, Origin, PlaceAccess},
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_enum_construction(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self.construction_descriptor(expression, span)?;
        let (variant, payload) = self.validate_enum_construction(&descriptor, span)?;
        let fields = self.lower_construction_fields(&descriptor, span)?;
        let result_type = self.expression_ssa_type(expression, span)?;
        let (_, payload_results) = self
            .function
            .append_instruction(
                self.block,
                Operation::AggregateConstruct {
                    aggregate: payload,
                    fields,
                },
                vec![EntityType::Value(payload)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let payload = require_value(payload_results[0], span)?;
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::TaggedConstruct {
                    tagged: result_type,
                    variant,
                    payload,
                },
                vec![EntityType::Value(result_type)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(LoweredValue::Value(require_value(results[0], span)?))
    }

    fn validate_enum_construction(
        &self,
        descriptor: &UnitConstructionDescriptor,
        span: Span,
    ) -> Result<(usize, crate::ssa::model::SsaTypeId), LoweringError> {
        let UnitConstructionTarget::EnumCase(target) = descriptor.target() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let Some(UnitTypeKind::Nominal {
            declaration: result_declaration,
            arguments,
        }) = self.typed.types().get(descriptor.result_type())
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let nominal = self
            .typed
            .signatures()
            .declaration(*result_declaration)
            .and_then(|signature| signature.nominal())
            .filter(|nominal| {
                nominal.kind() == NominalKind::EnumClass && nominal.type_parameters().is_empty()
            })
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        let case = nominal
            .enum_cases()
            .iter()
            .find(|case| case.value_symbol() == target)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Some(UnitTypeKind::EnumCase {
            case: case_symbol,
            root,
        }) = self.typed.types().get(case.case_type())
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if !arguments.is_empty()
            || !descriptor.instance().type_arguments().is_empty()
            || *case_symbol != case.type_symbol()
            || *root != descriptor.result_type()
            || descriptor.arguments().len() != case.payloads().len()
            || descriptor.arguments().iter().any(|argument| {
                case.payloads()
                    .get(argument.parameter_index())
                    .is_none_or(|payload| {
                        argument.parameter_symbol() != Some(payload.symbol())
                            || argument.parameter_type() != payload.ty()
                    })
            })
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let tagged = self
            .type_ids
            .get(&descriptor.result_type())
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let (variant, payload) = self
            .enum_payloads
            .get(&(tagged, target))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        Ok((variant, payload))
    }

    pub(super) fn lower_enum_projection(
        &mut self,
        expression: ExpressionId,
        projection: UnitAggregateProjectionDescriptor,
        receiver: UnitExpressionId,
        receiver_type: lang_frontend::type_checking::UnitTypeId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let Some(UnitTypeKind::EnumCase { case, root }) = self.typed.types().get(receiver_type)
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = self.typed.types().get(*root)
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let nominal = self
            .typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .filter(|nominal| {
                nominal.kind() == NominalKind::EnumClass
                    && nominal.type_parameters().is_empty()
                    && arguments.is_empty()
            })
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        let case_signature = nominal
            .enum_cases()
            .iter()
            .find(|candidate| candidate.type_symbol() == *case)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let field = self
            .field_indices
            .get(&(receiver_type, projection.field()))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if case_signature
            .payloads()
            .get(field)
            .is_none_or(|payload| payload.symbol() != projection.field())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let tagged = self
            .type_ids
            .get(root)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let (variant, payload) = self
            .enum_payloads
            .get(&(tagged, *case))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let receiver_value = self.require_expression_value(receiver.expression())?;
        let result_type = self.expression_ssa_type(expression, span)?;
        let (_, roots) = self
            .function
            .append_instruction(
                self.block,
                Operation::TaggedPayloadPlace {
                    owner: receiver_value,
                    variant,
                },
                vec![EntityType::Place(payload)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Place(root) = roots[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        let (_, fields) = self
            .function
            .append_instruction(
                self.block,
                Operation::FieldPlace { base: root, field },
                vec![EntityType::Place(result_type)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Place(field) = fields[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::Read {
                    source: PlaceAccess::Place(field),
                },
                vec![EntityType::Value(result_type)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(LoweredValue::Value(require_value(results[0], span)?))
    }
}
