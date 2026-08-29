//! Place、projection 与 Value delivery 的 source-qualified 所有权规则。

use crate::{
    ast::ExpressionId,
    diagnostic::{Diagnostic, Severity},
    ownership_checking::{ElementIndexIdentity, UnitValueDeliveryKind, UnitValueDeliverySource},
    parser::{Expression, LiteralKind, PrefixOperator, VariableKind},
    type_checking::{
        Copyability, ExpressionCategory, NominalKind, RcOperationKind, UnitAggregateProjectionKind,
        UnitExpressionId, UnitTypeKind,
    },
};

use super::{
    AccessKind, Checker, OwnershipBindingKind, OwnershipCheckingError, State,
    UnitCallArgumentOwnershipContract, UnitOwnershipPlace, add_parameter_label,
};

impl Checker<'_> {
    pub(super) fn value_delivery(
        &mut self,
        contract: UnitCallArgumentOwnershipContract,
        state: &mut State,
    ) -> Result<Option<(UnitValueDeliveryKind, UnitValueDeliverySource)>, OwnershipCheckingError>
    {
        if contract.category() == ExpressionCategory::Temporary {
            return Ok(Some((
                UnitValueDeliveryKind::Temporary,
                UnitValueDeliverySource::Temporary(contract.argument()),
            )));
        }
        let expression = contract.argument().expression();
        let Some(ty) = self.typed.expression_type(contract.argument()) else {
            return Err(OwnershipCheckingError::InvalidUnitArgumentType {
                source_unit: self.source_unit.index(),
                expression: expression.index(),
            });
        };
        let kind = match self.typed.copyability(ty) {
            Copyability::Copyable => UnitValueDeliveryKind::Copy,
            Copyability::MoveOnly => UnitValueDeliveryKind::Move,
            Copyability::Unknown | Copyability::Error => {
                return Err(OwnershipCheckingError::InvalidUnitArgumentType {
                    source_unit: self.source_unit.index(),
                    expression: expression.index(),
                });
            }
        };
        if let Some(receiver) = self.rc_value_receiver(expression)? {
            if kind == UnitValueDeliveryKind::Move {
                let mut diagnostic = Diagnostic::new(
                    self.sources,
                    Severity::Error,
                    self.codes.partial_move,
                    "cannot move a non-Copyable payload out of Rc",
                    contract.argument_span(),
                )?;
                diagnostic.add_label(
                    self.sources,
                    self.parsed.ast().expressions().get(receiver)?.span(),
                    "Rc owner remains responsible for the payload",
                )?;
                add_parameter_label(self.sources, &mut diagnostic, contract.parameter_span())?;
                self.diagnostics.push(diagnostic);
                return Ok(None);
            }
            return Ok(Some((
                kind,
                UnitValueDeliverySource::BorrowedProjection {
                    owner: self.unit_expression(receiver),
                },
            )));
        }
        if self.element_place_descriptor(expression)?.is_some() {
            if kind == UnitValueDeliveryKind::Move {
                let place = self.place(expression)?;
                if let Some(place) = &place
                    && !self.access_place(
                        place,
                        AccessKind::Move,
                        contract.argument_span(),
                        contract.parameter_span(),
                        state,
                    )?
                {
                    return Ok(None);
                }
                let mut diagnostic = Diagnostic::new(
                    self.sources,
                    Severity::Error,
                    self.codes.container_element_move,
                    "cannot move a non-Copyable element out of a sequential container",
                    contract.argument_span(),
                )?;
                if let Some(place) = place {
                    diagnostic.add_label(
                        self.sources,
                        self.symbol_span(place.root())?,
                        "container owner remains responsible for every initialized element",
                    )?;
                } else if let Some(owner) = self.temporary_element_owner(expression)? {
                    diagnostic.add_label(
                        self.sources,
                        self.parsed
                            .ast()
                            .expressions()
                            .get(owner.expression())?
                            .span(),
                        "temporary container owns every initialized element",
                    )?;
                }
                add_parameter_label(self.sources, &mut diagnostic, contract.parameter_span())?;
                self.diagnostics.push(diagnostic);
                return Ok(None);
            }
            if let Some(place) = self.place(expression)? {
                self.access_place(
                    &place,
                    AccessKind::Read,
                    contract.argument_span(),
                    contract.parameter_span(),
                    state,
                )?;
                return Ok(Some((kind, UnitValueDeliverySource::Place(place))));
            }
            if let Some(owner) = self.temporary_element_owner(expression)? {
                return Ok(Some((
                    kind,
                    UnitValueDeliverySource::BorrowedProjection { owner },
                )));
            }
        }
        let Some(place) = self.place(expression)? else {
            return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                source_unit: self.source_unit.index(),
                expression: expression.index(),
            });
        };
        Ok(Some((kind, UnitValueDeliverySource::Place(place))))
    }

    pub(super) fn loan_place(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<UnitOwnershipPlace>, OwnershipCheckingError> {
        if let Some(receiver) = self.rc_value_receiver(expression)? {
            return self.place(receiver);
        }
        self.place(expression)
    }

    fn rc_value_receiver(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<ExpressionId>, OwnershipCheckingError> {
        if let Some(operation) = self.typed.rc_operation(self.unit_expression(expression))
            && operation.kind() == RcOperationKind::Value
        {
            if operation.receiver().source_unit() != self.source_unit {
                return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                    source_unit: self.source_unit.index(),
                    expression: expression.index(),
                });
            }
            return Ok(Some(operation.receiver().expression()));
        }
        match self.parsed.ast().expressions().get(expression)?.payload() {
            Expression::Group { expression } => self.rc_value_receiver(*expression),
            _ => Ok(None),
        }
    }

    fn element_place_descriptor(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<crate::type_checking::UnitElementPlaceDescriptor>, OwnershipCheckingError>
    {
        if let Some(descriptor) = self.typed.element_place(self.unit_expression(expression)) {
            return Ok(Some(descriptor));
        }
        match self.parsed.ast().expressions().get(expression)?.payload() {
            Expression::Group { expression } => self.element_place_descriptor(*expression),
            _ => Ok(None),
        }
    }

    pub(super) fn temporary_element_owner(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<UnitExpressionId>, OwnershipCheckingError> {
        let Some(descriptor) = self.element_place_descriptor(expression)? else {
            return Ok(None);
        };
        let receiver = descriptor.receiver();
        if receiver.source_unit() != self.source_unit {
            return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                source_unit: self.source_unit.index(),
                expression: expression.index(),
            });
        }
        self.temporary_expression_origin(receiver.expression())
    }

    pub(super) fn temporary_projection_owner(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<UnitExpressionId>, OwnershipCheckingError> {
        if let Some(receiver) = self.rc_value_receiver(expression)?
            && let Some(owner) = self.temporary_expression_origin(receiver)?
        {
            return Ok(Some(owner));
        }
        self.temporary_element_owner(expression)
    }

    pub(super) fn temporary_expression_origin(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<UnitExpressionId>, OwnershipCheckingError> {
        let unit = self.unit_expression(expression);
        if self.typed.expression_category(unit) == Some(ExpressionCategory::Temporary) {
            return Ok(Some(unit));
        }
        match self.parsed.ast().expressions().get(expression)?.payload() {
            Expression::Group { expression } => self.temporary_expression_origin(*expression),
            _ => Ok(None),
        }
    }

    pub(super) fn place(
        &self,
        expression: ExpressionId,
    ) -> Result<Option<UnitOwnershipPlace>, OwnershipCheckingError> {
        let node = self.parsed.ast().expressions().get(expression)?;
        match node.payload() {
            Expression::Name => Ok(self
                .reference_symbol(node.span())
                .filter(|symbol| self.is_place_symbol(*symbol))
                .map(|symbol| UnitOwnershipPlace::new(symbol, Vec::new()))),
            Expression::Group { expression } => self.place(*expression),
            Expression::Member { .. } => {
                let Some(projection) = self
                    .typed
                    .aggregate_projection(self.unit_expression(expression))
                else {
                    return Ok(None);
                };
                if projection.kind() != UnitAggregateProjectionKind::Field {
                    return Ok(None);
                }
                match projection.receiver() {
                    crate::type_checking::UnitAggregateProjectionReceiver::Expression(receiver) => {
                        if receiver.source_unit() != self.source_unit {
                            return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                                source_unit: self.source_unit.index(),
                                expression: expression.index(),
                            });
                        }
                        let Some(mut place) = self.place(receiver.expression())? else {
                            return Ok(None);
                        };
                        Ok(place.push_field(projection.field()).then_some(place))
                    }
                    crate::type_checking::UnitAggregateProjectionReceiver::This(_) => Ok(Some(
                        UnitOwnershipPlace::new(projection.field(), Vec::new()),
                    )),
                }
            }
            Expression::Index { .. } => {
                let Some(descriptor) = self.element_place_descriptor(expression)? else {
                    return Ok(None);
                };
                if descriptor.receiver().source_unit() != self.source_unit
                    || descriptor.index().source_unit() != self.source_unit
                {
                    return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                        source_unit: self.source_unit.index(),
                        expression: expression.index(),
                    });
                }
                let Some(mut place) = self.place(descriptor.receiver().expression())? else {
                    return Ok(None);
                };
                let index = self.element_index_identity(descriptor.index().expression())?;
                Ok(place.push_element(index).then_some(place))
            }
            _ => Ok(None),
        }
    }

    fn element_index_identity(
        &self,
        expression: ExpressionId,
    ) -> Result<ElementIndexIdentity, OwnershipCheckingError> {
        let node = self.parsed.ast().expressions().get(expression)?;
        let value = match node.payload() {
            Expression::Literal(LiteralKind::Integer(_)) => self
                .sources
                .slice(node.span())?
                .trim_end_matches(['L', 'l', 'U', 'u'])
                .parse::<i128>()
                .ok(),
            Expression::Group { expression } => return self.element_index_identity(*expression),
            Expression::Prefix {
                operator: PrefixOperator::Minus,
                operand,
                ..
            } => match self.element_index_identity(*operand)? {
                ElementIndexIdentity::Known(value) => value.checked_neg(),
                ElementIndexIdentity::Unknown => None,
            },
            Expression::Prefix {
                operator: PrefixOperator::Plus,
                operand,
                ..
            } => match self.element_index_identity(*operand)? {
                ElementIndexIdentity::Known(value) => Some(value),
                ElementIndexIdentity::Unknown => None,
            },
            _ => None,
        };
        Ok(value.map_or(ElementIndexIdentity::Unknown, ElementIndexIdentity::Known))
    }

    pub(super) fn is_mutable_place(
        &self,
        expression: ExpressionId,
    ) -> Result<bool, OwnershipCheckingError> {
        let node = self.parsed.ast().expressions().get(expression)?;
        match node.payload() {
            Expression::Name => {
                let Some(symbol) = self.reference_symbol(node.span()) else {
                    return Ok(false);
                };
                if self.bindings.get(&symbol).map(|binding| binding.kind())
                    == Some(OwnershipBindingKind::Exclusive)
                {
                    return Ok(true);
                }
                Ok(self.variable_kinds.get(&symbol) == Some(&VariableKind::Var))
            }
            Expression::Group { expression } => self.is_mutable_place(*expression),
            Expression::Member { .. } => {
                if self.rc_value_receiver(expression)?.is_some() {
                    return Ok(false);
                }
                let Some(projection) = self
                    .typed
                    .aggregate_projection(self.unit_expression(expression))
                else {
                    return Ok(false);
                };
                if projection.kind() != UnitAggregateProjectionKind::Field
                    || self.field_kinds.get(&projection.field()) != Some(&VariableKind::Var)
                {
                    return Ok(false);
                }
                let crate::type_checking::UnitAggregateProjectionReceiver::Expression(receiver) =
                    projection.receiver()
                else {
                    return Ok(false);
                };
                if receiver.source_unit() != self.source_unit {
                    return Ok(false);
                }
                if self.expression_nominal_kind(receiver.expression()) == Some(NominalKind::Class) {
                    let Some(place) = self.place(receiver.expression())? else {
                        return Ok(false);
                    };
                    return Ok(self
                        .bindings
                        .get(&place.root())
                        .map(|binding| binding.kind())
                        != Some(OwnershipBindingKind::Shared));
                }
                self.is_mutable_place(receiver.expression())
            }
            Expression::Index { .. } => {
                let Some(descriptor) = self.element_place_descriptor(expression)? else {
                    return Ok(false);
                };
                if !descriptor.is_mutable() {
                    return Ok(false);
                }
                let Some(place) = self.place(expression)? else {
                    return Ok(self.temporary_element_owner(expression)?.is_some());
                };
                Ok(self
                    .bindings
                    .get(&place.root())
                    .map(|binding| binding.kind())
                    != Some(OwnershipBindingKind::Shared))
            }
            _ => Ok(false),
        }
    }

    fn expression_nominal_kind(&self, expression: ExpressionId) -> Option<NominalKind> {
        let mut ty = self
            .typed
            .expression_type(self.unit_expression(expression))?;
        while let UnitTypeKind::Nullable(inner) = self.typed.types().get(ty)? {
            ty = *inner;
        }
        let UnitTypeKind::Nominal { declaration, .. } = self.typed.types().get(ty)? else {
            return None;
        };
        self.typed
            .signatures()
            .declaration(*declaration)?
            .nominal()
            .map(|nominal| nominal.kind())
    }
}
