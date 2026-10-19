//! Unit checked operations retain their existing pending, receiver, loan and Map roles.
use super::{LoweredValue, UnitExpressionLowerer, builtin_type, cfg};
use crate::ssa::{
    LoweringError,
    expression_core::checked::{self, CheckedCarry, CheckedLowering},
    model::{
        BlockId, CheckedArithmeticOperator, EntityId, Function, LoanId, Operation, SsaTypeId,
        ValueId,
    },
};
use lang_frontend::{name_resolution::UnitSymbolId, source::Span, type_checking::BuiltinType};
use std::collections::BTreeMap;

pub(in crate::ssa) struct CheckedRoles {
    baseline: BTreeMap<UnitSymbolId, LoweredValue>,
    baseline_borrows: BTreeMap<UnitSymbolId, LoanId>,
    carried: Vec<cfg::CarriedBinding>,
    carried_loans: Vec<cfg::CarriedAccess>,
}

impl UnitExpressionLowerer<'_> {
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

impl CheckedLowering for UnitExpressionLowerer<'_> {
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
        let baseline_borrows = self.borrow_bindings.clone();
        let mut carried = if self
            .pending_operands
            .iter()
            .any(|entity| matches!(entity, EntityId::Place(_)))
        {
            // Receiver reservation/writeback 的 Copyable original 也必须与具名 binding 同步重绑。
            self.carried_bindings(&baseline, span)?
        } else {
            self.move_only_carried_bindings(&baseline, span)?
        };
        let mut carried_loans = self.carried_loans(&baseline_borrows, span)?;
        self.carry_pending_operands(&mut carried, &mut carried_loans, span)?;
        Ok(CheckedCarry {
            columns: cfg::checked_carry_columns(&carried, &carried_loans),
            roles: CheckedRoles {
                baseline,
                baseline_borrows,
                carried,
                carried_loans,
            },
        })
    }

    fn restore_roles(
        &mut self,
        roles: Self::Roles,
        from: BlockId,
        success: BlockId,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.bindings = self.rebind_carried_control(
            &roles.baseline,
            success,
            &roles.carried,
            &roles.carried_loans,
            span,
        )?;
        self.borrow_bindings =
            self.rebind_carried_loans(success, roles.carried.len(), &roles.carried_loans, span)?;
        self.resume_checked_map(from, &roles.baseline_borrows, span)
    }
}
