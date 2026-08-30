//! concrete non-generic nominal aggregate 与 intrinsic Box lowering。

use lang_frontend::{
    ast::ExpressionId,
    source::Span,
    type_checking::{
        Copyability, ExpressionCategory, IntrinsicTypeConstructor, NominalKind,
        UnitAggregateProjectionKind, UnitAggregateProjectionReceiver, UnitConstructionDescriptor,
        UnitConstructionTarget, UnitExpressionId, UnitTypeKind,
    },
};

use super::{LoweredValue, UnitExpressionLowerer, lowering_error, require_value};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{EntityId, EntityType, Operation, Origin, PlaceAccess},
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_aggregate_construction(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self.construction_descriptor(expression, span)?;
        self.validate_aggregate_construction_identity(&descriptor, span)?;
        let fields = self.lower_construction_fields(&descriptor, span)?;
        let result_type = self.expression_ssa_type(expression, span)?;
        let result = match descriptor.target() {
            UnitConstructionTarget::Nominal(declaration) => {
                let nominal = self
                    .typed
                    .types()
                    .signatures()
                    .declaration(declaration)
                    .and_then(|signature| signature.nominal())
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let aggregate = match nominal.kind() {
                    NominalKind::ValueClass => result_type,
                    NominalKind::Class => self
                        .heap_payloads
                        .get(&result_type)
                        .copied()
                        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?,
                    NominalKind::Interface | NominalKind::EnumClass | NominalKind::Object => {
                        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                    }
                };
                let (_, aggregate_results) = self
                    .function
                    .append_instruction(
                        self.block,
                        Operation::AggregateConstruct { aggregate, fields },
                        vec![EntityType::Value(aggregate)],
                        Origin::Source(span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                let payload = require_value(aggregate_results[0], span)?;
                if nominal.kind() == NominalKind::Class {
                    let (_, owner_results) = self
                        .function
                        .append_instruction(
                            self.block,
                            Operation::HeapAllocate {
                                owner: result_type,
                                payload,
                            },
                            vec![EntityType::Value(result_type)],
                            Origin::Source(span),
                        )
                        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                    require_value(owner_results[0], span)?
                } else {
                    payload
                }
            }
            UnitConstructionTarget::IntrinsicBox => {
                let [payload] = fields.as_slice() else {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                };
                let (_, results) = self
                    .function
                    .append_instruction(
                        self.block,
                        Operation::HeapAllocate {
                            owner: result_type,
                            payload: *payload,
                        },
                        vec![EntityType::Value(result_type)],
                        Origin::Source(span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                require_value(results[0], span)?
            }
            UnitConstructionTarget::IntrinsicRc | UnitConstructionTarget::EnumCase(_) => {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
        };
        Ok(LoweredValue::Value(result))
    }

    fn validate_aggregate_construction_identity(
        &self,
        descriptor: &UnitConstructionDescriptor,
        span: Span,
    ) -> Result<(), LoweringError> {
        match descriptor.target() {
            UnitConstructionTarget::Nominal(declaration) => {
                let nominal = self
                    .typed
                    .types()
                    .signatures()
                    .declaration(declaration)
                    .and_then(|signature| signature.nominal())
                    .filter(|nominal| {
                        nominal.type_parameters().is_empty()
                            && matches!(
                                nominal.kind(),
                                NominalKind::Class | NominalKind::ValueClass
                            )
                    })
                    .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
                let Some(UnitTypeKind::Nominal {
                    declaration: result_declaration,
                    arguments,
                }) = self.typed.types().types().get(descriptor.result_type())
                else {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                };
                if *result_declaration != declaration
                    || !arguments.is_empty()
                    || !descriptor.instance().type_arguments().is_empty()
                    || descriptor.arguments().len() != nominal.fields().len()
                    || descriptor.arguments().iter().any(|argument| {
                        nominal
                            .fields()
                            .get(argument.parameter_index())
                            .is_none_or(|field| {
                                argument.parameter_symbol() != Some(field.symbol())
                                    || argument.parameter_type() != field.ty()
                            })
                    })
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
                Ok(())
            }
            UnitConstructionTarget::IntrinsicBox => {
                let [type_argument] = descriptor.instance().type_arguments() else {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                };
                let [argument] = descriptor.arguments() else {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                };
                let Some(UnitTypeKind::Intrinsic {
                    constructor: IntrinsicTypeConstructor::Box,
                    arguments,
                }) = self.typed.types().types().get(descriptor.result_type())
                else {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                };
                if arguments.as_slice() != [*type_argument]
                    || *type_argument != argument.parameter_type()
                    || argument.parameter_symbol().is_some()
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
                Ok(())
            }
            UnitConstructionTarget::IntrinsicRc | UnitConstructionTarget::EnumCase(_) => {
                Err(lowering_error(LoweringErrorKind::UnsupportedNode, span))
            }
        }
    }

    pub(super) fn lower_aggregate_projection(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let projection = self
            .typed
            .types()
            .aggregate_projection(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if projection.kind() != UnitAggregateProjectionKind::Field
            || self.typed.types().copyability(projection.ty()) != Copyability::Copyable
            || self.typed.types().expression_type(id) != Some(projection.ty())
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        if receiver.source_unit() != self.source_unit {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let receiver_type = self
            .typed
            .types()
            .expression_type(receiver)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if self.typed.types().expression_category(receiver) == Some(ExpressionCategory::Temporary)
            && self.typed.types().copyability(receiver_type) == Copyability::MoveOnly
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = self.typed.types().types().get(receiver_type)
        else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        if !arguments.is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let nominal = self
            .typed
            .types()
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let field = self
            .field_indices
            .get(&(receiver_type, projection.field()))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let receiver_value = self.require_expression_value(receiver.expression())?;
        let result_type = self.expression_ssa_type(expression, span)?;
        if nominal.kind() == NominalKind::ValueClass {
            let (_, results) = self
                .function
                .append_instruction(
                    self.block,
                    Operation::AggregateProject {
                        aggregate: receiver_value,
                        field,
                    },
                    vec![EntityType::Value(result_type)],
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            return Ok(LoweredValue::Value(require_value(results[0], span)?));
        }
        if nominal.kind() != NominalKind::Class {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let receiver_ssa = self.expression_ssa_type(receiver.expression(), span)?;
        let payload = self
            .heap_payloads
            .get(&receiver_ssa)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let (_, roots) = self
            .function
            .append_instruction(
                self.block,
                Operation::HeapPayloadPlace {
                    owner: receiver_value,
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
