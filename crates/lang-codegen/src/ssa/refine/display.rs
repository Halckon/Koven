//! Shared Display receiver and layout queries for refine admission and construction.

use lang_frontend::type_checking::{BuiltinType, UnitExpressionId, UnitTypeId, UnitTypeKind};

use super::RefineInputs;

impl RefineInputs<'_> {
    pub(super) fn shared_display_receiver(
        &self,
        call: UnitExpressionId,
        operand: UnitExpressionId,
    ) -> Option<&lang_frontend::ownership_checking::UnitReceiverOwnershipTarget> {
        use lang_frontend::{
            ownership_checking::UnitReceiverOwnershipKind, type_checking::UnitCallReceiverOrigin,
        };
        let descriptor = self.typed.call(call)?;
        let receiver = descriptor.receiver()?;
        let fact = self.owned.receiver_fact(call)?;
        (descriptor.is_display_requirement()
            && fact.kind() == UnitReceiverOwnershipKind::SharedLoan
            && fact.source() == UnitCallReceiverOrigin::Expression(operand)
            && fact.receiver_type() == receiver.ty())
        .then_some(fact.target())
    }

    /// A refined enum binding is read from its existing enum storage.
    pub(super) fn display_receiver_type(
        &self,
        operand: UnitExpressionId,
    ) -> Option<(UnitTypeId, bool)> {
        let ty = self.typed.expression_type(operand)?;
        let (layout, enum_case) = match self.typed.types().get(ty)? {
            UnitTypeKind::EnumCase { root, .. } => (*root, true),
            _ => (ty, false),
        };
        self.concrete(layout).map(|ty| (ty, enum_case))
    }

    pub(super) fn builtin_display(
        &self,
        target: lang_frontend::type_checking::UnitDisplayTarget,
        ty: UnitTypeId,
    ) -> Option<BuiltinType> {
        let UnitTypeKind::Builtin(builtin) = self.typed.types().get(ty)? else {
            return None;
        };
        match target {
            lang_frontend::type_checking::UnitDisplayTarget::Builtin(expected)
                if expected != *builtin =>
            {
                None
            }
            _ => Some(*builtin),
        }
    }
}
