//! SPEC-0197 compilation-unit expression 类型与 value-category facts。

use crate::{
    ast::ExpressionId,
    name_resolution::{Namespace, SourceUnitId, UnitReferenceTarget},
    parser::Expression,
    type_checking::{ExpressionCategory, UnitAggregateProjectionKind, UnitTypeId},
};

use super::{BodyChecker, UnitExpressionId};

impl BodyChecker<'_> {
    pub(super) fn expression_category(
        &self,
        source: SourceUnitId,
        expression: ExpressionId,
    ) -> ExpressionCategory {
        if self
            .parts
            .constant_selections
            .contains_key(&UnitExpressionId::new(source, expression))
        {
            return ExpressionCategory::Temporary;
        }
        if let Ok(node) = self.file(source).ast().expressions().get(expression)
            && let Expression::Group { expression } = node.payload()
        {
            return self
                .parts
                .expression_categories
                .get(&UnitExpressionId::new(source, *expression))
                .copied()
                .unwrap_or(ExpressionCategory::Temporary);
        }
        let expression = UnitExpressionId::new(source, expression);
        if self.parts.aggregate_projections.iter().any(|projection| {
            projection.expression() == expression
                && projection.kind() == UnitAggregateProjectionKind::Field
        }) {
            return ExpressionCategory::Place;
        }
        if self
            .parts
            .element_places
            .iter()
            .any(|place| place.expression() == expression)
        {
            return ExpressionCategory::Place;
        }
        if self
            .is_read_only_container_size(source, expression.expression())
            .unwrap_or(false)
        {
            return ExpressionCategory::Temporary;
        }
        if self.is_syntactic_place(source, expression.expression()) {
            ExpressionCategory::Place
        } else {
            ExpressionCategory::Temporary
        }
    }

    pub(super) fn record_expression(
        &mut self,
        source: SourceUnitId,
        expression: ExpressionId,
        ty: UnitTypeId,
    ) {
        let key = UnitExpressionId::new(source, expression);
        self.parts.expression_types.insert(key, ty);
        self.parts
            .expression_categories
            .insert(key, self.expression_category(source, expression));
    }

    pub(super) fn is_syntactic_place(
        &self,
        source: SourceUnitId,
        expression: ExpressionId,
    ) -> bool {
        let Ok(node) = self.file(source).ast().expressions().get(expression) else {
            return false;
        };
        match node.payload() {
            Expression::This => self.current_receiver.is_some(),
            Expression::Member { name_span, .. } => !matches!(
                self.reference(source, *name_span, Namespace::Value),
                Some(UnitReferenceTarget::Declaration(_))
            ),
            Expression::Name => matches!(
                self.reference(source, node.span(), Namespace::Value),
                Some(UnitReferenceTarget::Symbol(_))
            ),
            Expression::Group { expression } => self.is_syntactic_place(source, *expression),
            _ => false,
        }
    }
}
