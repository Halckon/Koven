//! concrete nominal aggregate 与 intrinsic Box lowering。

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    source::Span,
    type_checking::{
        Copyability, ExpressionCategory, IntrinsicTypeConstructor, NominalKind, ParameterMode,
        UnitAggregateProjectionKind, UnitAggregateProjectionReceiver, UnitConstructionDescriptor,
        UnitConstructionTarget, UnitExpressionId, UnitTypeId, UnitTypeKind,
    },
};

use super::{
    LoweredValue, UnitExpressionLowerer, lowering_error, require_value, resolve_concrete_type,
};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{EntityId, EntityType, LoanId, LoanKind, Operation, Origin, PlaceAccess, SsaTypeId},
    unit_plan::resolve_nominal_runtime_field_types,
};

pub(super) enum CurrentReceiverFieldStorage {
    Heap {
        receiver: LoanId,
        receiver_kind: LoanKind,
    },
    Inline,
}

pub(super) struct CurrentReceiverField {
    pub(super) storage: CurrentReceiverFieldStorage,
    pub(super) owner_ty: UnitTypeId,
    pub(super) ty: UnitTypeId,
    pub(super) ssa_type: SsaTypeId,
    pub(super) symbol: UnitSymbolId,
    pub(super) field: usize,
}

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
                    .signatures()
                    .declaration(declaration)
                    .and_then(|signature| signature.nominal())
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let Some(UnitTypeKind::Nominal {
                    declaration: result_declaration,
                    arguments,
                }) = self.typed.types().get(descriptor.result_type())
                else {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                };
                if !matches!(nominal.kind(), NominalKind::Class | NominalKind::ValueClass)
                    || (!nominal.type_parameters().is_empty()
                        && nominal.kind() != NominalKind::Class)
                {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
                let concrete_fields = resolve_nominal_runtime_field_types(
                    self.typed,
                    descriptor.result_type(),
                    nominal,
                    arguments,
                )?;
                if *result_declaration != declaration
                    || arguments.len() != nominal.type_parameters().len()
                    || arguments != descriptor.instance().type_arguments()
                    || descriptor.arguments().len() != nominal.fields().len()
                    || descriptor.arguments().iter().any(|argument| {
                        nominal
                            .fields()
                            .get(argument.parameter_index())
                            .zip(concrete_fields.get(argument.parameter_index()))
                            .is_none_or(|(field, concrete)| {
                                argument.parameter_symbol() != Some(field.symbol())
                                    || argument.parameter_type() != *concrete
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
                }) = self.typed.types().get(descriptor.result_type())
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
            .aggregate_projection(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if projection.kind() != UnitAggregateProjectionKind::Field
            || self.typed.copyability(projection.ty()) != Copyability::Copyable
            || self.typed.expression_type(id) != Some(projection.ty())
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if let Some(field) = self.current_receiver_field(expression, span)? {
            match field.storage {
                CurrentReceiverFieldStorage::Heap { receiver, .. } => {
                    let (_, results) = self
                        .function
                        .append_instruction(
                            self.block,
                            Operation::HeapFieldRead {
                                receiver,
                                field: field.field,
                            },
                            vec![EntityType::Value(field.ssa_type)],
                            Origin::Source(span),
                        )
                        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                    return Ok(LoweredValue::Value(require_value(results[0], span)?));
                }
                CurrentReceiverFieldStorage::Inline => {
                    return self.lower_this_value_field(field.field, field.ssa_type, span);
                }
            }
        }
        let (receiver_type, explicit_receiver) = match projection.receiver() {
            UnitAggregateProjectionReceiver::Expression(receiver) => {
                if receiver.source_unit() != self.source_unit {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
                let receiver_type = self
                    .typed
                    .expression_type(receiver)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                if self.typed.expression_category(receiver) == Some(ExpressionCategory::Temporary)
                    && self.typed.copyability(receiver_type) == Copyability::MoveOnly
                {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
                (receiver_type, Some(receiver))
            }
            UnitAggregateProjectionReceiver::This(owner) => {
                let receiver = self
                    .current_receiver
                    .filter(|receiver| receiver.owner == owner)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                (receiver.ty, None)
            }
        };
        if matches!(
            self.typed.types().get(receiver_type),
            Some(UnitTypeKind::EnumCase { .. })
        ) {
            let Some(receiver) = explicit_receiver else {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            };
            return self.lower_enum_projection(
                expression,
                projection,
                receiver,
                receiver_type,
                span,
            );
        }
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = self.typed.types().get(receiver_type)
        else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let nominal = self
            .typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if arguments.len() != nominal.type_parameters().len()
            || (!nominal.type_parameters().is_empty() && nominal.kind() != NominalKind::Class)
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let concrete_fields =
            resolve_nominal_runtime_field_types(self.typed, receiver_type, nominal, arguments)?;
        let field = self
            .field_indices
            .get(&(receiver_type, projection.field()))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let result_type = self.expression_ssa_type(expression, span)?;
        let concrete_field = concrete_fields
            .get(field)
            .and_then(|ty| self.type_ids.get(ty))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if result_type != concrete_field {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if nominal.kind() == NominalKind::ValueClass {
            if explicit_receiver.is_none() {
                return self.lower_this_value_field(field, result_type, span);
            }
            let receiver = explicit_receiver.expect("explicit receiver was checked");
            let receiver_value = self.require_expression_value(receiver.expression())?;
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
        let Some(receiver) = explicit_receiver else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        if let Some(value) = self.read_borrowed_heap_field(receiver, field, result_type, span)? {
            return Ok(value);
        }
        let receiver_value = self.require_expression_value(receiver.expression())?;
        // 当前 SSA alias 合同仍按 parent owner 冲突；同父 sibling 读取不假装已封闭。
        if self.pending_call_frames.iter().any(|frame| {
            frame.field_replace_owner.is_some_and(|slot| {
                self.pending_operands.get(slot) == Some(&EntityId::Value(receiver_value))
            })
        }) {
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

    pub(super) fn current_receiver_field(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<CurrentReceiverField>, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let Some(projection) = self.typed.aggregate_projection(id) else {
            return Ok(None);
        };
        if projection.kind() != UnitAggregateProjectionKind::Field {
            return Ok(None);
        }
        let owner = match projection.receiver() {
            UnitAggregateProjectionReceiver::This(owner) => owner,
            UnitAggregateProjectionReceiver::Expression(receiver) => {
                if receiver.source_unit() != self.source_unit
                    || !self.is_this_expression(receiver.expression(), span)?
                {
                    return Ok(None);
                }
                let current = self
                    .current_receiver
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let receiver_type = self
                    .typed
                    .expression_type(receiver)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                if resolve_concrete_type(
                    self.typed,
                    receiver_type,
                    self.substitutions,
                    self.static_self,
                    span,
                )? != current.ty
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
                current.owner
            }
        };
        let current = self
            .current_receiver
            .filter(|receiver| receiver.owner == owner)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = self.typed.types().get(current.ty)
        else {
            return Ok(None);
        };
        let nominal = self
            .typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !matches!(nominal.kind(), NominalKind::Class | NominalKind::ValueClass)
            || arguments.len() != nominal.type_parameters().len()
            || (nominal.kind() == NominalKind::ValueClass && !arguments.is_empty())
        {
            return Ok(None);
        }
        let concrete_fields =
            resolve_nominal_runtime_field_types(self.typed, current.ty, nominal, arguments)?;
        let receiver_ssa = self
            .type_ids
            .get(&current.ty)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let storage = match nominal.kind() {
            NominalKind::Class => {
                let EntityId::Loan(receiver) = current.entity else {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                };
                let receiver_kind = match current.mode {
                    ParameterMode::Borrow => LoanKind::Shared,
                    ParameterMode::Inout => LoanKind::Exclusive,
                    ParameterMode::Value => {
                        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                    }
                };
                if self
                    .function
                    .entity(EntityId::Loan(receiver))
                    .map(|entity| entity.ty)
                    != Some(EntityType::Loan {
                        kind: receiver_kind,
                        target: receiver_ssa,
                    })
                    || !self.heap_payloads.contains_key(&receiver_ssa)
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
                CurrentReceiverFieldStorage::Heap {
                    receiver,
                    receiver_kind,
                }
            }
            NominalKind::ValueClass => {
                if self.heap_payloads.contains_key(&receiver_ssa) {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
                CurrentReceiverFieldStorage::Inline
            }
            NominalKind::Interface | NominalKind::EnumClass | NominalKind::Object => unreachable!(),
        };
        let field = self
            .field_indices
            .get(&(current.ty, projection.field()))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let concrete_type = concrete_fields
            .get(field)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if resolve_concrete_type(
            self.typed,
            projection.ty(),
            self.substitutions,
            self.static_self,
            span,
        )? != concrete_type
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let ssa_type = self
            .type_ids
            .get(&concrete_type)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        Ok(Some(CurrentReceiverField {
            storage,
            owner_ty: current.ty,
            ty: projection.ty(),
            ssa_type,
            symbol: projection.field(),
            field,
        }))
    }

    fn lower_this_value_field(
        &mut self,
        field: usize,
        result_type: crate::ssa::model::SsaTypeId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let receiver = self
            .current_receiver
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        match receiver.entity {
            EntityId::Value(value) => {
                let (_, results) = self
                    .function
                    .append_instruction(
                        self.block,
                        Operation::AggregateProject {
                            aggregate: value,
                            field,
                        },
                        vec![EntityType::Value(result_type)],
                        Origin::Source(span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                Ok(LoweredValue::Value(require_value(results[0], span)?))
            }
            EntityId::Loan(base) => {
                let receiver_type = self
                    .type_ids
                    .get(&receiver.ty)
                    .copied()
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let receiver_kind = self
                    .function
                    .entity(EntityId::Loan(base))
                    .and_then(|data| match data.ty {
                        EntityType::Loan { kind, target } if target == receiver_type => Some(kind),
                        EntityType::Value(_) | EntityType::Place(_) | EntityType::Loan { .. } => {
                            None
                        }
                    })
                    .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
                let (base, reborrow) = match (receiver.mode, receiver_kind) {
                    (ParameterMode::Borrow, LoanKind::Shared) => (base, None),
                    (ParameterMode::Inout, LoanKind::Exclusive) => {
                        let (_, loans) = self
                            .function
                            .append_instruction(
                                self.block,
                                Operation::SharedReborrow { source: base },
                                vec![EntityType::Loan {
                                    kind: LoanKind::Shared,
                                    target: receiver_type,
                                }],
                                Origin::Source(span),
                            )
                            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                        let EntityId::Loan(reborrow) = loans[0] else {
                            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                        };
                        (reborrow, Some(reborrow))
                    }
                    (ParameterMode::Value, _)
                    | (ParameterMode::Borrow, LoanKind::Exclusive)
                    | (ParameterMode::Inout, LoanKind::Shared) => {
                        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                    }
                };
                let (_, loans) = self
                    .function
                    .append_instruction(
                        self.block,
                        Operation::SharedFieldLoan { base, field },
                        vec![EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: result_type,
                        }],
                        Origin::Source(span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                let EntityId::Loan(field_loan) = loans[0] else {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                };
                let (_, results) = self
                    .function
                    .append_instruction(
                        self.block,
                        Operation::Read {
                            source: PlaceAccess::Loan(field_loan),
                        },
                        vec![EntityType::Value(result_type)],
                        Origin::Source(span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                self.function
                    .append_instruction(
                        self.block,
                        Operation::BorrowEnd { loan: field_loan },
                        Vec::new(),
                        Origin::Source(span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                if let Some(reborrow) = reborrow {
                    self.function
                        .append_instruction(
                            self.block,
                            Operation::BorrowEnd { loan: reborrow },
                            Vec::new(),
                            Origin::Source(span),
                        )
                        .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                }
                Ok(LoweredValue::Value(require_value(results[0], span)?))
            }
            EntityId::Place(_) => Err(lowering_error(LoweringErrorKind::InvalidModel, span)),
        }
    }
}
