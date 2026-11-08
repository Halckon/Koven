//! Type contracts and publication for container subscripts.

use crate::{
    ast::ExpressionId,
    diagnostic::codes,
    name_resolution::SourceUnitId,
    parser::Expression,
    type_checking::{
        BuiltinType, CompilationUnitTypeError, DeferredReason, SequentialContainerKind,
        TypeCheckingError, UnitElementPlaceDescriptor, UnitExpressionId, UnitTypeId,
    },
};

use super::{BodyChecker, ExpressionCheck};

impl BodyChecker<'_> {
    pub(super) fn check_index(
        &mut self,
        expression: UnitExpressionId,
        receiver: ExpressionId,
        index: ExpressionId,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let source = expression.source_unit();
        let deferred = self.deferred_facts.len();
        let checked = self.check_expression(source, receiver, None, None, return_type)?;
        if let Some(result) =
            self.check_map_index(expression, receiver, checked, index, return_type, deferred)?
        {
            return Ok(result);
        }
        self.check_sequential_index(expression, receiver, index, return_type, checked, deferred)
    }

    fn check_sequential_index(
        &mut self,
        expression: UnitExpressionId,
        receiver: ExpressionId,
        index: ExpressionId,
        return_type: UnitTypeId,
        receiver_result: ExpressionCheck,
        deferred: usize,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let source = expression.source_unit();
        let Some((container, element)) = self.container_parts(receiver_result.ty) else {
            self.check_expression(source, index, None, None, return_type)?;
            return Ok(ExpressionCheck {
                ty: self.deferred_type(DeferredReason::Index),
                falls_through: receiver_result.falls_through,
            });
        };
        let int = self.builtin(BuiltinType::Int);
        let index_result = self.check_expression(source, index, None, None, return_type)?;
        let falls_through = receiver_result.falls_through && index_result.falls_through;
        if !self.assignable(index_result.ty, int)
            && !self.is_error(index_result.ty)
            && !self.is_deferred(index_result.ty)
        {
            self.emit(
                codes::INVALID_CONTAINER_INDEX,
                "sequential container index must have type Int",
                self.file(source)
                    .ast()
                    .expressions()
                    .get(index)
                    .map_err(TypeCheckingError::from)?
                    .span(),
            )?;
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through,
            });
        }
        if self.is_error(index_result.ty) {
            return Ok(ExpressionCheck {
                ty: self.error_type(),
                falls_through,
            });
        }
        if self.is_deferred(index_result.ty) {
            return Ok(ExpressionCheck {
                ty: self.deferred_type(DeferredReason::Index),
                falls_through,
            });
        }
        if self.construction_type_contains_poison(element) {
            return Ok(ExpressionCheck {
                ty: self.deferred_type(DeferredReason::Index),
                falls_through,
            });
        }
        if self.has_pending_storage_operands(source, receiver, index, deferred) {
            self.note_pending_storage_value(expression, element);
            return Ok(ExpressionCheck {
                ty: element,
                falls_through,
            });
        }
        self.parts
            .element_places
            .push(UnitElementPlaceDescriptor::new(
                expression,
                UnitExpressionId::new(source, receiver),
                UnitExpressionId::new(source, index),
                container,
                element,
            ));
        Ok(ExpressionCheck {
            ty: element,
            falls_through,
        })
    }

    pub(super) fn sequential_element_contract(
        &self,
        source: SourceUnitId,
        expression: ExpressionId,
    ) -> Option<(SequentialContainerKind, UnitTypeId)> {
        let key = UnitExpressionId::new(source, expression);
        if let Some(place) = self
            .parts
            .element_places
            .iter()
            .copied()
            .find(|place| place.expression() == key)
        {
            return Some((place.container(), place.element_type()));
        }
        match self
            .file(source)
            .ast()
            .expressions()
            .get(expression)
            .ok()?
            .payload()
        {
            Expression::Group { expression } => {
                self.sequential_element_contract(source, *expression)
            }
            Expression::Index { receiver, .. } if self.storage_value_is_pending(key) => {
                // Type checking still needs the element and mutability for a
                // pending write or inout argument. No executable place exists.
                self.checked_sequential_receiver(source, *receiver)
            }
            _ => None,
        }
    }

    pub(super) fn is_known_read_only_index(
        &self,
        source: SourceUnitId,
        expression: ExpressionId,
    ) -> bool {
        let Ok(node) = self.file(source).ast().expressions().get(expression) else {
            return false;
        };
        match node.payload() {
            Expression::Group { expression } => self.is_known_read_only_index(source, *expression),
            Expression::Index { receiver, .. } => self
                .checked_sequential_receiver(source, *receiver)
                .is_some_and(|(container, _)| !container.elements_are_mutable()),
            _ => false,
        }
    }

    fn checked_sequential_receiver(
        &self,
        source: SourceUnitId,
        receiver: ExpressionId,
    ) -> Option<(SequentialContainerKind, UnitTypeId)> {
        let ty = *self
            .parts
            .expression_types
            .get(&UnitExpressionId::new(source, receiver))?;
        self.container_parts(ty)
    }
}
