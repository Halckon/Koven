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
            ParameterMode::Borrow => {
                matches!(receiver.mode, ParameterMode::Borrow | ParameterMode::Inout)
            }
            ParameterMode::Inout => receiver.mode == ParameterMode::Inout,
            ParameterMode::Value => self.copyability_of(receiver.ty) == Copyability::Copyable,
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
                                None | Some(ParameterMode::Value | ParameterMode::Borrow),
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
    /// plain class (Borrow/Inout) or a Copyable enum (Value/Borrow).
    pub(super) fn receiver_call(&self, call: ExpressionId) -> bool {
        let id = self.id(call);
        let typed = self.inputs.typed;
        let Some(receiver) = typed.call(id).and_then(|descriptor| descriptor.receiver()) else {
            return false;
        };
        let Some(fact) = self.inputs.owned.receiver_fact(id) else {
            return false;
        };
        let kind = match receiver.mode() {
            ParameterMode::Borrow => UnitReceiverOwnershipKind::SharedLoan,
            ParameterMode::Inout => UnitReceiverOwnershipKind::ExclusiveLoan,
            ParameterMode::Value => UnitReceiverOwnershipKind::Copy,
        };
        let shape = self
            .inputs
            .owned
            .conditional_receiver_delivery(id)
            .is_none()
            && fact.source() == receiver.origin()
            && self.c(fact.receiver_type()) == self.c(receiver.ty())
            && fact.kind() == kind
            && fact.is_receiver_reservation() == (receiver.mode() == ParameterMode::Inout)
            && fact.activation_point().is_none_or(|point| point == id)
            && (!matches!(fact.target(), UnitReceiverOwnershipTarget::Temporary(_))
                || self.string(self.c(receiver.ty())));
        let kind_ok = match receiver.mode() {
            ParameterMode::Borrow => {
                self.class(self.c(receiver.ty()))
                    || self.enum_type(self.c(receiver.ty()))
                    || self.value_class(self.c(receiver.ty()))
                    || self.string(self.c(receiver.ty()))
            }
            ParameterMode::Inout => self.class(self.c(receiver.ty())),
            ParameterMode::Value => {
                self.enum_type(self.c(receiver.ty())) || self.value_class(self.c(receiver.ty()))
            }
        };
        let owner = match receiver.origin() {
            UnitCallReceiverOrigin::Expression(expression) => {
                if expression.source_unit() != self.inputs.source_unit {
                    return false;
                }
                Owner::Expression(expression.expression())
            }
            UnitCallReceiverOrigin::ImplicitThis(declaration) => Owner::This(declaration),
        };
        shape && kind_ok && self.owner_type(owner, receiver.mode()) == Some(self.c(receiver.ty()))
    }
}
