//! concrete sequential-container construction lowering。

use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{UnitDropPoint, UnitValueDeliveryKind},
    parser::Expression,
    source::Span,
    type_checking::{
        BuiltinType, ContainerConstructionKind, Copyability, ExpressionCategory,
        IntrinsicTypeConstructor, ParameterMode, SequentialContainerKind,
        UnitElementPlaceDescriptor, UnitExpressionId, UnitTypeKind,
    },
};

use super::{LoweredValue, UnitExpressionLowerer, builtin_type, lowering_error, require_value};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{EntityId, EntityType, Operation, Origin},
};

impl UnitExpressionLowerer<'_> {
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
