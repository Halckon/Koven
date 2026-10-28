//! Subset rules for Copyable `value class` (SPEC-0361): inline construction, field reads by
//! copy and value-class receivers. They mirror `build/value.rs`.
use lang_frontend::{
    ast::ExpressionId,
    parser::Expression,
    type_checking::{
        Copyability, NominalKind, UnitAggregateProjectionKind, UnitAggregateProjectionReceiver,
        UnitConstructionTarget, UnitNominalSignature, UnitTypeId, UnitTypeKind,
    },
};

use super::{Checker, receiver::Owner};
use crate::ssa::unit_plan::resolve_nominal_runtime_field_types;

impl Checker<'_, '_> {
    /// The expression inside any number of grouping parentheses.
    pub(super) fn ungrouped(&self, mut expression: ExpressionId) -> ExpressionId {
        while let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) {
            let Expression::Group { expression: inner } = node.payload() else {
                break;
            };
            expression = *inner;
        }
        expression
    }

    fn value_signature(&self, ty: UnitTypeId) -> Option<(&UnitNominalSignature, Vec<UnitTypeId>)> {
        let Some(UnitTypeKind::Nominal {
            declaration,
            arguments,
        }) = self.kind(ty)
        else {
            return None;
        };
        let nominal = self
            .inputs
            .typed
            .signatures()
            .declaration(*declaration)?
            .nominal()?;
        (nominal.kind() == NominalKind::ValueClass).then(|| (nominal, arguments.clone()))
    }

    /// Concrete field types of a value class, in declaration order.
    pub(super) fn value_fields(&self, ty: UnitTypeId) -> Option<Vec<UnitTypeId>> {
        let (nominal, arguments) = self.value_signature(ty)?;
        resolve_nominal_runtime_field_types(self.inputs.typed, ty, nominal, &arguments).ok()
    }

    /// A concrete Copyable value class whose fields are all scalars.
    pub(super) fn value_class(&self, ty: UnitTypeId) -> bool {
        self.inputs.typed.copyability(ty) == Copyability::Copyable
            && self
                .value_fields(ty)
                .is_some_and(|fields| fields.iter().all(|field| self.scalar(*field)))
    }

    /// `V(fields...)`: every field delivered in evaluation order; no root obligation.
    pub(super) fn value_construction(&mut self, expression: ExpressionId) -> bool {
        self.migrated = true;
        let id = self.id(expression);
        let Some(descriptor) = self
            .inputs
            .typed
            .construction(id)
            .filter(|descriptor| descriptor.expression() == id)
            .cloned()
        else {
            return false;
        };
        let result = self.c(descriptor.result_type());
        let UnitConstructionTarget::Nominal(_) = descriptor.target() else {
            return false;
        };
        let (Some((nominal, _)), Some(fields)) =
            (self.value_signature(result), self.value_fields(result))
        else {
            return false;
        };
        let symbols: Vec<_> = nominal
            .fields()
            .iter()
            .map(|field| field.symbol())
            .collect();
        let mut plans = self
            .inputs
            .owned
            .construction_plans()
            .iter()
            .filter(|plan| plan.construction() == id);
        let (Some(plan), None) = (plans.next(), plans.next()) else {
            return false;
        };
        let identity = self.value_class(result)
            && self.expression_type(expression) == Some(result)
            && descriptor.arguments().len() == fields.len()
            && plan.target() == descriptor.target()
            && plan.terminating_operand().is_none()
            && plan.root_obligation().is_none()
            && plan.deliveries().len() == fields.len();
        if !identity {
            return false;
        }
        let mut arguments = descriptor.arguments().iter().collect::<Vec<_>>();
        arguments.sort_by_key(|argument| argument.evaluation_index());
        arguments
            .into_iter()
            .zip(plan.deliveries().to_vec())
            .enumerate()
            .all(|(index, (argument, delivery))| {
                let value = argument.argument();
                let parameter = self.c(argument.parameter_type());
                argument.evaluation_index() == index
                    && value.source_unit() == self.inputs.source_unit
                    && fields.get(argument.parameter_index()) == Some(&parameter)
                    && symbols.get(argument.parameter_index()).copied()
                        == argument.parameter_symbol()
                    && delivery.construction() == id
                    && delivery.argument() == value
                    && delivery.parameter_index() == argument.parameter_index()
                    && delivery.evaluation_index() == index
                    && self.expression_type(value.expression()) == Some(parameter)
                    && self.expression(value.expression())
            })
    }

    /// The value-class owner of a projection, read by copy: a known name or the current
    /// receiver (explicit or implicit `this`).
    pub(super) fn value_projection(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let typed = self.inputs.typed;
        let Some(projection) = typed.aggregate_projection(id) else {
            return false;
        };
        let field_type = self.c(projection.ty());
        if projection.kind() != UnitAggregateProjectionKind::Field
            || self.expression_type_of(id) != Some(field_type)
            || !self.scalar(field_type)
        {
            return false;
        }
        let owner = match projection.receiver() {
            UnitAggregateProjectionReceiver::Expression(receiver) => {
                if receiver.source_unit() != self.inputs.source_unit
                    || !self.plain(receiver.expression())
                {
                    return false;
                }
                let inner = self.ungrouped(receiver.expression());
                let Ok(node) = self.inputs.parsed.ast().expressions().get(inner) else {
                    return false;
                };
                match node.payload() {
                    Expression::Name if !self.name(inner) => return false,
                    Expression::Name | Expression::This => {}
                    _ => return false,
                }
                Owner::Expression(inner)
            }
            UnitAggregateProjectionReceiver::This(declaration) => Owner::This(declaration),
        };
        let Some(owner_type) =
            self.owner_type(owner, lang_frontend::type_checking::ParameterMode::Value)
        else {
            return false;
        };
        self.migrated = true;
        self.value_class(owner_type)
            && self
                .value_signature(owner_type)
                .is_some_and(|(nominal, _)| {
                    nominal
                        .fields()
                        .iter()
                        .any(|field| field.symbol() == projection.field())
                })
    }
}
