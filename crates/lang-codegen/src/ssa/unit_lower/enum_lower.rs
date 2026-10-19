//! concrete enum construction 与 smart-cast payload projection lowering。

use lang_frontend::{
    ast::ExpressionId,
    source::Span,
    type_checking::{
        Copyability, NominalKind, UnitAggregateProjectionDescriptor, UnitConstructionDescriptor,
        UnitConstructionTarget, UnitExpressionId, UnitTypeKind,
    },
};

use super::{LoweredValue, UnitExpressionLowerer, lowering_error, require_value};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{EntityId, EntityType, Operation, Origin, PlaceAccess, SsaTypeId, ValueId},
    unit_plan::resolve_enum_runtime_payload_types,
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_copyable_enum_case_condition(
        &mut self,
        owner: ValueId,
        tagged: SsaTypeId,
        expression: ExpressionId,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        if self.typed.construction(id).is_none_or(|descriptor| {
            !matches!(descriptor.target(), UnitConstructionTarget::EnumCase(_))
        }) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let descriptor = self.construction_descriptor(expression, span)?;
        if !descriptor.arguments().is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        match self.typed.copyability(descriptor.result_type()) {
            Copyability::Copyable => {}
            Copyability::MoveOnly => {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            Copyability::Unknown | Copyability::Error => {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        let (variant, _) = self.validate_enum_construction(&descriptor, span)?;
        if self.typed.expression_type(id) != Some(descriptor.result_type())
            || self.expression_ssa_type(expression, span)? != tagged
            || self
                .function
                .entity(EntityId::Value(owner))
                .map(|entity| entity.ty)
                != Some(EntityType::Value(tagged))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        // 零参数路径只验证完整 construction/ownership 身份，不求值或构造条件值。
        if !self
            .lower_construction_fields(&descriptor, span)?
            .is_empty()
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.lower_enum_tag_test(owner, variant, false, span)
    }

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
            .filter(|nominal| nominal.kind() == NominalKind::EnumClass)
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        let (case_index, case) = nominal
            .enum_cases()
            .iter()
            .enumerate()
            .find(|(_, case)| case.value_symbol() == target)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let concrete_payloads =
            resolve_enum_runtime_payload_types(self.typed, descriptor.result_type(), span)?;
        let concrete = concrete_payloads
            .get(case_index)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if arguments != descriptor.instance().type_arguments()
            || descriptor.arguments().len() != case.payloads().len()
            || descriptor.arguments().iter().any(|argument| {
                case.payloads()
                    .get(argument.parameter_index())
                    .zip(concrete.get(argument.parameter_index()))
                    .is_none_or(|(payload, concrete)| {
                        argument.parameter_symbol() != Some(payload.symbol())
                            || argument.parameter_type() != *concrete
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
        let Some(UnitTypeKind::Nominal { declaration, .. }) = self.typed.types().get(*root) else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let nominal = self
            .typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .filter(|nominal| nominal.kind() == NominalKind::EnumClass)
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        let (case_index, case_signature) = nominal
            .enum_cases()
            .iter()
            .enumerate()
            .find(|(_, candidate)| candidate.type_symbol() == *case)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let field = self
            .field_indices
            .get(&(*root, projection.field()))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let concrete_payloads = resolve_enum_runtime_payload_types(self.typed, *root, span)?;
        if case_signature
            .payloads()
            .get(field)
            .is_none_or(|payload| payload.symbol() != projection.field())
            || concrete_payloads
                .get(case_index)
                .and_then(|fields| fields.get(field))
                != Some(&projection.ty())
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
