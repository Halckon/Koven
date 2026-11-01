//! Subset rules for intrinsic `Rc<T>` (SPEC-0359, SPEC-0378): construction, `share()` and
//! `value` projections whose payload is any supported storage type. They mirror `build/rc.rs`.
use std::collections::BTreeSet;

use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::{ConstructionRootKind, LoanKind, RcOwnershipEffectKind},
    parser::Expression,
    type_checking::{
        Copyability, ExpressionCategory, IntrinsicTypeConstructor, ParameterMode, RcOperationKind,
        UnitConstructionTarget, UnitRcOperationDescriptor, UnitTypeId, UnitTypeKind,
    },
};

use super::Checker;

impl Checker<'_, '_> {
    /// The payload of `Rc<T>` when `T` is a non-recursive supported storage type.
    pub(super) fn rc_payload(&self, ty: UnitTypeId) -> Option<UnitTypeId> {
        self.rc_payload_seen(ty, &mut BTreeSet::new())
    }

    pub(super) fn rc_payload_seen(
        &self,
        ty: UnitTypeId,
        active: &mut BTreeSet<UnitTypeId>,
    ) -> Option<UnitTypeId> {
        let Some(UnitTypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Rc,
            arguments,
        }) = self.kind(ty)
        else {
            return None;
        };
        let [payload] = arguments.as_slice() else {
            return None;
        };
        if !active.insert(ty) {
            return None;
        }
        let ok = self.storage_type_seen(*payload, active);
        active.remove(&ty);
        ok.then_some(*payload)
    }

    /// `Rc(payload)`: one delivered argument and a shared-owner root obligation.
    pub(super) fn rc_construction(&mut self, expression: ExpressionId) -> bool {
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
        let (Some(payload), [argument], [type_argument]) = (
            self.rc_payload(result),
            descriptor.arguments(),
            descriptor.instance().type_arguments(),
        ) else {
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
        let value = argument.argument();
        descriptor.target() == UnitConstructionTarget::IntrinsicRc
            && self.expression_type(expression) == Some(result)
            && self.c(*type_argument) == payload
            && self.c(argument.parameter_type()) == payload
            && value.source_unit() == self.inputs.source_unit
            && plan.target() == descriptor.target()
            && plan.terminating_operand().is_none()
            && plan.deliveries().len() == 1
            && plan.deliveries()[0].argument() == value
            && plan.root_obligation().is_some_and(|root| {
                root.construction() == id
                    && self.c(root.result_type()) == result
                    && root.kind() == ConstructionRootKind::SharedOwner
            })
            && self.expression_type(value.expression()) == Some(payload)
            && self.expression(value.expression())
    }

    /// The `Rc` operation at `expression` with its single matching ownership effect, on an
    /// owned local, Value parameter, or proven non-null `Rc<T>?` view. A Borrow `Rc` parameter
    /// stays outside: its loan is a pointer to the owner's storage, which the shared-owner
    /// operations do not follow.
    pub(super) fn rc_operation_on_owner(
        &self,
        expression: ExpressionId,
    ) -> Option<UnitRcOperationDescriptor> {
        let id = self.id(expression);
        let descriptor = self.inputs.typed.rc_operation(id)?;
        let mut effects = self
            .inputs
            .owned
            .rc_effects()
            .iter()
            .filter(|effect| effect.expression() == id);
        let effect = effects.next()?;
        let expected = match descriptor.kind() {
            RcOperationKind::Share => RcOwnershipEffectKind::Retain,
            RcOperationKind::Value => RcOwnershipEffectKind::BorrowPayload,
        };
        let receiver = descriptor.receiver();
        let shape = descriptor.expression() == id
            && effects.next().is_none()
            && effect.receiver() == receiver
            && self.c(effect.payload_type()) == self.c(descriptor.payload_type())
            && effect.kind() == expected
            && receiver.source_unit() == self.inputs.source_unit
            && self.inputs.typed.expression_category(receiver) == Some(ExpressionCategory::Place);
        let root = self.ungrouped(receiver.expression());
        let node = self.inputs.parsed.ast().expressions().get(root).ok()?;
        let payload_ty = self.c(descriptor.payload_type());
        let owner = matches!(node.payload(), Expression::Name)
            && self.reference(root).is_some_and(|symbol| {
                self.known.contains(&symbol)
                    && matches!(
                        self.parameter_mode(symbol),
                        None | Some(ParameterMode::Value)
                    )
                    && match self.inputs.typed.non_null_use(receiver) {
                        None => self
                            .symbol_type(symbol)
                            .is_some_and(|ty| self.rc_payload(ty) == Some(payload_ty)),
                        Some(fact) => {
                            fact.symbol() == symbol
                                && self.view_symbols.contains(&symbol)
                                && self.rc_payload(self.c(fact.narrowed_type())) == Some(payload_ty)
                        }
                    }
            });
        (shape && owner).then_some(descriptor)
    }

    /// `owner.share()`, or `owner.value` read by copy from a Copyable payload.
    pub(super) fn rc_operation(&mut self, expression: ExpressionId) -> bool {
        self.migrated = true;
        self.rc_operation_on_owner(expression)
            .is_some_and(|descriptor| match descriptor.kind() {
                RcOperationKind::Share => true,
                RcOperationKind::Value => {
                    let payload = self.c(descriptor.payload_type());
                    self.storage_type(payload)
                        && self.copyability_of(payload) == Copyability::Copyable
                }
            })
    }

    /// `owner.value` as a field-projection receiver: the plain-class payload type.
    pub(super) fn rc_value_owner(&mut self, receiver: ExpressionId) -> Option<UnitTypeId> {
        self.migrated = true;
        let descriptor = self.rc_operation_on_owner(receiver)?;
        (descriptor.kind() == RcOperationKind::Value
            && self.class(self.c(descriptor.payload_type())))
        .then(|| self.c(descriptor.payload_type()))
    }

    /// `owner.value` of a MoveOnly payload lent to a Borrow call parameter.
    pub(super) fn borrowed_rc_payload(
        &mut self,
        call: ExpressionId,
        value: ExpressionId,
        parameter: UnitTypeId,
    ) -> bool {
        let Ok(call_node) = self.inputs.parsed.ast().expressions().get(call) else {
            return false;
        };
        if self.span_has_control_transfer(call_node.span())
            || self.copyability_of(parameter) != Copyability::MoveOnly
            || !self.storage_type(parameter)
        {
            return false;
        }
        let root = self.ungrouped(value);
        let Some(descriptor) = self.rc_operation_on_owner(root) else {
            return false;
        };
        let ok = descriptor.kind() == RcOperationKind::Value
            && self.c(descriptor.payload_type()) == parameter
            && self.expression_type(root) == Some(parameter)
            && self.inputs.owned.loans().iter().any(|loan| {
                loan.call() == self.id(call)
                    && (loan.argument() == self.id(value) || loan.argument() == self.id(root))
                    && loan.kind() == LoanKind::Shared
            });
        self.migrated |= ok;
        ok
    }
}
