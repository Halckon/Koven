//! Single-file checked operations retain their existing linear and Map roles.
use super::{
    BlockId, BuiltinType, CheckedArithmeticOperator, ExpressionLowerer, Function, LoweredValue,
    LoweringError, Operation, Span, SsaTypeId, SymbolId, ValueId, builtin_type,
};
use crate::ssa::expression_core::checked::{self, CarryColumn, CheckedCarry, CheckedLowering};
use std::collections::BTreeMap;

pub(in crate::ssa) struct CheckedRoles {
    baseline: BTreeMap<SymbolId, LoweredValue>,
    carried: super::control::LinearBindings,
}

impl ExpressionLowerer<'_> {
    pub(super) fn checked(
        &mut self,
        operator: CheckedArithmeticOperator,
        left: ValueId,
        right: ValueId,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        checked::lower_checked_arithmetic(self, operator, left, right, ty, span)
    }

    pub(super) fn checked_operation(
        &mut self,
        operation: Operation,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        checked::lower_checked_operation(self, operation, ty, span)
    }
}

impl CheckedLowering for ExpressionLowerer<'_> {
    type Roles = CheckedRoles;

    fn boolean_type(&self) -> Option<SsaTypeId> {
        self.type_ids.iter().find_map(|(frontend, ssa)| {
            (builtin_type(self.typed, *frontend) == Some(BuiltinType::Boolean)).then_some(*ssa)
        })
    }

    fn ir_cursor(&mut self) -> (&mut Function, &mut BlockId) {
        (&mut *self.function, &mut self.block)
    }

    fn capture_roles(&self, span: Span) -> Result<CheckedCarry<Self::Roles>, LoweringError> {
        let baseline = self.bindings.clone();
        let carried = self.linear_binding_slots(&baseline, span)?;
        self.carry_checked_map(&carried, span)?;
        let columns = carried
            .slots
            .iter()
            .map(|slot| CarryColumn {
                source: slot.source,
                ty: slot.ty,
            })
            .collect();
        Ok(CheckedCarry {
            columns,
            roles: CheckedRoles { baseline, carried },
        })
    }

    fn restore_roles(
        &mut self,
        roles: Self::Roles,
        from: BlockId,
        success: BlockId,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.bindings =
            self.rebind_linear_bindings(&roles.baseline, success, &roles.carried, span)?;
        self.resume_checked_map(from, &roles.carried, span)
    }
}
