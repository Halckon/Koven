//! Subset rules for class construction, field reads and `!!`; they mirror the builder.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{ConstructionRootKind, NonNullAssertionTransferKind},
    parser::Expression,
    type_checking::{
        Copyability, NullableWhenSubjectCategory, UnitAggregateProjectionKind,
        UnitAggregateProjectionReceiver, UnitConstructionTarget, UnitTypeKind,
    },
};

use super::Checker;

impl Checker<'_, '_> {
    /// A plain-class construction whose single plan delivers every field in order.
    pub(super) fn construction(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let typed = self.inputs.typed;
        let Some(descriptor) = typed
            .construction(id)
            .filter(|descriptor| descriptor.expression() == id)
            .cloned()
        else {
            return false;
        };
        let result = descriptor.result_type();
        let UnitConstructionTarget::Nominal(declaration) = descriptor.target() else {
            return false;
        };
        let (Some(nominal), Some(fields)) =
            (self.class_signature(result), self.class_fields(result))
        else {
            return false;
        };
        let nominal_fields: Vec<_> = nominal
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
        let identity = matches!(
            self.kind(result),
            Some(UnitTypeKind::Nominal { declaration: own, .. }) if *own == declaration
        ) && self.class(result)
            && self.expression_type(expression) == Some(result)
            && descriptor.arguments().len() == fields.len()
            && plan.target() == descriptor.target()
            && plan.terminating_operand().is_none()
            && plan.deliveries().len() == fields.len()
            && plan.root_obligation().is_some_and(|root| {
                root.construction() == id
                    && root.result_type() == result
                    && root.kind() == ConstructionRootKind::HeapOwner
            });
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
                let parameter = argument.parameter_type();
                argument.evaluation_index() == index
                    && value.source_unit() == self.inputs.source_unit
                    && fields.get(argument.parameter_index()) == Some(&parameter)
                    && nominal_fields.get(argument.parameter_index()).copied()
                        == argument.parameter_symbol()
                    && delivery.construction() == id
                    && delivery.argument() == value
                    && delivery.parameter_index() == argument.parameter_index()
                    && delivery.evaluation_index() == index
                    && self.adaptable(value.expression(), parameter)
                    && self.expression(value.expression())
            })
    }

    /// The value's type is the parameter type or its frontend-approved nullable wrap.
    fn adaptable(
        &self,
        value: ExpressionId,
        expected: lang_frontend::type_checking::UnitTypeId,
    ) -> bool {
        self.expression_type(value).is_some_and(|actual| {
            actual == expected
                || matches!(self.kind(expected), Some(UnitTypeKind::Nullable(inner)) if *inner == actual)
        })
    }

    /// A Copyable scalar field read from a named class owner or its proof view.
    pub(super) fn projection(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let typed = self.inputs.typed;
        let Some(projection) = typed.aggregate_projection(id) else {
            return false;
        };
        let UnitAggregateProjectionReceiver::Expression(receiver) = projection.receiver() else {
            return false;
        };
        let receiver_expression = receiver.expression();
        if projection.kind() != UnitAggregateProjectionKind::Field
            || receiver.source_unit() != self.inputs.source_unit
            || typed.expression_type(id) != Some(projection.ty())
            || !self.scalar(projection.ty())
            || typed.copyability(projection.ty()) != Copyability::Copyable
            || !self.plain(receiver_expression)
        {
            return false;
        }
        let Ok(node) = self
            .inputs
            .parsed
            .ast()
            .expressions()
            .get(receiver_expression)
        else {
            return false;
        };
        let Expression::Name = node.payload() else {
            return false;
        };
        let Some(symbol) = self.reference(receiver_expression) else {
            return false;
        };
        let owner = match typed.non_null_use(receiver) {
            // A smart-cast receiver is read through the condition's proof view.
            Some(fact) => (fact.symbol() == symbol).then(|| fact.narrowed_type()),
            None => self.expression_type(receiver_expression),
        };
        self.known.contains(&symbol)
            && owner.is_some_and(|owner| {
                self.class(owner)
                    && self.class_signature(owner).is_some_and(|nominal| {
                        nominal
                            .fields()
                            .iter()
                            .any(|field| field.symbol() == projection.field())
                    })
            })
    }

    /// `operand!!` consuming a MoveOnly nullable owner: a named root or a temporary.
    pub(super) fn non_null_assertion(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.non_null_assertion(id) else {
            return false;
        };
        let Some(plan) = self.inputs.owned.non_null_assertion(id) else {
            return false;
        };
        let operand = descriptor.operand();
        let inner = descriptor.inner_type();
        self.note_nullable(descriptor.nullable_type());
        let shape = plan.descriptor() == &descriptor
            && operand.source_unit() == self.inputs.source_unit
            && plan.non_null_transfer() == NonNullAssertionTransferKind::Consume
            && matches!(
                self.kind(descriptor.nullable_type()),
                Some(UnitTypeKind::Nullable(nullable_inner)) if *nullable_inner == inner
            )
            && self.view_inner(inner)
            && self.expression_type(expression) == Some(inner);
        if !shape {
            return false;
        }
        let operand = operand.expression();
        let Ok(node) = self.inputs.parsed.ast().expressions().get(operand) else {
            return false;
        };
        let source = match node.payload() {
            Expression::Name => {
                descriptor.source_category() == NullableWhenSubjectCategory::OwnedRoot
                    && self.inputs.typed.non_null_use(self.id(operand)).is_none()
            }
            Expression::Call { .. } => {
                descriptor.source_category() == NullableWhenSubjectCategory::Temporary
            }
            _ => false,
        };
        source && self.expression(operand)
    }
}
