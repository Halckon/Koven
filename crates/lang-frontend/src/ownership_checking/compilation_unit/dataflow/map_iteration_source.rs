//! Map source 的透明分组/真实字段路径；不把投影当成另一个 temporary owner。
use super::{Checker, ExpressionUse, Flows, OwnershipCheckingError, State};
use crate::{
    ast::ExpressionId,
    parser::Expression,
    type_checking::{
        UnitAggregateProjectionKind, UnitAggregateProjectionReceiver, UnitExpressionId,
    },
};
impl Checker<'_> {
    pub(super) fn check_map_iteration_source(
        &mut self,
        source: ExpressionId,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        if let Expression::Group { expression } =
            self.parsed.ast().expressions().get(source)?.payload()
        {
            return self.check_map_iteration_source(*expression, state);
        }
        if self.place(source)?.is_none()
            && let Some(projection) = self
                .typed
                .aggregate_projection(self.unit_expression(source))
            && projection.kind() == UnitAggregateProjectionKind::Field
            && let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver()
        {
            if receiver.source_unit() != self.source_unit {
                return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                    source_unit: self.source_unit.index(),
                    expression: source.index(),
                });
            }
            return self.check_map_iteration_source(receiver.expression(), state);
        }
        self.check_expression(
            source,
            state,
            ExpressionUse::Place {
                parameter_span: None,
            },
        )
    }

    pub(super) fn map_iteration_temporary_owner(
        &self,
        source: ExpressionId,
    ) -> Result<Option<UnitExpressionId>, OwnershipCheckingError> {
        if let Expression::Group { expression } =
            self.parsed.ast().expressions().get(source)?.payload()
        {
            return self.map_iteration_temporary_owner(*expression);
        }
        if let Some(projection) = self
            .typed
            .aggregate_projection(self.unit_expression(source))
            && projection.kind() == UnitAggregateProjectionKind::Field
            && let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver()
        {
            if receiver.source_unit() != self.source_unit {
                return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                    source_unit: self.source_unit.index(),
                    expression: source.index(),
                });
            }
            return self.map_iteration_temporary_owner(receiver.expression());
        }
        if let Some(element) = self.element_place_descriptor(source)? {
            if element.receiver().source_unit() != self.source_unit {
                return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                    source_unit: self.source_unit.index(),
                    expression: source.index(),
                });
            }
            return self.map_iteration_temporary_owner(element.receiver().expression());
        }
        self.temporary_expression_origin(source)
    }
}
