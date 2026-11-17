//! Admit an exact source-ordered construction prefix without a completed root.
use super::*;
use lang_frontend::{
    ownership_checking::ConstructionDeliveryKind, type_checking::ExpressionCategory,
};

impl Checker<'_, '_> {
    pub(super) fn terminated_construction(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self
            .inputs
            .typed
            .construction(id)
            .filter(|d| d.expression() == id)
            .cloned()
        else {
            return false;
        };
        let result = self.c(descriptor.result_type());
        let UnitConstructionTarget::Nominal(declaration) = descriptor.target() else {
            return false;
        };
        let (Some((nominal, type_arguments)), Some(fields)) =
            (self.class_signature(result), self.class_fields(result))
        else {
            return false;
        };
        let nominal_fields = nominal
            .fields()
            .iter()
            .map(|f| f.symbol())
            .collect::<Vec<_>>();
        let instance_arguments = descriptor
            .instance()
            .type_arguments()
            .iter()
            .map(|ty| self.c(*ty))
            .collect::<Vec<_>>();
        let mut plans = self
            .inputs
            .owned
            .construction_plans()
            .iter()
            .filter(|p| p.construction() == id);
        let (Some(plan), None) = (plans.next(), plans.next()) else {
            return false;
        };
        let Some(terminating) = plan.terminating_operand() else {
            return false;
        };
        if !matches!(self.kind(result), Some(UnitTypeKind::Nominal { declaration: own, .. }) if *own == declaration)
            || !self.class(result)
            || self.expression_type(expression) != Some(result)
            || instance_arguments != type_arguments
            || descriptor.arguments().len() != fields.len()
            || plan.target() != descriptor.target()
            || plan.root_obligation().is_some()
            || terminating.source_unit() != self.inputs.source_unit
            || self.inputs.typed.expression_falls_through(id) != Some(false)
            || self.inputs.typed.expression_falls_through(terminating) != Some(false)
            || !self
                .expression_type_of(terminating)
                .is_some_and(|ty| self.nothing(ty))
        {
            return false;
        }
        let deliveries = plan.deliveries().to_vec();
        let mut arguments = descriptor.arguments().to_vec();
        arguments.sort_by_key(|a| a.evaluation_index());
        let mut parameters = BTreeSet::new();
        if !arguments.iter().enumerate().all(|(index, argument)| {
            argument.evaluation_index() == index
                && argument.argument().source_unit() == self.inputs.source_unit
                && parameters.insert(argument.parameter_index())
                && fields.get(argument.parameter_index())
                    == Some(&self.c(argument.parameter_type()))
                && nominal_fields.get(argument.parameter_index()).copied()
                    == argument.parameter_symbol()
        }) {
            return false;
        }
        let Some(stop) = arguments.iter().position(|a| a.argument() == terminating) else {
            return false;
        };
        if deliveries.len() != stop {
            return false;
        }
        for (index, (argument, delivery)) in arguments[..stop].iter().zip(deliveries).enumerate() {
            let parameter = self.c(argument.parameter_type());
            let Some(argument_type) = self.expression_type_of(argument.argument()) else {
                return false;
            };
            let expected = match (argument.category(), self.copyability_of(argument_type)) {
                (ExpressionCategory::Temporary, _) => ConstructionDeliveryKind::DeliverTemporary,
                (ExpressionCategory::Place, Copyability::Copyable) => {
                    ConstructionDeliveryKind::Copy
                }
                (ExpressionCategory::Place, Copyability::MoveOnly) => {
                    ConstructionDeliveryKind::Move
                }
                _ => return false,
            };
            if delivery.construction() != id
                || delivery.argument() != argument.argument()
                || delivery.parameter_index() != argument.parameter_index()
                || delivery.parameter_symbol() != argument.parameter_symbol()
                || delivery.evaluation_index() != index
                || delivery.kind() != expected
                || !self.adaptable(argument.argument().expression(), parameter)
                || !self.expression(argument.argument().expression())
            {
                return false;
            }
        }
        if !self.expression(terminating.expression()) {
            return false;
        }
        self.migrated = true;
        true
    }
}
