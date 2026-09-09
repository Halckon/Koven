//! concrete sequential-container construction lowering。

use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{
        UnitDropPoint, UnitDropTarget, UnitLoanTarget, UnitValueDeliveryKind,
        UnitValueDeliverySource,
    },
    parser::{AssignmentOperator, Expression, ParameterModeMarker},
    source::Span,
    type_checking::{
        BuiltinType, ContainerConstructionKind, Copyability, ExpressionCategory,
        IntrinsicTypeConstructor, ParameterMode, SequentialContainerKind,
        UnitElementPlaceDescriptor, UnitExpressionId, UnitTypeKind,
    },
};

use super::{
    LoweredValue, UnitExpressionLowerer, builtin_type, lowering_error, require_value,
    resolve_concrete_type, scalar::is_integer_builtin, span_key,
};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{
        CheckedArithmeticOperator, EntityId, EntityType, Operation, Origin, PlaceAccess, PlaceId,
        SsaTypeId, ValueId,
    },
};

struct ElementOperands {
    owner: EntityId,
    root: Option<lang_frontend::name_resolution::UnitSymbolId>,
    index: ValueId,
    element: SsaTypeId,
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn element_place_descriptor(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<UnitElementPlaceDescriptor>, LoweringError> {
        let id = UnitExpressionId::new(self.source_unit, expression);
        if let Some(descriptor) = self.typed.types().element_place(id) {
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

    pub(super) fn lower_container_index(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .element_place_descriptor(expression)?
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let element = resolve_concrete_type(
            self.typed,
            descriptor.element_type(),
            self.substitutions,
            self.static_self,
            span,
        )?;
        if self.typed.types().copyability(element) != Copyability::Copyable
            || builtin_type(self.typed, element) == Some(BuiltinType::Unit)
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let operands = self.lower_element_operands(descriptor, None, span)?;
        let place = self.append_element_place(&operands, span)?;
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::Read {
                    source: PlaceAccess::Place(place),
                },
                vec![EntityType::Value(operands.element)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(LoweredValue::Value(require_value(results[0], span)?))
    }

    pub(super) fn lower_borrowed_container_element(
        &mut self,
        expression: ExpressionId,
        target: &UnitLoanTarget,
        expected_element: SsaTypeId,
        span: Span,
    ) -> Result<Option<PlaceId>, LoweringError> {
        let Some(descriptor) = self.element_place_descriptor(expression)? else {
            return Ok(None);
        };
        let temporary_owner =
            self.temporary_expression_origin(descriptor.receiver().expression())?;
        let expected_root = match target {
            UnitLoanTarget::Place(place)
                if place.fields().is_empty() && place.element().is_some() =>
            {
                Some(place.root())
            }
            UnitLoanTarget::Temporary(owner) if Some(*owner) == temporary_owner => None,
            UnitLoanTarget::Place(_) | UnitLoanTarget::Temporary(_) => {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
        };
        let operands = self.lower_element_operands(descriptor, expected_root, span)?;
        if let UnitLoanTarget::Temporary(owner) = target {
            let EntityId::Value(value) = operands.owner else {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            };
            // Group lowering records the produced owner at the inner expression. Loan/drop facts use
            // their source-qualified temporary origin, so collapse that alias before CallReturn.
            let before = self.temporaries.len();
            self.temporaries.retain(|_, temporary| *temporary != value);
            if self.temporaries.len() == before {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            self.temporaries.insert(*owner, value);
        }
        if operands.element != expected_element {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.append_element_place(&operands, span).map(Some)
    }

    pub(super) fn lower_container_assignment(
        &mut self,
        expression: ExpressionId,
        target: ExpressionId,
        operator: AssignmentOperator,
        value: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let descriptor = self
            .element_place_descriptor(target)?
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if !descriptor.is_mutable() {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let concrete_element = resolve_concrete_type(
            self.typed,
            descriptor.element_type(),
            self.substitutions,
            self.static_self,
            span,
        )?;
        self.validate_replacement_drop(expression, concrete_element, span)?;
        let operands = self.lower_element_operands(descriptor, None, span)?;
        let EntityId::Value(original_owner) = operands.owner else {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        };
        let replacement = match operator {
            AssignmentOperator::Assign => match self.lower(value)? {
                LoweredValue::Value(value) => value,
                LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                LoweredValue::Unit => {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
            },
            AssignmentOperator::AddAssign
            | AssignmentOperator::SubtractAssign
            | AssignmentOperator::MultiplyAssign
            | AssignmentOperator::DivideAssign
            | AssignmentOperator::RemainderAssign => {
                if operands.root.is_none() {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
                if !builtin_type(self.typed, concrete_element).is_some_and(is_integer_builtin) {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
                let place = self.append_element_place(&operands, span)?;
                let (_, current) = self
                    .function
                    .append_instruction(
                        self.block,
                        Operation::Read {
                            source: PlaceAccess::Place(place),
                        },
                        vec![EntityType::Value(operands.element)],
                        Origin::Source(span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                let current = require_value(current[0], span)?;
                let rhs = match self.lower(value)? {
                    LoweredValue::Value(value) => value,
                    LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                    LoweredValue::Unit => {
                        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                    }
                };
                match self.checked(
                    container_assignment_operator(operator),
                    current,
                    rhs,
                    operands.element,
                    span,
                )? {
                    LoweredValue::Value(value) => value,
                    LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
                    LoweredValue::Unit => {
                        return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                    }
                }
            }
        };
        if self.expression_ssa_type(value, span)? != operands.element {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.transfer_owned_expression(value, replacement, span)?;
        let owner = if let Some(root) = operands.root {
            match self.bindings.get(&root).copied() {
                Some(LoweredValue::Value(owner)) => owner,
                Some(LoweredValue::Unit | LoweredValue::Diverged) | None => {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
            }
        } else {
            original_owner
        };
        self.function
            .append_instruction(
                self.block,
                Operation::ContainerReplace {
                    owner,
                    index: operands.index,
                    value: replacement,
                },
                Vec::new(),
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(LoweredValue::Unit)
    }

    fn lower_element_operands(
        &mut self,
        descriptor: UnitElementPlaceDescriptor,
        expected_root: Option<lang_frontend::name_resolution::UnitSymbolId>,
        span: Span,
    ) -> Result<ElementOperands, LoweringError> {
        if descriptor.expression().source_unit() != self.source_unit
            || descriptor.receiver().source_unit() != self.source_unit
            || descriptor.index().source_unit() != self.source_unit
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let (owner, root) =
            self.container_owner_operand(descriptor.receiver().expression(), expected_root, span)?;
        let index = match self.lower(descriptor.index().expression())? {
            LoweredValue::Value(index) => index,
            LoweredValue::Unit | LoweredValue::Diverged => {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
        };
        let concrete = resolve_concrete_type(
            self.typed,
            descriptor.element_type(),
            self.substitutions,
            self.static_self,
            span,
        )?;
        let element = self
            .type_ids
            .get(&concrete)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        Ok(ElementOperands {
            owner,
            root,
            index,
            element,
        })
    }

    fn container_owner_operand(
        &mut self,
        expression: ExpressionId,
        expected_root: Option<lang_frontend::name_resolution::UnitSymbolId>,
        span: Span,
    ) -> Result<
        (
            EntityId,
            Option<lang_frontend::name_resolution::UnitSymbolId>,
        ),
        LoweringError,
    > {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        match node.payload() {
            Expression::Group { expression } => {
                self.container_owner_operand(*expression, expected_root, span)
            }
            Expression::Name => {
                let symbol = self
                    .references
                    .get(&span_key(node.span()))
                    .copied()
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, node.span()))?;
                if expected_root.is_some_and(|expected| expected != symbol) {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, node.span()));
                }
                if let Some(LoweredValue::Value(owner)) = self.bindings.get(&symbol).copied() {
                    return Ok((EntityId::Value(owner), Some(symbol)));
                }
                self.borrow_bindings
                    .get(&symbol)
                    .copied()
                    .map(|loan| (EntityId::Loan(loan), Some(symbol)))
                    .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, node.span()))
            }
            _ if expected_root.is_some() => Err(lowering_error(
                LoweringErrorKind::UnsupportedNode,
                node.span(),
            )),
            _ => match self.lower(expression)? {
                LoweredValue::Value(owner) => Ok((EntityId::Value(owner), None)),
                LoweredValue::Unit | LoweredValue::Diverged => Err(lowering_error(
                    LoweringErrorKind::UnsupportedNode,
                    node.span(),
                )),
            },
        }
    }

    fn temporary_expression_origin(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<UnitExpressionId>, LoweringError> {
        let unit = UnitExpressionId::new(self.source_unit, expression);
        if self.typed.types().expression_category(unit) == Some(ExpressionCategory::Temporary) {
            return Ok(Some(unit));
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
            Expression::Group { expression } => self.temporary_expression_origin(*expression),
            _ => Ok(None),
        }
    }

    fn append_element_place(
        &mut self,
        operands: &ElementOperands,
        span: Span,
    ) -> Result<PlaceId, LoweringError> {
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::ContainerElementPlace {
                    owner: operands.owner,
                    index: operands.index,
                },
                vec![EntityType::Place(operands.element)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Place(place) = results[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        Ok(place)
    }

    fn validate_replacement_drop(
        &self,
        expression: ExpressionId,
        element: lang_frontend::type_checking::UnitTypeId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let expression = UnitExpressionId::new(self.source_unit, expression);
        let facts = self
            .owned
            .ownership()
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.point() == UnitDropPoint::AfterReplacement(expression))
            .collect::<Vec<_>>();
        let expected = self.typed.types().copyability(element) == Copyability::MoveOnly;
        if (expected
            && matches!(facts.as_slice(), [fact] if fact.target() == UnitDropTarget::ReplacedElement(expression)))
            || (!expected && facts.is_empty())
        {
            return Ok(());
        }
        Err(lowering_error(LoweringErrorKind::MissingFact, span))
    }

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
            self.pending_operands.push(EntityId::Value(value));
        }

        let elements = self.pending_operands[pending_start..]
            .iter()
            .copied()
            .map(|entity| require_value(entity, span))
            .collect::<Result<Vec<_>, _>>()?;
        self.pending_operands.truncate(pending_start);

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

fn container_assignment_operator(operator: AssignmentOperator) -> CheckedArithmeticOperator {
    match operator {
        AssignmentOperator::AddAssign => CheckedArithmeticOperator::Add,
        AssignmentOperator::SubtractAssign => CheckedArithmeticOperator::Subtract,
        AssignmentOperator::MultiplyAssign => CheckedArithmeticOperator::Multiply,
        AssignmentOperator::DivideAssign => CheckedArithmeticOperator::Divide,
        AssignmentOperator::RemainderAssign => CheckedArithmeticOperator::Remainder,
        AssignmentOperator::Assign => unreachable!("plain element assignment is not arithmetic"),
    }
}
