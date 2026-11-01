//! Subset rules for class owners and member-call receivers (SPEC-0358 step 3). They mirror
//! `build/place.rs` and `build/receiver.rs`.
use lang_frontend::{
    ast::ExpressionId,
    name_resolution::DeclarationId,
    ownership_checking::{UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget},
    parser::Expression,
    type_checking::{Copyability, ParameterMode, UnitCallReceiverOrigin, UnitTypeId},
};

use super::Checker;

/// The owner a class field access or receiver goes through.
#[derive(Clone, Copy, Debug)]
pub(super) enum Owner {
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
    pub(super) fn owner_type(&self, owner: Owner, mode: ParameterMode) -> Option<UnitTypeId> {
        match owner {
            Owner::This(declaration) => self
                .inputs
                .receiver
                .filter(|receiver| receiver.owner == declaration)
                .and_then(|_| self.receiver_serves(mode)),
            Owner::Expression(expression) => {
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
                        if let Some(fact) = self.inputs.typed.non_null_use(self.id(expression)) {
                            let narrowed = self.c(fact.narrowed_type());
                            return (fact.symbol() == symbol
                                && mode == ParameterMode::Borrow
                                && self.view_symbols.contains(&symbol)
                                && self.class(narrowed))
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
                    _ => None,
                }
            }
        }
    }

    /// A member call whose receiver fact matches its descriptor and whose receiver is a
    /// plain class (Borrow/Inout/Value), an enum (Value/Borrow), a value class (Value/Borrow),
    /// or a stateless object (Value/Borrow).
    pub(super) fn receiver_call(&mut self, call: ExpressionId) -> bool {
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
            || !fact.activation_point().is_none_or(|point| point == id)
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
                            fact.target() == &UnitReceiverOwnershipTarget::Temporary(receiver_expr)
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
        if self.box_enum_type(receiver_ty)
            || self.move_value_class(receiver_ty)
            || self.object_type(receiver_ty)
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
