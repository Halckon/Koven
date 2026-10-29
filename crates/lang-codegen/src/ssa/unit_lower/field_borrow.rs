//! Field views consume validated projection identity and retain their parent owner loan.
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::SymbolKind,
    ownership_checking::{UnitLoanTarget, UnitOwnershipPlace},
    source::Span,
    type_checking::{
        BorrowReturnOrigin, CallableResultSource, NominalKind, UnitAggregateProjectionKind,
        UnitAggregateProjectionReceiver, UnitCallReceiverOrigin, UnitExpressionId, UnitTypeKind,
    },
};

use super::{LoweredValue, UnitExpressionLowerer, lowering_error};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{
        Definition, EntityId, EntityType, LoanId, LoanKind, Operation, Origin, PlaceAccess,
        PlaceId, SsaTypeId,
    },
};

impl UnitExpressionLowerer<'_> {
    /// The aggregate caller has already checked its nominal field/type identity. A Borrow
    /// parameter and a thunk capture both read through the current CFG-bound parent loan.
    pub(super) fn read_borrowed_heap_field(
        &mut self,
        receiver: UnitExpressionId,
        field: usize,
        target: SsaTypeId,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        let Some(base) = self.borrowed_heap_receiver(receiver, span)? else {
            return Ok(None);
        };
        let (_, values) = self
            .function
            .append_instruction(
                self.block,
                Operation::HeapFieldRead {
                    receiver: base,
                    field,
                },
                vec![EntityType::Value(target)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(Some(LoweredValue::Value(super::require_value(
            values[0], span,
        )?)))
    }

    /// Only a published single-field LoanFact can form this child. The caller retains the
    /// parent parameter/capture loan and ends the returned child at this synchronous call.
    pub(super) fn lower_borrowed_field_loan(
        &mut self,
        call: UnitExpressionId,
        expression: ExpressionId,
        loan_target: &UnitLoanTarget,
        target: SsaTypeId,
        span: Span,
    ) -> Result<Option<LoanId>, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if let lang_frontend::parser::Expression::Group { expression } = node.payload() {
            return self.lower_borrowed_field_loan(call, *expression, loan_target, target, span);
        }
        let id = UnitExpressionId::new(self.source_unit, expression);
        let Some(projection) = self.typed.aggregate_projection(id) else {
            return Ok(None);
        };
        let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            return Ok(None);
        };
        let Some(base) = self.borrowed_heap_receiver(receiver, span)? else {
            return Ok(None);
        };
        let UnitLoanTarget::Place(place) = loan_target else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let [symbol] = place.fields() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        if place.element().is_some()
            || projection.kind() != UnitAggregateProjectionKind::Field
            || projection.expression() != id
            || projection.field() != *symbol
            || self.direct_name_symbol(receiver.expression(), span)? != Some(place.root())
            || self.typed.expression_type(id) != Some(projection.ty())
            || self.type_ids.get(&projection.ty()).copied() != Some(target)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let owner_type = self
            .typed
            .expression_type(receiver)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = self.typed.types().get(owner_type)
        else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let nominal = self
            .typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !arguments.is_empty() || !nominal.type_parameters().is_empty() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let inline = match nominal.kind() {
            NominalKind::Class => false,
            NominalKind::ValueClass => {
                // This slice only creates call-scoped children, not a borrowed-result source.
                if self.field_is_result_source(call, expression, span)?
                    || self.borrowed_inline_parameter(receiver, span)? != Some(base)
                {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
                true
            }
            _ => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
        };
        let field = self
            .field_indices
            .get(&(owner_type, *symbol))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if nominal
            .fields()
            .get(field)
            .is_none_or(|field| field.symbol() != *symbol || field.ty() != projection.ty())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let (_, values) = self
            .function
            .append_instruction(
                self.block,
                if inline {
                    Operation::SharedFieldLoan { base, field }
                } else {
                    Operation::SharedHeapFieldLoan { base, field }
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Loan(loan) = values[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        self.pending_call_frames
            .last_mut()
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
            .shared_field_roots
            .push(place.root());
        Ok(Some(loan))
    }

    fn field_is_result_source(
        &self,
        call: UnitExpressionId,
        field_expression: ExpressionId,
        span: Span,
    ) -> Result<bool, LoweringError> {
        let descriptor = self
            .typed
            .call(call)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let origin = match descriptor.result_source() {
            CallableResultSource::Owned => return Ok(false),
            CallableResultSource::Borrow(contract) => contract.origin(),
            CallableResultSource::Carrier(contract) => contract.origin(),
        };
        if call.source_unit() != self.source_unit {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let source = match origin {
            BorrowReturnOrigin::Parameter(index) => {
                let node = self
                    .parsed
                    .ast()
                    .expressions()
                    .get(call.expression())
                    .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let lang_frontend::parser::Expression::Call { arguments, .. } = node.payload()
                else {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                };
                descriptor
                    .arguments()
                    .iter()
                    .find(|mapping| mapping.parameter_index() == index)
                    .and_then(|mapping| arguments.get(mapping.argument_index()))
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
                    .value
            }
            BorrowReturnOrigin::Receiver => {
                let receiver = descriptor
                    .receiver()
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let UnitCallReceiverOrigin::Expression(source) = receiver.origin() else {
                    return Ok(false);
                };
                if source.source_unit() != self.source_unit {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
                source.expression()
            }
        };
        Ok(
            crate::ssa::borrow_result_support::ungroup(self.parsed, source, span)?
                == field_expression,
        )
    }

    /// The aggregate caller has checked the concrete field and Copyable result. Only
    /// an ordinary entry parameter is enabled here; other shared bindings retain their gates.
    pub(super) fn read_borrowed_inline_field(
        &mut self,
        receiver: UnitExpressionId,
        field: usize,
        target: SsaTypeId,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        let Some(base) = self.borrowed_inline_parameter(receiver, span)? else {
            return Ok(None);
        };
        let (_, loans) = self
            .function
            .append_instruction(
                self.block,
                Operation::SharedFieldLoan { base, field },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Loan(loan) = loans[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        let (_, values) = self
            .function
            .append_instruction(
                self.block,
                Operation::Read {
                    source: PlaceAccess::Loan(loan),
                },
                vec![EntityType::Value(target)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.function
            .append_instruction(
                self.block,
                Operation::BorrowEnd { loan },
                Vec::new(),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(Some(LoweredValue::Value(super::require_value(
            values[0], span,
        )?)))
    }

    fn borrowed_inline_parameter(
        &self,
        receiver: UnitExpressionId,
        span: Span,
    ) -> Result<Option<LoanId>, LoweringError> {
        let Some(base) = self.borrowed_heap_receiver(receiver, span)? else {
            return Ok(None);
        };
        let symbol = self
            .direct_name_symbol(receiver.expression(), span)?
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let binding = self.names.names().source_units()[symbol.source_unit().index()]
            .resolution()
            .symbols()
            .get(symbol.symbol().index())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let entity = self
            .function
            .entity(EntityId::Loan(base))
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if binding.kind() != SymbolKind::ValueParameter
            || !matches!(entity.definition, Definition::BlockParameter { block, .. }
                if Some(block) == self.function.entry_block())
        {
            // A Copyable aggregate may already be readable through the existing Value
            // fallback. Do not replace that path or admit this binding as a new field loan.
            return Ok(None);
        }
        Ok(Some(base))
    }

    fn borrowed_heap_receiver(
        &self,
        receiver: UnitExpressionId,
        span: Span,
    ) -> Result<Option<LoanId>, LoweringError> {
        if receiver.source_unit() != self.source_unit {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let Some(symbol) = self.direct_name_symbol(receiver.expression(), span)? else {
            return Ok(None);
        };
        let Some(base) = self.borrow_bindings.get(&symbol).copied() else {
            return Ok(None);
        };
        let owner = self.expression_ssa_type(receiver.expression(), span)?;
        if self
            .function
            .entity(EntityId::Loan(base))
            .map(|entity| entity.ty)
            != Some(EntityType::Loan {
                kind: LoanKind::Shared,
                target: owner,
            })
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(Some(base))
    }

    pub(super) fn lower_owned_field_borrow_place(
        &mut self,
        expression: ExpressionId,
        place: &UnitOwnershipPlace,
        target: SsaTypeId,
        span: Span,
    ) -> Result<PlaceId, LoweringError> {
        let [symbol] = place.fields() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        if place.element().is_some() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let id = UnitExpressionId::new(self.source_unit, expression);
        let projection = self
            .typed
            .aggregate_projection(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        if projection.expression() != id
            || projection.kind() != UnitAggregateProjectionKind::Field
            || projection.field() != *symbol
            || self.typed.expression_type(id) != Some(projection.ty())
            || self.type_ids.get(&projection.ty()).copied() != Some(target)
            || receiver.source_unit() != self.source_unit
            || place.root().source_unit() != self.source_unit
            || self.direct_place_symbol(receiver.expression(), span)? != place.root()
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let root = place.root();
        let root_symbol = self.names.names().source_units()[root.source_unit().index()]
            .resolution()
            .symbols()
            .get(root.symbol().index())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if let Some(receiver_type) = self.typed.expression_type(receiver)
            && let Some(UnitTypeKind::EnumCase {
                root: owner_type, ..
            }) = self.typed.types().get(receiver_type)
        {
            if !matches!(
                root_symbol.kind(),
                SymbolKind::Variable | SymbolKind::ValueParameter
            ) || self.typed.symbol_type(root) != Some(*owner_type)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            let Some(LoweredValue::Value(owner)) = self.bindings.get(&root).copied() else {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            };
            if self.pending_call_frames.iter().any(|frame| {
                frame.field_replace_owner.is_some_and(|slot| {
                    self.pending_operands.get(slot) == Some(&EntityId::Value(owner))
                })
            }) {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            let field =
                self.lower_enum_field_place(projection, receiver_type, owner, target, span)?;
            self.pending_call_frames
                .last_mut()
                .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
                .shared_field_roots
                .push(root);
            return Ok(field);
        }
        if root_symbol.kind() != SymbolKind::Variable {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owner_type = self
            .typed
            .symbol_type(root)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if self.typed.expression_type(receiver) != Some(owner_type) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = self.typed.types().get(owner_type)
        else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let nominal = self
            .typed
            .signatures()
            .declaration(*declaration)
            .and_then(|signature| signature.nominal())
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if nominal.kind() != NominalKind::Class
            || !arguments.is_empty()
            || !nominal.type_parameters().is_empty()
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let Some(LoweredValue::Value(owner)) = self.bindings.get(&root).copied() else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        // SSA currently aliases sibling fields through their parent. Preserve the existing gate.
        if self.pending_call_frames.iter().any(|frame| {
            frame.field_replace_owner.is_some_and(|slot| {
                self.pending_operands.get(slot) == Some(&EntityId::Value(owner))
            })
        }) {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owner_ssa = self
            .type_ids
            .get(&owner_type)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if self
            .function
            .entity(EntityId::Value(owner))
            .map(|entity| entity.ty)
            != Some(EntityType::Value(owner_ssa))
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let payload = self
            .heap_payloads
            .get(&owner_ssa)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let field = self
            .field_indices
            .get(&(owner_type, *symbol))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if nominal
            .fields()
            .get(field)
            .is_none_or(|field| field.symbol() != *symbol || field.ty() != projection.ty())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let (_, roots) = self
            .function
            .append_instruction(
                self.block,
                Operation::HeapPayloadPlace { owner },
                vec![EntityType::Place(payload)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Place(base) = roots[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        let (_, fields) = self
            .function
            .append_instruction(
                self.block,
                Operation::FieldPlace { base, field },
                vec![EntityType::Place(target)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Place(field) = fields[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        self.pending_call_frames
            .last_mut()
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?
            .shared_field_roots
            .push(root);
        Ok(field)
    }
}
