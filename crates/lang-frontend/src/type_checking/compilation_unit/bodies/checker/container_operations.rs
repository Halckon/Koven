//! SPEC-0197 compilation-unit 顺序容器 index/place/member/assignment 类型事实。

use crate::{
    ast::ExpressionId,
    diagnostic::codes,
    name_resolution::SourceUnitId,
    parser::{AssignmentOperator, Expression},
    source::Span,
    type_checking::{
        BuiltinType, CompilationUnitTypeError, DeferredReason, TypeCheckingError,
        UnitElementPlaceDescriptor, UnitExpressionId, UnitTypeId,
    },
};

use super::{BodyChecker, ExpressionCheck};

impl BodyChecker<'_> {
    pub(super) fn check_container_index(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        receiver: ExpressionId,
        index: ExpressionId,
        return_type: UnitTypeId,
    ) -> Result<ExpressionCheck, CompilationUnitTypeError> {
        let receiver_result = self.check_expression(source, receiver, None, None, return_type)?;
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
        self.parts
            .element_places
            .push(UnitElementPlaceDescriptor::new(
                UnitExpressionId::new(source, expression),
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

    #[allow(clippy::too_many_arguments)]
    pub(super) fn check_container_assignment(
        &mut self,
        source: SourceUnitId,
        target: ExpressionId,
        operator: AssignmentOperator,
        operator_span: Span,
        value: ExpressionId,
        return_type: UnitTypeId,
    ) -> Result<Option<ExpressionCheck>, CompilationUnitTypeError> {
        self.check_expression(source, target, None, None, return_type)?;
        if let Some(place) = self.element_place_for_expression(source, target) {
            let mut valid = place.is_mutable();
            if !valid {
                self.emit(
                    codes::IMMUTABLE_CONTAINER_PLACE,
                    "List element place is read-only",
                    operator_span,
                )?;
            }
            let value_result = self.check_expression(
                source,
                value,
                Some(place.element_type()),
                None,
                return_type,
            )?;
            valid &= !self.is_error(value_result.ty) && !self.is_deferred(value_result.ty);
            if operator != AssignmentOperator::Assign && !self.is_numeric(place.element_type()) {
                self.emit(
                    codes::INVALID_OPERAND_TYPES,
                    "compound element assignment requires a numeric element type",
                    operator_span,
                )?;
                valid = false;
            }
            return Ok(Some(ExpressionCheck {
                ty: if valid {
                    self.builtin(BuiltinType::Unit)
                } else {
                    self.error_type()
                },
                falls_through: value_result.falls_through,
            }));
        }
        if self.is_read_only_container_size(source, target)? {
            let value_result = self.check_expression(source, value, None, None, return_type)?;
            self.emit(
                codes::IMMUTABLE_CONTAINER_PLACE,
                "sequential container size is read-only",
                operator_span,
            )?;
            return Ok(Some(ExpressionCheck {
                ty: self.error_type(),
                falls_through: value_result.falls_through,
            }));
        }
        Ok(None)
    }

    pub(super) fn container_member_type(
        &mut self,
        receiver: UnitTypeId,
        name_span: Span,
    ) -> Result<Option<UnitTypeId>, CompilationUnitTypeError> {
        if self.container_parts(receiver).is_none() {
            return Ok(None);
        }
        let name = self
            .sources
            .slice(name_span)
            .map_err(TypeCheckingError::from)?;
        if name == "size" {
            return Ok(Some(self.builtin(BuiltinType::Int)));
        }
        if matches!(name, "get" | "set") {
            self.emit(
                codes::INVALID_CONTAINER_MEMBER,
                "sequential container indexing is available only through []",
                name_span,
            )?;
            return Ok(Some(self.error_type()));
        }
        Ok(None)
    }

    pub(super) fn is_inout_argument_syntax(
        &self,
        source: SourceUnitId,
        expression: ExpressionId,
    ) -> bool {
        let Ok(node) = self.file(source).ast().expressions().get(expression) else {
            return false;
        };
        match node.payload() {
            Expression::Index { .. } => true,
            Expression::Group { expression } => self.is_inout_argument_syntax(source, *expression),
            _ => self.is_syntactic_place(source, expression),
        }
    }

    pub(super) fn is_mutable_inout_place(
        &self,
        source: SourceUnitId,
        expression: ExpressionId,
    ) -> Result<bool, CompilationUnitTypeError> {
        if let Some(place) = self.element_place_for_expression(source, expression) {
            return Ok(place.is_mutable());
        }
        if self.is_read_only_container_size(source, expression)? {
            return Ok(false);
        }
        Ok(self.expression_category(source, expression)
            == crate::type_checking::ExpressionCategory::Place)
    }

    pub(super) fn is_read_only_container_size(
        &self,
        source: SourceUnitId,
        expression: ExpressionId,
    ) -> Result<bool, CompilationUnitTypeError> {
        let payload = self
            .file(source)
            .ast()
            .expressions()
            .get(expression)
            .map_err(TypeCheckingError::from)?
            .payload();
        if let Expression::Group { expression } = payload {
            return self.is_read_only_container_size(source, *expression);
        }
        let Expression::Member {
            receiver,
            name_span,
            ..
        } = payload
        else {
            return Ok(false);
        };
        let Some(receiver_type) = self
            .parts
            .expression_types
            .get(&UnitExpressionId::new(source, *receiver))
            .copied()
        else {
            return Ok(false);
        };
        Ok(self.container_parts(receiver_type).is_some()
            && self
                .sources
                .slice(*name_span)
                .map_err(TypeCheckingError::from)?
                == "size")
    }

    fn element_place_for_expression(
        &self,
        source: SourceUnitId,
        expression: ExpressionId,
    ) -> Option<UnitElementPlaceDescriptor> {
        let key = UnitExpressionId::new(source, expression);
        if let Some(place) = self
            .parts
            .element_places
            .iter()
            .copied()
            .find(|place| place.expression() == key)
        {
            return Some(place);
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
                self.element_place_for_expression(source, *expression)
            }
            _ => None,
        }
    }
}
