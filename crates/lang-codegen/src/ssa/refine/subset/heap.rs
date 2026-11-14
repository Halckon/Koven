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
            || self.move_value_class_seen(ty, active)
            || self.container_element_seen(ty, active).is_some()
            || self.map_kv_seen(ty, active).is_some()
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

    pub(super) fn class_has_deinit(&self, ty: UnitTypeId) -> bool {
        self.class_signature(ty)
            .is_some_and(|(nominal, _)| nominal.deinit().is_some())
    }

    pub(super) fn push_demand(
        &self,
        ty: UnitTypeId,
        seen: &mut BTreeSet<UnitTypeId>,
        ordered: &mut Vec<UnitTypeId>,
    ) {
        if matches!(self.kind(ty), Some(UnitTypeKind::Nullable(inner)) if self.nothing(*inner)) {
            return;
        }
        if !(self.unit(ty) || self.storage_type(ty)) || !seen.insert(ty) {
            return;
        }
        if let Some(element) = self.container_element(ty) {
            self.push_demand(element, seen, ordered);
        }
        if let Some(element) = self.range_view_element(ty) {
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
        if (self.value_class(ty) || self.move_value_class(ty))
            && let Some(fields) = self.value_fields(ty)
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
            Some(UnitTypeKind::Builtin(
                lang_frontend::type_checking::BuiltinType::String
                | lang_frontend::type_checking::BuiltinType::Char,
            )) => true,
            Some(UnitTypeKind::Nullable(inner)) => {
                self.nothing(*inner)
                    || self.scalar(*inner)
                    || self.range_type(*inner)
                    || self.value_class_seen(*inner, active)
                    || self.view_inner_seen(*inner, active)
            }
            _ => {
                self.scalar(ty)
                    || self.range_type(ty)
                    || self.class_seen(ty, active)
                    || self.enum_type(ty)
                    || self.box_enum_type_seen(ty, active)
                    || self.box_payload_seen(ty, active).is_some()
                    || self.rc_payload_seen(ty, active).is_some()
                    || self.value_class_seen(ty, active)
                    || self.move_value_class_seen(ty, active)
                    || self.object_type(ty)
                    || self.container_element_seen(ty, active).is_some()
                    || self.range_view_element_seen(ty, active).is_some()
                    || self.map_kv_seen(ty, active).is_some()
            }
        }
    }

    /// `T?` whose payload is a supported storage type.
    pub(super) fn nullable_storage(&self, ty: UnitTypeId) -> bool {
        matches!(self.kind(ty), Some(UnitTypeKind::Nullable(inner)) if self.storage_type(*inner))
    }

    /// Presence through a MoveOnly Borrow does not mint an owned payload view.
    pub(super) fn borrowed_nullable_presence(
        &self,
        symbol: lang_frontend::name_resolution::UnitSymbolId,
        nullable: UnitTypeId,
    ) -> Option<bool> {
        (self.parameter_mode(symbol) == Some(ParameterMode::Borrow)
            && self.copyability_of(nullable) == Copyability::MoveOnly)
            .then(|| {
                matches!(
                    self.kind(nullable),
                    Some(UnitTypeKind::Nullable(inner))
                        if self.map_kv(*inner).is_none() && self.view_inner(*inner)
                )
            })
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
        if !active.insert(ty) {
            return Some(*payload);
        }
        let valid_payload = self.value_class_seen(*payload, active)
            || self.move_value_class_seen(*payload, active)
            || self.enum_type(*payload)
            || self.box_enum_type_seen(*payload, active)
            || active.contains(payload);
        let ok = valid_payload && self.storage_type_seen(*payload, active);
        active.remove(&ty);
        ok.then_some(*payload)
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
        if !type_arguments.is_empty()
            || self.class_has_non_scalar_field(result)
            || self.class_has_deinit(result)
        {
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
                if receiver.source_unit() != self.inputs.source_unit {
                    return false;
                }
                let receiver_expression = receiver.expression();
                if typed
                    .aggregate_projection(self.id(self.ungrouped(receiver_expression)))
                    .is_some()
                {
                    let Some(owner) = self.field_loan_receiver(receiver_expression, true) else {
                        return false;
                    };
                    if !self.class_field(owner, projection.field()) {
                        return false;
                    }
                    self.migrated = true;
                    return true;
                }
                if !self.plain(receiver_expression) {
                    return false;
                }
                // Transparent groups around the narrowed name keep the same proof.
                let root = self.ungrouped(receiver_expression);
                let use_fact = typed
                    .non_null_use(receiver)
                    .or_else(|| typed.non_null_use(self.id(root)));
                match use_fact {
                    // A smart-cast receiver is read through the condition's proof view.
                    Some(fact) => {
                        let named = self
                            .inputs
                            .parsed
                            .ast()
                            .expressions()
                            .get(root)
                            .is_ok_and(|node| matches!(node.payload(), Expression::Name));
                        (named
                            && self.reference(root) == Some(fact.symbol())
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

    /// `operand!!` consuming a MoveOnly nullable owner or unwrapping a Copyable nullable:
    /// a (grouped) named root or a temporary call, index, or `if` result.
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
        let transfer_ok = (plan.non_null_transfer() == NonNullAssertionTransferKind::Consume
            && self.view_inner(inner))
            || (plan.non_null_transfer() == NonNullAssertionTransferKind::Copy
                && self.copyability_of(inner) == Copyability::Copyable
                && self.storage_type(inner));
        let shape = plan.descriptor() == &descriptor
            && operand.source_unit() == self.inputs.source_unit
            && transfer_ok
            && matches!(
                self.kind(self.c(descriptor.nullable_type())),
                Some(UnitTypeKind::Nullable(nullable_inner)) if *nullable_inner == inner
            )
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
                (plan.non_null_transfer() == NonNullAssertionTransferKind::Copy
                    || descriptor.source_category() == NullableWhenSubjectCategory::OwnedRoot)
                    && self.inputs.typed.non_null_use(self.id(root)).is_none()
            }
            // Temporaries, including branch results whose other branches leave the
            // enclosing call, loop or function before the assertion.
            Expression::Call { .. }
            | Expression::If { .. }
            | Expression::When { .. }
            | Expression::Index { .. } => {
                descriptor.source_category() == NullableWhenSubjectCategory::Temporary
            }
            _ => false,
        };
        source && self.expression(operand)
    }

    /// The mode of a parameter of this function (or a thunk capture); `None` for locals.
    pub(super) fn parameter_mode(
        &self,
        symbol: lang_frontend::name_resolution::UnitSymbolId,
    ) -> Option<ParameterMode> {
        if self.for_borrowed.contains(&symbol) {
            return Some(ParameterMode::Borrow);
        }
        if let crate::ssa::refine::RefineBody::Lambda(thunk) = &self.inputs.body
            && thunk
                .captures
                .iter()
                .any(|capture| capture.symbol == symbol)
        {
            return Some(ParameterMode::Borrow);
        }
        let index = self
            .inputs
            .parameters
            .iter()
            .position(|parameter| *parameter == symbol)?;
        self.inputs.modes.get(index).copied()
    }

    /// `&x` for an `Inout` parameter: a known scalar or plain-class local, or an `Inout`
    /// parameter passed on, whose exclusive loan the ownership facts publish.
    pub(super) fn inout_argument(&mut self, call: ExpressionId, value: ExpressionId) -> bool {
        self.migrated = true;
        let Some(symbol) = self.direct_place_symbol(value) else {
            return false;
        };
        let (call, value_id) = (self.id(call), self.id(value));
        let typed = self.inputs.typed;
        self.known.contains(&symbol)
            && matches!(
                self.parameter_mode(symbol),
                None | Some(ParameterMode::Inout)
            )
            && typed.non_null_use(value_id).is_none()
            && self.symbol_type(symbol).is_some_and(|ty| {
                ((self.storage_type(ty)
                    && self.copyability_of(ty)
                        == lang_frontend::type_checking::Copyability::Copyable)
                    || self.class(ty))
                    && self.expression_type_of(value_id) == Some(ty)
            })
            && self.inputs.owned.loans().iter().any(|loan| {
                loan.call() == call
                    && loan.argument() == value_id
                    && loan.kind() == lang_frontend::ownership_checking::LoanKind::Exclusive
            })
    }

    pub(super) fn direct_place_symbol(
        &self,
        mut expression: ExpressionId,
    ) -> Option<lang_frontend::name_resolution::UnitSymbolId> {
        loop {
            let node = self
                .inputs
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .ok()?;
            match node.payload() {
                Expression::Name => return self.reference(expression),
                Expression::Group { expression: inner } => expression = *inner,
                _ => return None,
            }
        }
    }

    /// Root `replace(&local, value)`, direct field `replace(&holder.field, value)`, and
    /// `swap(&left, &right)` on local bindings.
    pub(super) fn ownership_primitive(&mut self, expression: ExpressionId) -> bool {
        let id = self.id(expression);
        let Some(primitive) = self.inputs.typed.ownership_primitive(id) else {
            return false;
        };
        let Some(call) = self.inputs.typed.call(id) else {
            return false;
        };
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return false;
        };
        let Expression::Call { arguments, .. } = node.payload() else {
            return false;
        };
        if primitive.expression() != id
            || arguments.len() != 2
            || call.arguments().len() != 2
            || call.receiver().is_some()
        {
            return false;
        }
        let call_span = node.span();
        let value_ty = self.c(primitive.value_type());
        self.note_nullable(value_ty);
        if !(self.unit(value_ty) || self.storage_type(value_ty)) {
            return false;
        }
        let modes = match primitive.kind() {
            lang_frontend::type_checking::OwnershipPrimitiveKind::Replace => {
                [ParameterMode::Inout, ParameterMode::Value]
            }
            lang_frontend::type_checking::OwnershipPrimitiveKind::Swap => {
                [ParameterMode::Inout, ParameterMode::Inout]
            }
        };
        for (index, argument) in arguments.iter().enumerate() {
            let Some(mapping) = call
                .arguments()
                .iter()
                .find(|mapping| mapping.argument_index() == index)
            else {
                return false;
            };
            let arg_id = self.id(argument.value);
            if primitive.operands()[index] != arg_id
                || mapping.parameter_index() != index
                || mapping.mode() != modes[index]
                || mapping.parameter_type() != primitive.value_type()
            {
                return false;
            }
        }
        let projected = self.ungrouped(arguments[0].value);
        let projection = self.inputs.typed.aggregate_projection(self.id(projected));
        if primitive.kind() == lang_frontend::type_checking::OwnershipPrimitiveKind::Replace
            && let Some(projection) = projection
            && projection.kind() == lang_frontend::type_checking::UnitAggregateProjectionKind::Field
        {
            if matches!(
                self.kind(value_ty),
                Some(lang_frontend::type_checking::UnitTypeKind::Nullable(inner))
                    if self.rc_payload(*inner).is_some()
            ) {
                return false;
            }
            let lang_frontend::type_checking::UnitAggregateProjectionReceiver::Expression(receiver) =
                projection.receiver()
            else {
                return false;
            };
            let Some(symbol) = self
                .direct_place_symbol(receiver.expression())
                .filter(|_| receiver.source_unit() == self.inputs.source_unit)
            else {
                return false;
            };
            let Some(owner_ty) = self.symbol_type(symbol) else {
                return false;
            };
            if self.c(projection.ty()) != value_ty
                || !self.known.contains(&symbol)
                || self.callables.contains(&symbol)
                || self.parameter_mode(symbol).is_some()
                || !self.class(owner_ty)
                || !self
                    .class_signature(owner_ty)
                    .is_some_and(|(nominal, args)| {
                        args.is_empty() && nominal.type_parameters().is_empty()
                    })
                || !self.class_field(owner_ty, projection.field())
            {
                return false;
            }
            let arg0_id = self.id(arguments[0].value);
            let mut loans = self
                .inputs
                .owned
                .loans()
                .iter()
                .filter(|loan| loan.call() == id && loan.argument() == arg0_id);
            let Some(fact) = loans.next() else {
                return false;
            };
            if loans.next().is_some()
                || fact.kind() != lang_frontend::ownership_checking::LoanKind::Exclusive
                || fact.end_span() != call_span
                || !matches!(
                    fact.target(),
                    lang_frontend::ownership_checking::UnitLoanTarget::Place(place)
                        if place.root() == symbol
                            && place.fields() == [projection.field()]
                            && place.element().is_none()
                )
            {
                return false;
            }
            if let Some(field_plan) = self.inputs.owned.field_replacement(id) {
                if field_plan.descriptor() != &primitive
                    || field_plan.owner_type() != owner_ty
                    || field_plan.place().root() != symbol
                    || field_plan.place().fields() != [projection.field()]
                    || field_plan.place().element().is_some()
                {
                    return false;
                }
                let Some(delivery) = self
                    .inputs
                    .owned
                    .value_deliveries()
                    .iter()
                    .find(|delivery| {
                        delivery.call() == id && delivery.argument() == primitive.operands()[1]
                    })
                else {
                    return false;
                };
                let expected = match delivery.kind() {
                    lang_frontend::ownership_checking::UnitValueDeliveryKind::Copy => {
                        lang_frontend::ownership_checking::OwnershipPrimitiveValueTransfer::Copy
                    }
                    lang_frontend::ownership_checking::UnitValueDeliveryKind::Move => {
                        lang_frontend::ownership_checking::OwnershipPrimitiveValueTransfer::Move
                    }
                    lang_frontend::ownership_checking::UnitValueDeliveryKind::Temporary => {
                        lang_frontend::ownership_checking::OwnershipPrimitiveValueTransfer::Temporary
                    }
                };
                if field_plan.new_value_transfer() != expected {
                    return false;
                }
            } else if self.inputs.owned.value_deliveries().iter().any(|delivery| {
                delivery.call() == id && delivery.argument() == primitive.operands()[1]
            }) {
                return false;
            }
            if self.field_replace_rhs_conflicts(arguments[1].value, symbol, projection.field())
                || !self.expression(arguments[1].value)
            {
                return false;
            }
            self.migrated = true;
            return true;
        }
        let diverges = arguments.iter().any(|arg| {
            self.expression_type(arg.value)
                .is_some_and(|ty| self.nothing(ty))
        });
        let mut roots = Vec::new();
        for (index, argument) in arguments.iter().enumerate() {
            let arg_id = self.id(argument.value);
            if modes[index] == ParameterMode::Inout {
                let Some(symbol) = self.direct_place_symbol(argument.value) else {
                    return false;
                };
                if !self.known.contains(&symbol)
                    || self.callables.contains(&symbol)
                    || self.parameter_mode(symbol).is_some()
                    || self.inputs.typed.non_null_use(arg_id).is_some()
                    || self.symbol_type(symbol) != Some(value_ty)
                    || self.expression_type_of(arg_id) != Some(value_ty)
                {
                    return false;
                }
                let mut loans = self
                    .inputs
                    .owned
                    .loans()
                    .iter()
                    .filter(|loan| loan.call() == id && loan.argument() == arg_id);
                let Some(fact) = loans.next() else {
                    return false;
                };
                if loans.next().is_some()
                    || fact.kind() != lang_frontend::ownership_checking::LoanKind::Exclusive
                    || fact.end_span() != call_span
                    || !matches!(
                        fact.target(),
                        lang_frontend::ownership_checking::UnitLoanTarget::Place(place)
                            if place.is_root() && place.root() == symbol
                    )
                {
                    return false;
                }
                roots.push(symbol);
            } else if !self.expression(argument.value) {
                return false;
            }
        }
        if !diverges {
            let Some(plan) = self.inputs.owned.ownership_primitive(id) else {
                return false;
            };
            if plan.descriptor() != &primitive
                || plan.places().len() != roots.len()
                || plan
                    .places()
                    .iter()
                    .zip(&roots)
                    .any(|(place, symbol)| !place.is_root() || place.root() != *symbol)
            {
                return false;
            }
            let expected_transfer = match primitive.kind() {
                lang_frontend::type_checking::OwnershipPrimitiveKind::Swap => None,
                lang_frontend::type_checking::OwnershipPrimitiveKind::Replace => {
                    let Some(delivery) =
                        self.inputs
                            .owned
                            .value_deliveries()
                            .iter()
                            .find(|delivery| {
                                delivery.call() == id
                                    && delivery.argument() == primitive.operands()[1]
                            })
                    else {
                        return false;
                    };
                    Some(match delivery.kind() {
                        lang_frontend::ownership_checking::UnitValueDeliveryKind::Copy => {
                            lang_frontend::ownership_checking::OwnershipPrimitiveValueTransfer::Copy
                        }
                        lang_frontend::ownership_checking::UnitValueDeliveryKind::Move => {
                            lang_frontend::ownership_checking::OwnershipPrimitiveValueTransfer::Move
                        }
                        lang_frontend::ownership_checking::UnitValueDeliveryKind::Temporary => {
                            lang_frontend::ownership_checking::OwnershipPrimitiveValueTransfer::Temporary
                        }
                    })
                }
            };
            if plan.new_value_transfer() != expected_transfer {
                return false;
            }
        }
        self.migrated = true;
        true
    }

    fn field_replace_rhs_conflicts(
        &self,
        rhs: ExpressionId,
        owner_symbol: lang_frontend::name_resolution::UnitSymbolId,
        field: lang_frontend::name_resolution::UnitSymbolId,
    ) -> bool {
        let Ok(rhs_node) = self.inputs.parsed.ast().expressions().get(rhs) else {
            return true;
        };
        let rhs_span = rhs_node.span();
        self.inputs
            .parsed
            .ast()
            .expressions()
            .iter()
            .any(|(expr_id, node)| {
                let span = node.span();
                rhs_span.source_id() == span.source_id()
                    && rhs_span.start() <= span.start()
                    && span.end() <= rhs_span.end()
                    && matches!(node.payload(), Expression::Name)
                    && self.reference(expr_id) == Some(owner_symbol)
                    && !self.is_disjoint_nested_field_replace_receiver(
                        rhs_span,
                        expr_id,
                        owner_symbol,
                        field,
                    )
                    && !self.is_disjoint_sibling_field_borrow(rhs_span, expr_id, field)
            })
    }

    fn is_disjoint_sibling_field_borrow(
        &self,
        rhs_span: lang_frontend::source::Span,
        name_expr: ExpressionId,
        outer_field: lang_frontend::name_resolution::UnitSymbolId,
    ) -> bool {
        self.inputs
            .parsed
            .ast()
            .expressions()
            .iter()
            .any(|(call_expr, call_node)| {
                let cspan = call_node.span();
                if rhs_span.source_id() != cspan.source_id()
                    || cspan.start() < rhs_span.start()
                    || cspan.end() > rhs_span.end()
                {
                    return false;
                }
                let Expression::Call { arguments, .. } = call_node.payload() else {
                    return false;
                };
                let call_id = self.id(call_expr);
                arguments.iter().any(|arg| {
                    let projected = self.ungrouped(arg.value);
                    let Some(projection) =
                        self.inputs.typed.aggregate_projection(self.id(projected))
                    else {
                        return false;
                    };
                    let lang_frontend::type_checking::UnitAggregateProjectionReceiver::Expression(
                        receiver,
                    ) = projection.receiver()
                    else {
                        return false;
                    };
                    if receiver.source_unit() != self.inputs.source_unit
                        || self.ungrouped(receiver.expression()) != name_expr
                        || projection.field() == outer_field
                    {
                        return false;
                    }
                    let arg_id = self.id(arg.value);
                    self.inputs.owned.loans().iter().any(|loan| {
                        loan.call() == call_id
                            && (loan.argument() == arg_id || loan.argument() == self.id(projected))
                            && loan.kind() == lang_frontend::ownership_checking::LoanKind::Shared
                    })
                })
            })
    }

    fn is_disjoint_nested_field_replace_receiver(
        &self,
        rhs_span: lang_frontend::source::Span,
        name_expr: ExpressionId,
        owner_symbol: lang_frontend::name_resolution::UnitSymbolId,
        outer_field: lang_frontend::name_resolution::UnitSymbolId,
    ) -> bool {
        self.inputs
            .parsed
            .ast()
            .expressions()
            .iter()
            .any(|(call_expr, call_node)| {
                let cspan = call_node.span();
                if rhs_span.source_id() != cspan.source_id()
                    || cspan.start() < rhs_span.start()
                    || cspan.end() > rhs_span.end()
                {
                    return false;
                }
                let Expression::Call { arguments, .. } = call_node.payload() else {
                    return false;
                };
                let Some(arg0) = arguments.first() else {
                    return false;
                };
                let call_id = self.id(call_expr);
                let Some(plan) = self.inputs.owned.field_replacement(call_id) else {
                    return false;
                };
                let projected = self.ungrouped(arg0.value);
                let Some(projection) = self.inputs.typed.aggregate_projection(self.id(projected))
                else {
                    return false;
                };
                let lang_frontend::type_checking::UnitAggregateProjectionReceiver::Expression(
                    receiver,
                ) = projection.receiver()
                else {
                    return false;
                };
                receiver.source_unit() == self.inputs.source_unit
                    && self.ungrouped(receiver.expression()) == name_expr
                    && plan.place().root() == owner_symbol
                    && projection.field() != outer_field
            })
    }
}
