//! 已验证的一级普通 class 字段置换；父 owner 始终保持原身份与完整析构义务。
use lang_frontend::{
    name_resolution::{SymbolKind, UnitSymbolId},
    ownership_checking::{
        LoanKind as FrontendLoanKind, OwnershipPrimitiveValueTransfer, UnitDropPoint,
        UnitLoanTarget, UnitValueDeliveryKind,
    },
    parser::{CallArgument, Expression},
    source::Span,
    type_checking::{
        BuiltinType, NominalKind, UnitAggregateProjectionKind, UnitAggregateProjectionReceiver,
        UnitExpressionId, UnitOwnershipPrimitiveDescriptor, UnitTypeId, UnitTypeKind,
    },
};

use super::{LoweredValue, UnitExpressionLowerer, lowering_error, require_value};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{EntityId, EntityType, LoanKind, Operation, Origin, SsaTypeId},
};

struct PendingField {
    symbol: UnitSymbolId,
    owner_type: UnitTypeId,
    field: usize,
    owner_slot: usize,
    loan_slot: usize,
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_field_replace_in_frame(
        &mut self,
        primitive: UnitOwnershipPrimitiveDescriptor,
        arguments: &[CallArgument],
        target: SsaTypeId,
        copyable: bool,
        unit: bool,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let field = self.begin_field_replace(primitive, &arguments[0], target, span)?;
        let argument = &arguments[1];
        // Nothing 保留已建立的 field loan 前缀，但没有正常提交能力。
        if self.expression_builtin_type(argument.value, argument.span)?
            == Some(BuiltinType::Nothing)
        {
            return match self.lower(argument.value)? {
                LoweredValue::Diverged => Ok(LoweredValue::Diverged),
                _ => Err(lowering_error(
                    LoweringErrorKind::InvalidModel,
                    argument.span,
                )),
            };
        }
        let replacement = match self.lower_value_argument(
            primitive.expression(),
            argument.value,
            primitive.value_type(),
            argument.span,
        )? {
            LoweredValue::Value(value) => {
                if copyable {
                    let (_, values) = self
                        .function
                        .append_instruction(
                            self.block,
                            Operation::Copy { source: value },
                            vec![EntityType::Value(target)],
                            Origin::Source(argument.span),
                        )
                        .map_err(|_| {
                            lowering_error(LoweringErrorKind::InvalidModel, argument.span)
                        })?;
                    require_value(values[0], argument.span)?
                } else {
                    value
                }
            }
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
            LoweredValue::Unit => {
                return Err(lowering_error(
                    LoweringErrorKind::InvalidModel,
                    argument.span,
                ));
            }
        };
        self.require_field_replace_commit(primitive, &field, span)?;
        let Some(EntityId::Value(owner)) = self.pending_operands.get(field.owner_slot).copied()
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let Some(EntityId::Loan(loan)) = self.pending_operands.get(field.loan_slot).copied() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if self.bindings.get(&field.symbol).copied() != Some(LoweredValue::Value(owner)) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let (_, values) = self
            .function
            .append_instruction(
                self.block,
                Operation::HeapFieldExchange {
                    owner,
                    field: field.field,
                    loan,
                    replacement,
                },
                vec![EntityType::Value(target)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        // Exchange 只消费新字段值与独占 loan；父 binding 不改写，旧字段转为独立返回 owner。
        self.emit_borrow_argument_expression_drops(primitive.expression())?;
        self.emit_drops(UnitDropPoint::CallReturn(primitive.expression()))?;
        Ok(if unit {
            LoweredValue::Unit
        } else {
            LoweredValue::Value(require_value(values[0], span)?)
        })
    }

    fn begin_field_replace(
        &mut self,
        primitive: UnitOwnershipPrimitiveDescriptor,
        argument: &CallArgument,
        target: SsaTypeId,
        span: Span,
    ) -> Result<PendingField, LoweringError> {
        let mut facts = self.owned.loans().iter().filter(|fact| {
            fact.call() == primitive.expression() && fact.argument() == primitive.operands()[0]
        });
        let fact = facts
            .next()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
        if facts.next().is_some()
            || fact.kind() != FrontendLoanKind::Exclusive
            || fact.end_span() != span
        {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                argument.span,
            ));
        }
        let UnitLoanTarget::Place(place) = fact.target() else {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                argument.span,
            ));
        };
        let [symbol] = place.fields() else {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                argument.span,
            ));
        };
        if place.element().is_some() {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                argument.span,
            ));
        }
        let mut projected = argument.value;
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(projected)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
            match node.payload() {
                Expression::Group { expression } => projected = *expression,
                _ => break,
            }
        }
        let projection = self
            .typed
            .aggregate_projection(UnitExpressionId::new(self.source_unit, projected))
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, argument.span))?;
        let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                argument.span,
            ));
        };
        if projection.kind() != UnitAggregateProjectionKind::Field
            || projection.field() != *symbol
            || projection.ty() != primitive.value_type()
            || receiver.source_unit() != self.source_unit
            || self.direct_place_symbol(receiver.expression(), argument.span)? != place.root()
        {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                argument.span,
            ));
        }
        let root = place.root();
        let root_symbol = self.names.names().source_units()[root.source_unit().index()]
            .resolution()
            .symbols()
            .get(root.symbol().index())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
        if root_symbol.kind() != SymbolKind::Variable {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                argument.span,
            ));
        }
        let owner_type = self
            .typed
            .symbol_type(root)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = self.typed.types().get(owner_type)
        else {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                argument.span,
            ));
        };
        let nominal = self
            .typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
        if nominal.kind() != NominalKind::Class
            || !arguments.is_empty()
            || !nominal.type_parameters().is_empty()
        {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                argument.span,
            ));
        }
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&root).copied() else {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                argument.span,
            ));
        };
        if self.pending_call_frames.iter().any(|frame| {
            frame.shared_field_roots.contains(&root)
                || frame.field_replace_owner.is_some_and(|slot| {
                    self.pending_operands.get(slot) == Some(&EntityId::Value(owner))
                })
        }) {
            return Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                argument.span,
            ));
        }
        let owner_ssa = self
            .type_ids
            .get(&owner_type)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
        if self
            .function
            .entity(EntityId::Value(owner))
            .map(|entity| entity.ty)
            != Some(EntityType::Value(owner_ssa))
        {
            return Err(lowering_error(
                LoweringErrorKind::MissingFact,
                argument.span,
            ));
        }
        let payload = self
            .heap_payloads
            .get(&owner_ssa)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
        let field = self
            .field_indices
            .get(&(owner_type, *symbol))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
        let (_, roots) = self
            .function
            .append_instruction(
                self.block,
                Operation::HeapPayloadPlace { owner },
                vec![EntityType::Place(payload)],
                Origin::Source(fact.begin_span()),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, argument.span))?;
        let EntityId::Place(base) = roots[0] else {
            return Err(lowering_error(
                LoweringErrorKind::InvalidModel,
                argument.span,
            ));
        };
        let (_, fields) = self
            .function
            .append_instruction(
                self.block,
                Operation::FieldPlace { base, field },
                vec![EntityType::Place(target)],
                Origin::Source(fact.begin_span()),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, argument.span))?;
        let EntityId::Place(place) = fields[0] else {
            return Err(lowering_error(
                LoweringErrorKind::InvalidModel,
                argument.span,
            ));
        };
        let (_, loans) = self
            .function
            .append_instruction(
                self.block,
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Exclusive,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    target,
                }],
                Origin::Source(fact.begin_span()),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, argument.span))?;
        let EntityId::Loan(loan) = loans[0] else {
            return Err(lowering_error(
                LoweringErrorKind::InvalidModel,
                argument.span,
            ));
        };
        let owner_slot = self.pending_operands.len();
        self.pending_operands.push(EntityId::Value(owner));
        let loan_slot = self.pending_operands.len();
        self.pending_operands.push(EntityId::Loan(loan));
        let frame = self
            .pending_call_frames
            .last_mut()
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, argument.span))?;
        frame.created_loans.push(loan_slot);
        frame.field_replace_owner = Some(owner_slot);
        Ok(PendingField {
            symbol: root,
            owner_type,
            field,
            owner_slot,
            loan_slot,
        })
    }

    fn require_field_replace_commit(
        &self,
        primitive: UnitOwnershipPrimitiveDescriptor,
        field: &PendingField,
        span: Span,
    ) -> Result<(), LoweringError> {
        let plan = self
            .owned
            .field_replacement(primitive.expression())
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        let loan = self
            .owned
            .loans()
            .iter()
            .find(|loan| {
                loan.call() == primitive.expression() && loan.argument() == primitive.operands()[0]
            })
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if plan.descriptor() != &primitive
            || plan.owner_type() != field.owner_type
            || !matches!(loan.target(), UnitLoanTarget::Place(place) if place == plan.place())
            || plan.place().root() != field.symbol
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let delivery = self
            .owned
            .value_deliveries()
            .iter()
            .find(|delivery| {
                delivery.call() == primitive.expression()
                    && delivery.argument() == primitive.operands()[1]
            })
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let expected = match delivery.kind() {
            UnitValueDeliveryKind::Copy => OwnershipPrimitiveValueTransfer::Copy,
            UnitValueDeliveryKind::Move => OwnershipPrimitiveValueTransfer::Move,
            UnitValueDeliveryKind::Temporary => OwnershipPrimitiveValueTransfer::Temporary,
        };
        if plan.new_value_transfer() != expected {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }
}
