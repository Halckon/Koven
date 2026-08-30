//! concrete sequential-container construction lowering。

use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{UnitValueDeliveryKind, UnitValueDeliverySource},
    parser::{Expression, ParameterModeMarker},
    source::Span,
    type_checking::{
        BuiltinType, ContainerConstructionKind, Copyability, ExpressionCategory,
        IntrinsicTypeConstructor, ParameterMode, SequentialContainerKind, UnitExpressionId,
        UnitTypeKind,
    },
};

use super::{LoweredValue, UnitExpressionLowerer, lowering_error, require_value};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{EntityType, Operation, Origin, ScalarConstant},
};

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_container_construction(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        let descriptor = self
            .typed
            .types()
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
        self.validate_container_construction(&descriptor, arguments, span)?;
        if descriptor.kind() == ContainerConstructionKind::RuntimeLength {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }

        let mut elements = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let value = match self.lower(argument.value)? {
                LoweredValue::Value(value) => value,
                LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                LoweredValue::Unit => {
                    let argument_id = UnitExpressionId::new(self.source_unit, argument.value);
                    let argument_type = self
                        .typed
                        .types()
                        .expression_type(argument_id)
                        .ok_or_else(|| {
                            lowering_error(LoweringErrorKind::MissingFact, argument.span)
                        })?;
                    if !matches!(
                        self.typed.types().types().get(argument_type),
                        Some(UnitTypeKind::Builtin(BuiltinType::Unit))
                    ) {
                        return Err(lowering_error(
                            LoweringErrorKind::MissingFact,
                            argument.span,
                        ));
                    }
                    let unit = self.expression_ssa_type(argument.value, argument.span)?;
                    let (_, results) = self
                        .function
                        .append_instruction(
                            self.block,
                            Operation::Constant(ScalarConstant::Unit),
                            vec![EntityType::Value(unit)],
                            Origin::Source(argument.span),
                        )
                        .map_err(|_| {
                            lowering_error(LoweringErrorKind::InvalidModel, argument.span)
                        })?;
                    require_value(results[0], argument.span)?
                }
            };
            self.consume_container_delivery(id, argument.value, value, argument.span)?;
            elements.push(value);
        }

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
        }) = self.typed.types().types().get(descriptor.container_type())
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if self.typed.types().expression_type(descriptor.expression())
            != Some(descriptor.container_type())
            || self
                .typed
                .types()
                .expression_category(descriptor.expression())
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
                if !matches!(
                    descriptor.container(),
                    SequentialContainerKind::Array | SequentialContainerKind::List
                ) || descriptor.parameter_modes()
                    != [ParameterMode::Borrow, ParameterMode::Borrow]
                    || arguments.iter().any(|argument| {
                        argument.named_prefix.is_some()
                            || !matches!(
                                argument.mode_marker,
                                None | Some(ParameterModeMarker::Borrow(_))
                            )
                    })
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
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
            .ownership()
            .value_deliveries()
            .iter()
            .filter(|delivery| delivery.call() == call && delivery.argument() == argument);
        let delivery = deliveries
            .next()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let ty = self
            .typed
            .types()
            .expression_type(argument)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let expected = match (
            self.typed.types().expression_category(argument),
            self.typed.types().copyability(ty),
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
                if delivery.source() != &UnitValueDeliverySource::Temporary(argument) {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
                if self.typed.types().copyability(ty) == Copyability::MoveOnly {
                    self.take_owned_temporary(value, span)?;
                }
                Ok(())
            }
        }
    }
}
