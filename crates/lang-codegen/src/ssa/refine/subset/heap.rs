//! Subset rules for class construction, field reads and `!!`; they mirror the builder.
use std::collections::BTreeSet;

use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{ConstructionRootKind, NonNullAssertionTransferKind},
    parser::Expression,
    type_checking::{
        Copyability, IntrinsicTypeConstructor, NullableWhenSubjectCategory, ParameterMode,
        UnitAggregateProjectionKind, UnitAggregateProjectionReceiver, UnitConstructionTarget,
        UnitTypeId, UnitTypeKind,
    },
};

use super::{Checker, receiver::Owner};

impl Checker<'_, '_> {
    pub(super) fn view_inner_seen(
        &self,
        ty: UnitTypeId,
        active: &mut BTreeSet<UnitTypeId>,
    ) -> bool {
        self.string(ty)
            || self.class_seen(ty, active)
            || self.container_element_seen(ty, active).is_some()
            || self.box_payload_seen(ty, active).is_some()
            || self.rc_payload_seen(ty, active).is_some()
    }

    pub(super) fn class_seen(&self, ty: UnitTypeId, active: &mut BTreeSet<UnitTypeId>) -> bool {
        let Some(fields) = self.class_fields(ty) else {
            return false;
        };
        if !active.insert(ty) {
            return false;
        }
        let ok = fields
            .iter()
            .all(|field| self.storage_type_seen(*field, active));
        active.remove(&ty);
        ok
    }

    pub(super) fn class_has_non_scalar_field(&self, ty: UnitTypeId) -> bool {
        self.class(ty)
            && self.class_fields(ty).is_some_and(|fields| {
                fields
                    .iter()
                    .any(|field| !(self.scalar(*field) || self.string(*field)))
            })
    }

    pub(super) fn push_demand(
        &self,
        ty: UnitTypeId,
        seen: &mut BTreeSet<UnitTypeId>,
        ordered: &mut Vec<UnitTypeId>,
    ) {
        if !(self.unit(ty) || self.storage_type(ty)) || !seen.insert(ty) {
            return;
        }
        if let Some(element) = self.container_element(ty) {
            self.push_demand(element, seen, ordered);
        }
        if let Some((key, value)) = self.map_kv(ty) {
            self.push_demand(key, seen, ordered);
            self.push_demand(value, seen, ordered);
        }
        if let Some(payload) = self.box_payload(ty) {
            self.push_demand(payload, seen, ordered);
        }
        if let Some(payload) = self.rc_payload(ty) {
            self.push_demand(payload, seen, ordered);
        }
        if self.class(ty)
            && let Some(fields) = self.class_fields(ty)
        {
            for field in fields {
                self.push_demand(field, seen, ordered);
            }
        }
        if self.box_enum_type(ty)
            && let Some(cases) = self.enum_payload_types(ty)
        {
            for payload in cases.into_iter().flatten() {
                self.push_demand(payload, seen, ordered);
            }
        }
        if let Some(UnitTypeKind::Nullable(inner)) = self.kind(ty) {
            self.push_demand(*inner, seen, ordered);
        }
        ordered.push(ty);
    }

    pub(super) fn storage_type_seen(
        &self,
        ty: UnitTypeId,
        active: &mut BTreeSet<UnitTypeId>,
    ) -> bool {
        match self.kind(ty) {
            Some(UnitTypeKind::Builtin(lang_frontend::type_checking::BuiltinType::String)) => true,
            Some(UnitTypeKind::Nullable(inner)) => {
                self.scalar(*inner) || self.view_inner_seen(*inner, active)
            }
            _ => {
                self.scalar(ty)
                    || self.range_type(ty)
                    || self.class_seen(ty, active)
                    || self.enum_type(ty)
                    || self.box_enum_type_seen(ty, active)
                    || self.box_payload_seen(ty, active).is_some()
                    || self.rc_payload_seen(ty, active).is_some()
                    || self.value_class(ty)
                    || self.move_value_class(ty)
                    || self.container_element_seen(ty, active).is_some()
                    || self.map_kv(ty).is_some()
            }
        }
    }

    /// A concrete `Box<T>` whose payload is a supported `value class` or `enum class`.
    pub(super) fn box_payload(&self, ty: UnitTypeId) -> Option<UnitTypeId> {
        self.box_payload_seen(ty, &mut BTreeSet::new())
    }

    pub(super) fn box_payload_seen(
        &self,
        ty: UnitTypeId,
        active: &mut BTreeSet<UnitTypeId>,
    ) -> Option<UnitTypeId> {
        let Some(UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Box,
            arguments,
        }) = self.kind(ty)
        else {
            return None;
        };
        let [payload] = arguments.as_slice() else {
            return None;
        };
        let valid_payload = self.value_class(*payload)
            || self.move_value_class(*payload)
            || self.enum_type(*payload)
            || self.box_enum_type_seen(*payload, active)
            || active.contains(payload);
        (valid_payload && self.storage_type_seen(*payload, active)).then_some(*payload)
    }

    /// `Box(payload)` constructing a single-owner heap box.
    pub(super) fn box_construction(&mut self, expression: ExpressionId) -> bool {
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
        let Some(payload) = self.box_payload(result) else {
            return false;
        };
        let [type_argument] = descriptor.instance().type_arguments() else {
            return false;
        };
        let [argument] = descriptor.arguments() else {
            return false;
        };
        let mut plans = self
            .inputs
            .owned
            .construction_plans()
            .iter()
            .filter(|plan| plan.construction() == id);
        let (Some(plan), None) = (plans.next(), plans.next()) else {
            return false;
        };
        let [delivery] = plan.deliveries() else {
            return false;
        };
        let value = argument.argument();
        let ok = descriptor.target() == UnitConstructionTarget::IntrinsicBox
            && self.expression_type(expression) == Some(result)
            && self.c(*type_argument) == payload
            && self.c(argument.parameter_type()) == payload
            && argument.parameter_symbol().is_none()
            && argument.parameter_index() == 0
            && argument.evaluation_index() == 0
            && value.source_unit() == self.inputs.source_unit
            && plan.target() == UnitConstructionTarget::IntrinsicBox
            && plan.terminating_operand().is_none()
            && plan.root_obligation().is_some_and(|root| {
                root.construction() == id
                    && self.c(root.result_type()) == result
                    && root.kind() == ConstructionRootKind::HeapOwner
            })
            && delivery.construction() == id
            && delivery.argument() == value
            && delivery.parameter_index() == 0
            && delivery.evaluation_index() == 0
            && self.adaptable(value.expression(), payload)
            && self.expression(value.expression());
        self.migrated |= ok;
        ok
    }

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
        let result = self.c(descriptor.result_type());
        let UnitConstructionTarget::Nominal(declaration) = descriptor.target() else {
            return false;
        };
        let (Some((nominal, type_arguments)), Some(fields)) =
            (self.class_signature(result), self.class_fields(result))
        else {
            return false;
        };
        let instance_arguments: Vec<_> = descriptor
            .instance()
            .type_arguments()
            .iter()
            .map(|ty| self.c(*ty))
            .collect();
        let nominal_fields: Vec<_> = nominal
            .fields()
            .iter()
            .map(|field| field.symbol())
            .collect();
        if !type_arguments.is_empty() || self.class_has_non_scalar_field(result) {
            self.migrated = true;
        }
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
            && instance_arguments == type_arguments
            && descriptor.arguments().len() == fields.len()
            && plan.target() == descriptor.target()
            && plan.terminating_operand().is_none()
            && plan.deliveries().len() == fields.len()
            && plan.root_obligation().is_some_and(|root| {
                root.construction() == id
                    && self.c(root.result_type()) == result
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
                let parameter = self.c(argument.parameter_type());
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
    pub(super) fn adaptable(&self, value: ExpressionId, expected: UnitTypeId) -> bool {
        self.expression_type(value).is_some_and(|actual| {
            actual == expected
                || matches!(self.kind(expected), Some(UnitTypeKind::Nullable(inner)) if *inner == actual)
        })
    }

    /// A Copyable field read from a class owner: a narrowed name through its proof
    /// view, a known local, a Borrow/Inout parameter or the current receiver.
    pub(super) fn projection(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let typed = self.inputs.typed;
        let Some(projection) = typed.aggregate_projection(id) else {
            return false;
        };
        if projection.kind() != UnitAggregateProjectionKind::Field
            || self.expression_type_of(id) != Some(self.c(projection.ty()))
            || !self.storage_type(self.c(projection.ty()))
            || self.copyability_of(self.c(projection.ty())) != Copyability::Copyable
        {
            return false;
        }
        let owner = match projection.receiver() {
            UnitAggregateProjectionReceiver::Expression(receiver)
                if typed.rc_operation(receiver).is_some() =>
            {
                // `rc.value.f`: the payload is read through its place (SPEC-0359).
                if receiver.source_unit() != self.inputs.source_unit {
                    return false;
                }
                self.rc_value_owner(receiver.expression())
            }
            UnitAggregateProjectionReceiver::Expression(receiver) => {
                let receiver_expression = receiver.expression();
                if receiver.source_unit() != self.inputs.source_unit
                    || !self.plain(receiver_expression)
                {
                    return false;
                }
                match typed.non_null_use(receiver) {
                    // A smart-cast receiver is read through the condition's proof view.
                    Some(fact) => {
                        let named = self
                            .inputs
                            .parsed
                            .ast()
                            .expressions()
                            .get(receiver_expression)
                            .is_ok_and(|node| matches!(node.payload(), Expression::Name));
                        (named
                            && self.reference(receiver_expression) == Some(fact.symbol())
                            && self.known.contains(&fact.symbol())
                            && self.view_symbols.contains(&fact.symbol()))
                        .then(|| self.c(fact.narrowed_type()))
                    }
                    None => self.owner_type(
                        Owner::Expression(receiver_expression),
                        ParameterMode::Borrow,
                    ),
                }
            }
            UnitAggregateProjectionReceiver::This(declaration) => {
                self.owner_type(Owner::This(declaration), ParameterMode::Borrow)
            }
        };
        let Some(owner) = owner.filter(|owner| self.class_field(*owner, projection.field())) else {
            return false;
        };
        if self
            .class_signature(owner)
            .is_some_and(|(_, arguments)| !arguments.is_empty())
            || self.class_has_non_scalar_field(owner)
            || !self.scalar(self.c(projection.ty()))
        {
            self.migrated = true;
        }
        true
    }

    /// `field` is a field of the plain class `owner`.
    pub(super) fn class_field(
        &self,
        owner: lang_frontend::type_checking::UnitTypeId,
        field: lang_frontend::name_resolution::UnitSymbolId,
    ) -> bool {
        self.class(owner)
            && self.class_signature(owner).is_some_and(|(nominal, _)| {
                nominal
                    .fields()
                    .iter()
                    .any(|candidate| candidate.symbol() == field)
            })
    }

    /// `operand!!` consuming a MoveOnly nullable owner: a (grouped) named root or a temporary
    /// call or `if` result.
    pub(super) fn non_null_assertion(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(descriptor) = self.inputs.typed.non_null_assertion(id) else {
            return false;
        };
        let Some(plan) = self.inputs.owned.non_null_assertion(id) else {
            return false;
        };
        let operand = descriptor.operand();
        let inner = self.c(descriptor.inner_type());
        self.note_nullable(self.c(descriptor.nullable_type()));
        let shape = plan.descriptor() == &descriptor
            && operand.source_unit() == self.inputs.source_unit
            && plan.non_null_transfer() == NonNullAssertionTransferKind::Consume
            && matches!(
                self.kind(self.c(descriptor.nullable_type())),
                Some(UnitTypeKind::Nullable(nullable_inner)) if *nullable_inner == inner
            )
            && self.view_inner(inner)
            && self.expression_type(expression) == Some(inner);
        if !shape {
            return false;
        }
        let operand = operand.expression();
        let root = self.ungrouped(operand);
        let Ok(node) = self.inputs.parsed.ast().expressions().get(root) else {
            return false;
        };
        let source = match node.payload() {
            Expression::Name => {
                descriptor.source_category() == NullableWhenSubjectCategory::OwnedRoot
                    && self.inputs.typed.non_null_use(self.id(root)).is_none()
            }
            // Temporaries, including branch results whose other branches leave the
            // enclosing call, loop or function before the assertion.
            Expression::Call { .. } | Expression::If { .. } => {
                descriptor.source_category() == NullableWhenSubjectCategory::Temporary
            }
            _ => false,
        };
        source && self.expression(operand)
    }
}
