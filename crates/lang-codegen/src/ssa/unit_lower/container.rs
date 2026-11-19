//! Field views consume validated projection identity and retain their parent owner loan.
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::SymbolKind,
    ownership_checking::{
        UnitDropPoint, UnitLoanTarget, UnitOwnershipPlace, UnitValueDeliveryKind,
    },
    parser::Expression,
    source::Span,
    type_checking::{
        BuiltinType, ContainerConstructionKind, Copyability, ExpressionCategory,
        IntrinsicTypeConstructor, NominalKind, ParameterMode, SequentialContainerKind,
        UnitAggregateProjectionKind, UnitAggregateProjectionReceiver, UnitElementPlaceDescriptor,
        UnitExpressionId, UnitTypeKind,
    },
};

use super::{LoweredValue, UnitExpressionLowerer, builtin_type, lowering_error, require_value};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{EntityId, EntityType, LoanId, LoanKind, Operation, Origin, PlaceId, SsaTypeId},
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
        Ok(Some(LoweredValue::Value(require_value(values[0], span)?)))
    }

    /// Only a published single-field LoanFact can form this child. The caller retains the
    /// parent parameter/capture loan and ends the returned child at this synchronous call.
    pub(super) fn lower_borrowed_field_loan(
        &mut self,
        _call: UnitExpressionId,
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
            return self.lower_borrowed_field_loan(_call, *expression, loan_target, target, span);
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
        if nominal.kind() != NominalKind::Class
            || !arguments.is_empty()
            || !nominal.type_parameters().is_empty()
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
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
                Operation::SharedHeapFieldLoan { base, field },
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
        Ok(Some(loan))
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
        Ok(field)
    }

    pub(super) fn lower_container_expression(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<LoweredValue>, LoweringError> {
        let unit_expression = UnitExpressionId::new(self.source_unit, expression);
        if self.typed.container_construction(unit_expression).is_some() {
            return self
                .lower_container_construction(expression, span)
                .map(Some);
        }
        if self.typed.container_size(unit_expression).is_some() {
            return self.lower_container_size(expression, span).map(Some);
        }
        if self.typed.element_place(unit_expression).is_some()
            || self.typed.container_append(unit_expression).is_some()
            || self.typed.container_clear(unit_expression).is_some()
            || self.typed.container_remove_at(unit_expression).is_some()
            || self.typed.container_remove_first(unit_expression).is_some()
            || self.typed.container_remove_last(unit_expression).is_some()
            || self.typed.container_insert_at(unit_expression).is_some()
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(None)
    }

    /// Read the header through the frontend's synchronous receiver loan.
    pub(super) fn lower_container_size(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let descriptor = self
            .typed
            .container_size(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let constructor = match descriptor.container() {
            SequentialContainerKind::Array => IntrinsicTypeConstructor::Array,
            SequentialContainerKind::List => IntrinsicTypeConstructor::List,
            SequentialContainerKind::MutableList => IntrinsicTypeConstructor::MutableList,
        };
        if descriptor.expression() != id
            || descriptor.receiver().source_unit() != self.source_unit
            || descriptor.span() != span
            || self.typed.expression_type(id) != Some(descriptor.result_type())
            || builtin_type(self.typed, descriptor.result_type()) != Some(BuiltinType::Int)
            || self.typed.expression_type(descriptor.receiver())
                != Some(descriptor.container_type())
            || !matches!(
                self.typed.types().get(descriptor.container_type()),
                Some(UnitTypeKind::Intrinsic { constructor: actual, arguments })
                    if *actual == constructor && arguments.as_slice() == [descriptor.element_type()]
            )
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let receiver = descriptor.receiver().expression();
        let receiver_span = self
            .parsed
            .ast()
            .expressions()
            .get(receiver)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?
            .span();
        let receiver_type = self.expression_ssa_type(receiver, receiver_span)?;
        let result_type = self.expression_ssa_type(expression, span)?;
        let (loan, created, _) =
            self.lower_borrow_argument(id, receiver, receiver_type, receiver_span, span)?;
        let result = self.append_scalar(
            Operation::ContainerLength {
                owner: EntityId::Loan(loan),
            },
            result_type,
            span,
        )?;
        for loan in created.into_iter().rev() {
            self.function
                .append_instruction(
                    self.block,
                    Operation::BorrowEnd { loan },
                    Vec::new(),
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        }
        self.emit_drops(UnitDropPoint::CallReturn(id))?;
        Ok(result)
    }

    pub(super) fn element_place_descriptor(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<UnitElementPlaceDescriptor>, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        if let Some(descriptor) = self.typed.element_place(id) {
            return Ok(Some(descriptor));
        }
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        match node.payload() {
            Expression::Group { expression } => self.element_place_descriptor(*expression),
            _ => Ok(None),
        }
    }

    pub(super) fn lower_container_construction(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let descriptor = self
            .typed
            .container_construction(id)
            .filter(|descriptor| descriptor.expression() == id)
            .cloned()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Expression::Call { arguments, .. } = node.payload() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if descriptor.kind() == ContainerConstructionKind::RuntimeLength {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        self.validate_container_construction(&descriptor, arguments, span)?;

        let pending_start = self.pending_operands.len();
        for argument in arguments {
            let value = match self.lower(argument.value)? {
                LoweredValue::Value(value) => value,
                LoweredValue::Diverged => {
                    self.pending_operands.truncate(pending_start);
                    return Ok(LoweredValue::Diverged);
                }
                LoweredValue::Unit => self.materialize_unit_value(argument.value, argument.span)?,
            };
            self.consume_container_delivery(id, argument.value, value, argument.span)?;
            let argument_id = UnitExpressionId::new(self.source_unit, argument.value);
            self.retain_pending_construction_operand(argument_id, value, argument.span)?;
            self.pending_operands.push(EntityId::Value(value));
        }

        let elements = self.commit_construction_operands(
            pending_start..self.pending_operands.len(),
            pending_start,
            span,
        )?;

        let container = self.expression_ssa_type(expression, span)?;
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::ContainerConstruct {
                    container,
                    elements,
                },
                vec![EntityType::Value(container)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(LoweredValue::Value(require_value(results[0], span)?))
    }

    fn validate_container_construction(
        &self,
        descriptor: &lang_frontend::type_checking::UnitContainerConstructionDescriptor,
        arguments: &[lang_frontend::parser::CallArgument],
        span: Span,
    ) -> Result<(), LoweringError> {
        let constructor = match descriptor.container() {
            SequentialContainerKind::Array => IntrinsicTypeConstructor::Array,
            SequentialContainerKind::List => IntrinsicTypeConstructor::List,
            SequentialContainerKind::MutableList => IntrinsicTypeConstructor::MutableList,
        };
        let Some(UnitTypeKind::Intrinsic {
            constructor: actual,
            arguments: type_arguments,
        }) = self.typed.types().get(descriptor.container_type())
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if self.typed.expression_type(descriptor.expression()) != Some(descriptor.container_type())
            || self.typed.expression_category(descriptor.expression())
                != Some(ExpressionCategory::Temporary)
            || *actual != constructor
            || type_arguments.as_slice() != [descriptor.element_type()]
            || arguments.len() != descriptor.parameter_modes().len()
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        match descriptor.kind() {
            ContainerConstructionKind::ListForm => {
                if descriptor
                    .parameter_modes()
                    .iter()
                    .any(|mode| *mode != ParameterMode::Value)
                    || arguments.iter().any(|argument| {
                        argument.named_prefix.is_some() || argument.mode_marker.is_some()
                    })
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
            }
            ContainerConstructionKind::EmptyMutableList => {
                if descriptor.container() != SequentialContainerKind::MutableList
                    || !arguments.is_empty()
                    || !descriptor.parameter_modes().is_empty()
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
            }
            ContainerConstructionKind::RuntimeLength => {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
        }
        Ok(())
    }

    fn consume_container_delivery(
        &mut self,
        call: UnitExpressionId,
        argument: ExpressionId,
        value: crate::ssa::model::ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let argument = UnitExpressionId::new(self.source_unit, argument);
        let mut deliveries = self
            .owned
            .value_deliveries()
            .iter()
            .filter(|delivery| delivery.call() == call && delivery.argument() == argument);
        let delivery = deliveries
            .next()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let ty = self
            .typed
            .expression_type(argument)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let expected = match (
            self.typed.expression_category(argument),
            self.typed.copyability(ty),
        ) {
            (Some(ExpressionCategory::Temporary), _) => UnitValueDeliveryKind::Temporary,
            (Some(ExpressionCategory::Place), Copyability::Copyable) => UnitValueDeliveryKind::Copy,
            (Some(ExpressionCategory::Place), Copyability::MoveOnly) => UnitValueDeliveryKind::Move,
            _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        if deliveries.next().is_some() || delivery.kind() != expected {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        match expected {
            UnitValueDeliveryKind::Copy => Ok(()),
            UnitValueDeliveryKind::Move => {
                let place = delivery
                    .place()
                    .filter(|place| place.is_root())
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                self.take_owned_binding(place.root(), value, span)
            }
            UnitValueDeliveryKind::Temporary => {
                self.validate_temporary_delivery_origin(argument, delivery.source(), span)?;
                if self.typed.copyability(ty) == Copyability::MoveOnly {
                    self.take_owned_temporary(value, span)?;
                }
                Ok(())
            }
        }
    }
}
