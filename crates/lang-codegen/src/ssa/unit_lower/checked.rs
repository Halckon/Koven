//! Checked scalar CFG preserves the existing entry-specific carry and rebind contract.
use super::{LoweredValue, UnitExpressionLowerer, builtin_type, lowering_error, require_value};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{
        CheckedArithmeticOperator, EntityId, EntityType, Operation, Origin, SsaTypeId,
        TerminatorKind, ValueId,
    },
};
use lang_frontend::{source::Span, type_checking::BuiltinType};

impl UnitExpressionLowerer<'_> {
    pub(super) fn checked(
        &mut self,
        operator: CheckedArithmeticOperator,
        left: ValueId,
        right: ValueId,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        self.checked_operation(
            Operation::CheckedArithmetic {
                operator,
                left,
                right,
            },
            ty,
            span,
        )
    }

    pub(super) fn checked_operation(
        &mut self,
        operation: Operation,
        ty: SsaTypeId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let boolean = self
            .type_ids
            .iter()
            .find_map(|(frontend, ssa)| {
                (builtin_type(self.typed, *frontend) == Some(BuiltinType::Boolean)).then_some(*ssa)
            })
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                operation,
                vec![EntityType::Value(ty), EntityType::Value(boolean)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let previous_block = self.block;
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
        let failure = self.add_carried_control_block(&carried, &carried_loans, span)?;
        let success = self.add_carried_control_block(&carried, &carried_loans, span)?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition: require_value(results[1], span)?,
                    when_true: super::cfg::carried_control_edge(failure, &carried, &carried_loans),
                    when_false: super::cfg::carried_control_edge(success, &carried, &carried_loans),
                },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.function
            .set_terminator(failure, TerminatorKind::Abort, Origin::Source(span))
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.block = success;
        self.bindings =
            self.rebind_carried_control(&baseline, success, &carried, &carried_loans, span)?;
        self.borrow_bindings =
            self.rebind_carried_loans(success, carried.len(), &carried_loans, span)?;
        self.resume_checked_map(previous_block, &baseline_borrows, span)?;
        Ok(LoweredValue::Value(require_value(results[0], span)?))
    }
}
