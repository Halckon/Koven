use std::collections::BTreeSet;

use lang_frontend::{
    ast::{ExpressionId, TypeRefId},
    ownership_checking::ConstructionRootKind,
    parser::Expression,
    type_checking::{
        Copyability, NominalKind, UnitAggregateProjectionKind, UnitAggregateProjectionReceiver,
        UnitConstructionTarget, UnitEnumCaseSignature, UnitNominalSignature, UnitTypeId,
        UnitTypeKind, UnitTypeRefId,
    },
};

use super::Checker;
use crate::ssa::unit_plan::resolve_enum_runtime_payload_types;

impl Checker<'_, '_> {
    /// A concrete Copyable `enum class` whose payloads are all scalars.
    pub(super) fn enum_type(&self, ty: UnitTypeId) -> bool {
        self.enum_signature(ty).is_some()
            && self.copyability_of(ty) == Copyability::Copyable
            && self
                .enum_payload_types(ty)
                .is_some_and(|cases| cases.iter().flatten().all(|payload| self.scalar(*payload)))
    }

    /// A concrete MoveOnly `enum class` whose payloads are supported storage types.
    pub(super) fn box_enum_type(&self, ty: UnitTypeId) -> bool {
        self.box_enum_type_seen(ty, &mut BTreeSet::new())
    }

    pub(super) fn box_enum_type_seen(
        &self,
        ty: UnitTypeId,
        active: &mut BTreeSet<UnitTypeId>,
    ) -> bool {
        let Some(cases) = self
            .enum_signature(ty)
            .filter(|_| self.copyability_of(ty) == Copyability::MoveOnly)
            .and_then(|_| self.enum_payload_types(ty))
        else {
            return false;
        };
        if !active.insert(ty) {
            return true;
        }
        let ok = cases
            .iter()
            .flatten()
            .all(|payload| self.storage_type_seen(*payload, active));
        active.remove(&ty);
        ok
    }

    pub(super) fn enum_signature(
        &self,
        ty: UnitTypeId,
    ) -> Option<(&UnitNominalSignature, Vec<UnitTypeId>)> {
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
        (nominal.kind() == NominalKind::EnumClass
            && arguments.len() == nominal.type_parameters().len())
        .then(|| (nominal, arguments.clone()))
    }

    pub(super) fn enum_payload_types(&self, ty: UnitTypeId) -> Option<Vec<Vec<UnitTypeId>>> {
        let span = self.inputs.span;
        resolve_enum_runtime_payload_types(self.inputs.typed, ty, span).ok()
    }

    /// The case of `root` whose type-test symbol is the case type `case_type`.
    fn case_of_type(
        &self,
        root: UnitTypeId,
        case_type: UnitTypeId,
    ) -> Option<&UnitEnumCaseSignature> {
        let Some(UnitTypeKind::EnumCase { case, root: own }) = self.kind(case_type) else {
            return None;
        };
        let (nominal, _) = (self.c(*own) == root)
            .then(|| self.enum_signature(root))
            .flatten()?;
        nominal
            .enum_cases()
            .iter()
            .find(|candidate| candidate.type_symbol() == *case)
    }

    /// `this` of a method whose receiver is in the subset and used by value or copy.
    pub(super) fn this_expression(&mut self, expression: ExpressionId) -> bool {
        let Some(receiver) = self.inputs.receiver else {
            return false;
        };
        let ok = self.expression_type(expression) == Some(receiver.ty)
            && self.storage_type(receiver.ty)
            && (receiver.mode == lang_frontend::type_checking::ParameterMode::Value
                || self.copyability_of(receiver.ty) == Copyability::Copyable);
        if ok {
            self.migrated = true;
        }
        ok
    }

    /// `E.C(...)` / `C`: fields delivered in source order, no root obligation (Copyable)
    /// or `Inline` root obligation (MoveOnly boxed enum).
    pub(super) fn enum_construction(&mut self, expression: ExpressionId) -> bool {
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
        let UnitConstructionTarget::EnumCase(target) = descriptor.target() else {
            return false;
        };
        let Some((nominal, type_arguments)) = self.enum_signature(result) else {
            return false;
        };
        let Some((index, case)) = nominal
            .enum_cases()
            .iter()
            .enumerate()
            .find(|(_, case)| case.value_symbol() == target)
        else {
            return false;
        };
        let payload_symbols: Vec<_> = case.payloads().iter().map(|field| field.symbol()).collect();
        let is_box_enum = self.box_enum_type(result);
        self.migrated = true;
        let Some(payloads) = self
            .enum_payload_types(result)
            .and_then(|cases| cases.get(index).cloned())
        else {
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
        let instance_arguments: Vec<_> = descriptor
            .instance()
            .type_arguments()
            .iter()
            .map(|ty| self.c(*ty))
            .collect();
        let root_ok = if self.enum_type(result) {
            plan.root_obligation().is_none()
        } else if is_box_enum {
            plan.root_obligation().is_some_and(|root| {
                root.construction() == id
                    && self.c(root.result_type()) == result
                    && root.kind() == ConstructionRootKind::Inline
            })
        } else {
            false
        };
        let identity = root_ok
            && self.expression_type(expression) == Some(result)
            && instance_arguments == type_arguments
            && descriptor.arguments().len() == payloads.len()
            && plan.target() == descriptor.target()
            && plan.terminating_operand().is_none()
            && plan.deliveries().len() == payloads.len();
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
                    && payloads.get(argument.parameter_index()) == Some(&parameter)
                    && payload_symbols.get(argument.parameter_index()).copied()
                        == argument.parameter_symbol()
                    && delivery.construction() == id
                    && delivery.argument() == value
                    && delivery.parameter_index() == argument.parameter_index()
                    && delivery.evaluation_index() == index
                    && self.adaptable(value.expression(), parameter)
                    && self.expression(value.expression())
            })
    }

    /// A payloadless case value compared with an enum subject: identity only, never built.
    pub(super) fn case_value_condition(
        &self,
        expression: ExpressionId,
        subject: UnitTypeId,
    ) -> bool {
        let id = self.id(expression);
        self.inputs
            .typed
            .construction(id)
            .filter(|descriptor| descriptor.expression() == id)
            .is_some_and(|descriptor| {
                matches!(descriptor.target(), UnitConstructionTarget::EnumCase(_))
                    && descriptor.arguments().is_empty()
                    && self.c(descriptor.result_type()) == subject
                    && self.expression_type(expression) == Some(subject)
            })
    }

    /// `is C` / `!is C` against a case of the subject's own enum root.
    pub(super) fn type_test_target(&self, type_ref: TypeRefId, subject: UnitTypeId) -> bool {
        self.inputs
            .typed
            .type_ref_type(UnitTypeRefId::new(self.inputs.source_unit, type_ref))
            .is_some_and(|target| self.case_of_type(subject, target).is_some())
    }

    /// `operand is C` as a Boolean value: a stable enum place (`this` or a known name).
    pub(super) fn type_test_expression(
        &mut self,
        operand: ExpressionId,
        type_ref: TypeRefId,
    ) -> bool {
        self.migrated = true;
        let Some(operand_type) = self.expression_type(operand) else {
            return false;
        };
        (self.enum_type(operand_type) || self.box_enum_type(operand_type))
            && self.type_test_target(type_ref, operand_type)
            && self.enum_place(operand)
    }

    /// `this` or a known local/parameter name, read without a drop of its own.
    fn enum_place(&mut self, expression: ExpressionId) -> bool {
        let Ok(node) = self.inputs.parsed.ast().expressions().get(expression) else {
            return false;
        };
        match node.payload() {
            Expression::This => self.this_expression(expression),
            Expression::Name => self.expression(expression),
            Expression::Group { expression: inner } => self.enum_place(*inner),
            _ => false,
        }
    }

    /// A scalar payload read from a receiver the flow facts narrowed to one case:
    /// `x.f` on a narrowed name or `this`, or bare `f` (implicit `this`).
    pub(super) fn enum_projection(&mut self, expression: ExpressionId) -> bool {
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
        let root = match projection.receiver() {
            UnitAggregateProjectionReceiver::This(owner) => {
                let Some(receiver) = self.inputs.receiver else {
                    return false;
                };
                if receiver.owner != owner || !self.enum_type(receiver.ty) {
                    return false;
                }
                receiver.ty
            }
            UnitAggregateProjectionReceiver::Expression(receiver) => {
                if receiver.source_unit() != self.inputs.source_unit {
                    return false;
                }
                let Some(UnitTypeKind::EnumCase { root, .. }) = self
                    .inputs
                    .typed
                    .expression_type(receiver)
                    .and_then(|ty| self.kind(ty))
                else {
                    return false;
                };
                let root = self.c(*root);
                if !(self.enum_type(root) || self.box_enum_type(root))
                    || !self.plain(receiver.expression())
                {
                    return false;
                }
                let Ok(node) = self
                    .inputs
                    .parsed
                    .ast()
                    .expressions()
                    .get(receiver.expression())
                else {
                    return false;
                };
                let place = match node.payload() {
                    Expression::This => self
                        .inputs
                        .receiver
                        .is_some_and(|this| this.ty == root && self.enum_type(root)),
                    Expression::Name => {
                        self.reference(receiver.expression()).is_some_and(|symbol| {
                            self.known.contains(&symbol) && self.symbol_type(symbol) == Some(root)
                        })
                    }
                    _ => false,
                };
                if !place {
                    return false;
                }
                root
            }
        };
        self.migrated = true;
        self.payload_case(root, projection.field()).is_some()
    }

    /// The unique case of `root` declaring payload `field`, with the field's index.
    pub(super) fn payload_case(
        &self,
        root: UnitTypeId,
        field: lang_frontend::name_resolution::UnitSymbolId,
    ) -> Option<(usize, usize)> {
        let (nominal, _) = self.enum_signature(root)?;
        let mut found = nominal
            .enum_cases()
            .iter()
            .enumerate()
            .filter_map(|(case, signature)| {
                signature
                    .payloads()
                    .iter()
                    .position(|payload| payload.symbol() == field)
                    .map(|index| (case, index))
            });
        match (found.next(), found.next()) {
            (Some(found), None) => Some(found),
            _ => None,
        }
    }
}

pub(in crate::ssa::refine::subset) mod receiver {
    //! Subset rules for class owners and member-call receivers (SPEC-0358 step 3). They mirror
    //! `build/place.rs` and `build/receiver.rs`.
    use lang_frontend::{
        ast::ExpressionId,
        name_resolution::DeclarationId,
        ownership_checking::{UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget},
        parser::Expression,
        type_checking::{Copyability, ParameterMode, UnitCallReceiverOrigin, UnitTypeId},
    };

    use super::super::Checker;

    /// The owner a class field access or receiver goes through.
    #[derive(Clone, Copy, Debug)]
    pub(in crate::ssa::refine::subset) enum Owner {
        Expression(ExpressionId),
        This(DeclarationId),
    }

    impl Checker<'_, '_> {
        /// Whether the current receiver may be used in `mode`: a class through its loan (an
        /// Inout receiver also serves a read), an enum by value or shared loan.
        fn receiver_serves(&self, mode: ParameterMode) -> Option<UnitTypeId> {
            let receiver = self.inputs.receiver?;
            let serves = match mode {
                ParameterMode::Borrow => true,
                ParameterMode::Inout => receiver.mode == ParameterMode::Inout,
                ParameterMode::Value => {
                    receiver.mode == ParameterMode::Value
                        || self.copyability_of(receiver.ty) == Copyability::Copyable
                }
            };
            serves.then_some(receiver.ty)
        }

        /// The class (or enum) type of `owner` when it can serve an access of `mode`: a known
        /// local (lent for the access), a Borrow/Inout parameter or the current receiver.
        pub(in crate::ssa::refine::subset) fn owner_type(
            &self,
            owner: Owner,
            mode: ParameterMode,
        ) -> Option<UnitTypeId> {
            match owner {
                Owner::This(declaration) => self
                    .inputs
                    .receiver
                    .filter(|receiver| receiver.owner == declaration)
                    .and_then(|_| self.receiver_serves(mode)),
                Owner::Expression(expression) => {
                    let expression = self.ungrouped(expression);
                    let node = self
                        .inputs
                        .parsed
                        .ast()
                        .expressions()
                        .get(expression)
                        .ok()?;
                    match node.payload() {
                        Expression::This => self.receiver_serves(mode),
                        Expression::Name => {
                            let symbol = self.reference(expression)?;
                            if !self.known.contains(&symbol) {
                                return None;
                            }
                            if let Some(fact) = self.inputs.typed.non_null_use(self.id(expression))
                            {
                                let narrowed = self.c(fact.narrowed_type());
                                let view_ok =
                                    (self.class(narrowed) || self.move_value_class(narrowed))
                                        && mode == ParameterMode::Borrow
                                        && self.view_symbols.contains(&symbol);
                                let copy_ok = self.value_class(narrowed)
                                    && matches!(mode, ParameterMode::Borrow | ParameterMode::Value);
                                return (fact.symbol() == symbol && (view_ok || copy_ok))
                                    .then_some(narrowed);
                            }
                            let ty = self.symbol_type(symbol)?;
                            let serves = match (self.parameter_mode(symbol), mode) {
                                // A local is lent for the access; a Copyable one is copied.
                                (None, ParameterMode::Borrow | ParameterMode::Inout)
                                | (Some(ParameterMode::Value), ParameterMode::Borrow) => true,
                                (
                                    None
                                    | Some(
                                        ParameterMode::Value
                                        | ParameterMode::Borrow
                                        | ParameterMode::Inout,
                                    ),
                                    ParameterMode::Value,
                                ) => self.copyability_of(ty) == Copyability::Copyable,
                                (Some(ParameterMode::Borrow), ParameterMode::Borrow) => true,
                                (
                                    Some(ParameterMode::Inout),
                                    ParameterMode::Borrow | ParameterMode::Inout,
                                ) => true,
                                _ => false,
                            };
                            serves.then_some(ty)
                        }
                        _ => {
                            if mode == ParameterMode::Borrow {
                                self.expression_type(expression)
                            } else {
                                None
                            }
                        }
                    }
                }
            }
        }

        /// A member call whose receiver fact matches its descriptor and whose receiver is a
        /// plain class (Borrow/Inout/Value), an enum (Value/Borrow), a value class (Value/Borrow),
        /// or a stateless object (Value/Borrow).
        pub(in crate::ssa::refine::subset) fn receiver_call(&mut self, call: ExpressionId) -> bool {
            let id = self.id(call);
            let typed = self.inputs.typed;
            let Some(receiver) = typed.call(id).and_then(|descriptor| descriptor.receiver()) else {
                return false;
            };
            let Ok(call_node) = self.inputs.parsed.ast().expressions().get(call) else {
                return false;
            };
            if let Some(cond) = self.inputs.owned.conditional_receiver_delivery(id) {
                if self.inputs.owned.receiver_fact(id).is_some()
                    || self.span_has_control_transfer(call_node.span())
                    || typed.call(id).map(|d| d.target()) != Some(cond.target())
                    || receiver.origin() != cond.source()
                    || receiver.mode() != ParameterMode::Value
                    || receiver.ty() != cond.receiver_type()
                    || !self.inputs.receiver.is_some_and(|this| {
                        this.owner == cond.owner()
                            && this.mode == ParameterMode::Value
                            && this.ty == self.c(cond.receiver_type())
                    })
                {
                    return false;
                }
                if let UnitCallReceiverOrigin::Expression(expr) = receiver.origin()
                    && (expr.source_unit() != self.inputs.source_unit
                        || !self
                            .inputs
                            .parsed
                            .ast()
                            .expressions()
                            .get(self.ungrouped(expr.expression()))
                            .is_ok_and(|node| matches!(node.payload(), Expression::This)))
                {
                    return false;
                }
                self.migrated = true;
                return true;
            }
            let Some(fact) = self.inputs.owned.receiver_fact(id) else {
                return false;
            };
            if fact.source() != receiver.origin()
                || self.c(fact.receiver_type()) != self.c(receiver.ty())
                || fact.is_receiver_reservation() != (receiver.mode() == ParameterMode::Inout)
                || fact.activation_point().is_some_and(|point| point != id)
            {
                return false;
            }
            let receiver_ty = self.c(receiver.ty());
            let kind_ok = match receiver.mode() {
                ParameterMode::Borrow | ParameterMode::Value => {
                    self.class(receiver_ty)
                        || self.enum_type(receiver_ty)
                        || self.box_enum_type(receiver_ty)
                        || self.value_class(receiver_ty)
                        || self.move_value_class(receiver_ty)
                        || self.object_type(receiver_ty)
                        || (receiver.mode() == ParameterMode::Borrow && self.string(receiver_ty))
                }
                ParameterMode::Inout => {
                    self.class(receiver_ty)
                        || self.value_class(receiver_ty)
                        || self.move_value_class(receiver_ty)
                        || self.enum_type(receiver_ty)
                        || self.box_enum_type(receiver_ty)
                }
            };
            if !kind_ok {
                return false;
            }
            if receiver.mode() == ParameterMode::Inout && !self.class(receiver_ty) {
                if self.span_has_control_transfer(call_node.span())
                    || fact.kind() != UnitReceiverOwnershipKind::ExclusiveLoan
                {
                    return false;
                }
                let ok = match (receiver.origin(), fact.target()) {
                    (
                        UnitCallReceiverOrigin::ImplicitThis(declaration),
                        UnitReceiverOwnershipTarget::This(owner),
                    ) => {
                        declaration == *owner
                            && self.inputs.receiver.is_some_and(|this| {
                                this.owner == declaration
                                    && this.mode == ParameterMode::Inout
                                    && this.ty == receiver_ty
                            })
                    }
                    (
                        UnitCallReceiverOrigin::Expression(expr),
                        UnitReceiverOwnershipTarget::This(owner),
                    ) => {
                        expr.source_unit() == self.inputs.source_unit
                            && self
                                .inputs
                                .parsed
                                .ast()
                                .expressions()
                                .get(self.ungrouped(expr.expression()))
                                .is_ok_and(|node| matches!(node.payload(), Expression::This))
                            && self.inputs.receiver.is_some_and(|this| {
                                this.owner == *owner
                                    && this.mode == ParameterMode::Inout
                                    && this.ty == receiver_ty
                            })
                    }
                    (
                        UnitCallReceiverOrigin::Expression(expr),
                        UnitReceiverOwnershipTarget::Place(place),
                    ) if place.is_root() => {
                        expr.source_unit() == self.inputs.source_unit && {
                            let root = self.ungrouped(expr.expression());
                            self.inputs
                                .parsed
                                .ast()
                                .expressions()
                                .get(root)
                                .is_ok_and(|node| matches!(node.payload(), Expression::Name))
                                && self.reference(root).is_some_and(|symbol| {
                                    symbol == place.root()
                                        && self.known.contains(&symbol)
                                        && self.parameter_mode(symbol).is_none()
                                        && self.symbol_type(symbol) == Some(receiver_ty)
                                })
                        }
                    }
                    _ => false,
                };
                self.migrated |= ok;
                return ok;
            }
            if receiver.mode() == ParameterMode::Value
                && self.copyability_of(receiver_ty) == Copyability::MoveOnly
            {
                if self.span_has_control_transfer(call_node.span()) {
                    return false;
                }
                match receiver.origin() {
                    UnitCallReceiverOrigin::ImplicitThis(declaration) => {
                        let ok = fact.kind() == UnitReceiverOwnershipKind::Move
                            && fact.target() == &UnitReceiverOwnershipTarget::This(declaration)
                            && self.inputs.receiver.is_some_and(|this| {
                                this.owner == declaration
                                    && this.mode == ParameterMode::Value
                                    && this.ty == receiver_ty
                            });
                        self.migrated |= ok;
                        return ok;
                    }
                    UnitCallReceiverOrigin::Expression(receiver_expr) => {
                        if receiver_expr.source_unit() != self.inputs.source_unit {
                            return false;
                        }
                        let root = self.ungrouped(receiver_expr.expression());
                        let target_ok = match fact.kind() {
                            UnitReceiverOwnershipKind::Move => {
                                match fact.target() {
                                    UnitReceiverOwnershipTarget::This(declaration) => {
                                        self.inputs.parsed.ast().expressions().get(root).is_ok_and(
                                            |node| matches!(node.payload(), Expression::This),
                                        ) && self.inputs.receiver.is_some_and(|this| {
                                            this.owner == *declaration
                                                && this.mode == ParameterMode::Value
                                                && this.ty == receiver_ty
                                        })
                                    }
                                    UnitReceiverOwnershipTarget::Place(place) if place.is_root() => {
                                        self.inputs.parsed.ast().expressions().get(root).is_ok_and(
                                            |node| matches!(node.payload(), Expression::Name),
                                        ) && self.reference(root).is_some_and(|symbol| {
                                            self.known.contains(&symbol)
                                                && matches!(
                                                    self.parameter_mode(symbol),
                                                    None | Some(ParameterMode::Value)
                                                )
                                                && self.symbol_type(symbol) == Some(receiver_ty)
                                                && place.root() == symbol
                                        })
                                    }
                                    _ => false,
                                }
                            }
                            UnitReceiverOwnershipKind::Temporary => {
                                fact.target()
                                    == &UnitReceiverOwnershipTarget::Temporary(receiver_expr)
                            }
                            _ => false,
                        };
                        let ok = target_ok && self.expression(receiver_expr.expression());
                        self.migrated |= ok;
                        return ok;
                    }
                }
            }
            if let UnitReceiverOwnershipTarget::Temporary(receiver_expr) = fact.target() {
                let temp_ok = matches!(
                    (receiver.mode(), fact.kind()),
                    (ParameterMode::Borrow, UnitReceiverOwnershipKind::SharedLoan)
                        | (
                            ParameterMode::Value,
                            UnitReceiverOwnershipKind::Copy | UnitReceiverOwnershipKind::Temporary
                        )
                ) && receiver.origin()
                    == UnitCallReceiverOrigin::Expression(*receiver_expr)
                    && receiver_expr.source_unit() == self.inputs.source_unit
                    && !self.span_has_control_transfer(call_node.span())
                    && self.expression(receiver_expr.expression());
                self.migrated |= temp_ok;
                return temp_ok;
            }
            let kind = match receiver.mode() {
                ParameterMode::Borrow => UnitReceiverOwnershipKind::SharedLoan,
                ParameterMode::Inout => UnitReceiverOwnershipKind::ExclusiveLoan,
                ParameterMode::Value => UnitReceiverOwnershipKind::Copy,
            };
            if fact.kind() != kind {
                return false;
            }
            let owner = match receiver.origin() {
                UnitCallReceiverOrigin::Expression(expression) => {
                    if expression.source_unit() != self.inputs.source_unit {
                        return false;
                    }
                    Owner::Expression(expression.expression())
                }
                UnitCallReceiverOrigin::ImplicitThis(declaration) => Owner::This(declaration),
            };
            let ok = self.owner_type(owner, receiver.mode()) == Some(receiver_ty);
            if receiver.mode() == ParameterMode::Inout
                || self.box_enum_type(receiver_ty)
                || self.move_value_class(receiver_ty)
                || self.object_type(receiver_ty)
                || self
                    .inputs
                    .source_plan
                    .call_site(self.inputs.source_token, id)
                    .is_some_and(|site| !site.delegation().is_empty())
                || (matches!(fact.target(), UnitReceiverOwnershipTarget::This(_))
                    && self
                        .inputs
                        .receiver
                        .is_some_and(|this| this.mode != receiver.mode()))
            {
                self.migrated |= ok;
            }
            ok
        }
    }
}
