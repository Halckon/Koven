//! Completed owned temporary parents of copied fields have one final read boundary.
use super::*;
use crate::type_checking::{
    CallableResultSource, NominalKind, UnitAggregateProjectionKind, UnitCallTarget,
    UnitConstructionTarget,
};

impl DropPlanner<'_, '_> {
    pub(super) fn drop_copied_field_owner(
        &mut self,
        field: ExpressionId,
        receiver: ExpressionId,
    ) -> Result<(), OwnershipCheckingError> {
        let Some(projection) = self
            .checker
            .typed
            .aggregate_projection(self.checker.unit_expression(field))
        else {
            return Ok(());
        };
        if projection.kind() != UnitAggregateProjectionKind::Field
            || self.checker.typed.copyability(projection.ty()) != Copyability::Copyable
        {
            return Ok(());
        }
        let mut constructor = receiver;
        while let Expression::Group { expression } = self
            .checker
            .parsed
            .ast()
            .expressions()
            .get(constructor)?
            .payload()
        {
            constructor = *expression;
        }
        let declaration = if let Some(descriptor) = self
            .checker
            .construction_descriptors
            .get(&self.checker.unit_expression(constructor))
        {
            let UnitConstructionTarget::Nominal(declaration) = descriptor.target() else {
                return Ok(());
            };
            declaration
        } else {
            let unit = self.checker.unit_expression(constructor);
            let Some(call) = self.checker.typed.call(unit) else {
                return Ok(());
            };
            if !matches!(call.target(), UnitCallTarget::Declaration(_))
                || call.receiver().is_some()
                || call.result_source() != CallableResultSource::Owned
            {
                return Ok(());
            }
            let Some(UnitTypeKind::Nominal { declaration, .. }) = self
                .checker
                .typed
                .expression_type(unit)
                .and_then(|ty| self.checker.typed.types().get(ty))
            else {
                return Ok(());
            };
            *declaration
        };
        if !self.is_move_only_temporary(constructor)
            || !self
                .checker
                .typed
                .signatures()
                .declaration(declaration)
                .and_then(|signature| signature.nominal())
                .is_some_and(|nominal| nominal.kind() == NominalKind::Class)
        {
            return Ok(());
        }
        self.push_fact(PlannerDropFact::new(
            PlannerDropPoint::AfterExpression(receiver),
            PlannerDropTarget::Temporary(constructor),
            self.checker
                .parsed
                .ast()
                .expressions()
                .get(constructor)?
                .span(),
        ));
        Ok(())
    }
}
