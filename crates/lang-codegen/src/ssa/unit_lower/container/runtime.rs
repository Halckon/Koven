//! Runtime length constructors retain both synchronous loans through the checked guard.
use super::*;
use crate::ssa::model::{ComparisonOperator, LoanId, ScalarConstant, TerminatorKind};

impl UnitExpressionLowerer<'_> {
    pub(in crate::ssa::unit_lower) fn lower_runtime_container(
        &mut self,
        expression: ExpressionId,
        arguments: &[lang_frontend::parser::CallArgument],
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let [size, initializer] = arguments else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let call = UnitExpressionId::new(self.source_unit, expression);
        let start = self.pending_operands.len();
        self.pending_call_frames
            .push(super::super::call_lifetimes::PendingCallFrame {
                call,
                receiver: false,
                loan_arguments: Vec::new(),
                loop_depth: self.loops.len(),
                pending_start: start,
                created_loans: Vec::new(),
                abi_slots: Vec::new(),
                field_replace_owner: None,
                shared_field_roots: Vec::new(),
                exclusive_root_owners: Vec::new(),
            });
        let result = self.runtime_container_operands(call, size, initializer, span);
        let frame = self
            .pending_call_frames
            .pop()
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let result = result?;
        if result != LoweredValue::Diverged {
            for index in frame.created_loans.into_iter().rev() {
                let loan = self.runtime_pending_loan(index, span)?;
                self.function
                    .append_instruction(
                        self.block,
                        Operation::BorrowEnd { loan },
                        Vec::new(),
                        Origin::Source(span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            }
            let owners = frame
                .abi_slots
                .iter()
                .map(|(owner, _)| require_value(self.pending_operands[*owner], span))
                .collect::<Result<Vec<_>, _>>()?;
            self.drop_abi_call_owners(owners, span)?;
            self.pending_operands.truncate(start);
            self.emit_borrow_argument_expression_drops(call)?;
            self.emit_drops(UnitDropPoint::CallReturn(call))?;
        } else {
            self.pending_operands.truncate(start);
        }
        Ok(result)
    }

    fn runtime_container_operands(
        &mut self,
        call: UnitExpressionId,
        size: &lang_frontend::parser::CallArgument,
        initializer: &lang_frontend::parser::CallArgument,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        if self.runtime_operand_exits(size.value, size.span)? {
            return Ok(LoweredValue::Diverged);
        }
        let int = self.expression_ssa_type(size.value, size.span)?;
        let size_slot = self.runtime_borrow_operand(call, size.value, int, size.span, span)?;
        let loan = self.runtime_pending_loan(size_slot, span)?;
        let (_, values) = self
            .function
            .append_instruction(
                self.block,
                Operation::Read {
                    source: PlaceAccess::Loan(loan),
                },
                vec![EntityType::Value(int)],
                Origin::Source(size.span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, size.span))?;
        let length_slot = self.pending_operands.len();
        self.pending_operands.push(values[0]);
        self.runtime_nonnegative_guard(require_value(values[0], span)?, int, span)?;
        if self.runtime_operand_exits(initializer.value, initializer.span)? {
            return Ok(LoweredValue::Diverged);
        }
        let callable = self.expression_ssa_type(initializer.value, initializer.span)?;
        let initializer_slot =
            self.runtime_borrow_operand(call, initializer.value, callable, initializer.span, span)?;
        let initializer = self.runtime_pending_loan(initializer_slot, span)?;
        let length = require_value(self.pending_operands[length_slot], span)?;
        let container = self.expression_ssa_type(call.expression(), span)?;
        let (_, values) = self
            .function
            .append_instruction(
                self.block,
                Operation::ContainerGenerateBorrowed {
                    container,
                    length,
                    initializer,
                },
                vec![EntityType::Value(container)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(LoweredValue::Value(require_value(values[0], span)?))
    }

    /// Stable indices are also registered with the existing control-transfer cleanup frame.
    pub(in crate::ssa::unit_lower) fn runtime_borrow_operand(
        &mut self,
        call: UnitExpressionId,
        expression: ExpressionId,
        ty: SsaTypeId,
        argument_span: Span,
        span: Span,
    ) -> Result<usize, LoweringError> {
        let (loan, created, _) =
            self.lower_borrow_argument(call, expression, ty, argument_span, span)?;
        let mut slots = Vec::new();
        let mut result = None;
        for current in created {
            let index = self.pending_operands.len();
            self.pending_operands.push(EntityId::Loan(current));
            if current == loan {
                result = Some(index);
            }
            slots.push(index);
        }
        let index = result.unwrap_or_else(|| {
            let index = self.pending_operands.len();
            self.pending_operands.push(EntityId::Loan(loan));
            index
        });
        let frame = self
            .pending_call_frames
            .last_mut()
            .ok_or_else(|| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        frame.created_loans.extend(&slots);
        frame
            .loan_arguments
            .push((UnitExpressionId::new(self.source_unit, expression), slots));
        Ok(index)
    }

    pub(in crate::ssa::unit_lower) fn runtime_operand_exits(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<bool, LoweringError> {
        if self.expression_builtin_type(expression, span)? != Some(BuiltinType::Nothing) {
            return Ok(false);
        }
        match self.lower(expression)? {
            LoweredValue::Diverged => Ok(true),
            _ => Err(lowering_error(LoweringErrorKind::InvalidModel, span)),
        }
    }

    pub(in crate::ssa::unit_lower) fn runtime_pending_loan(
        &self,
        index: usize,
        span: Span,
    ) -> Result<LoanId, LoweringError> {
        match self.pending_operands.get(index) {
            Some(EntityId::Loan(loan)) => Ok(*loan),
            _ => Err(lowering_error(LoweringErrorKind::InvalidModel, span)),
        }
    }

    fn runtime_nonnegative_guard(
        &mut self,
        length: ValueId,
        int: SsaTypeId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let boolean = self
            .type_ids
            .iter()
            .find_map(|(ty, ssa)| {
                (builtin_type(self.typed, *ty) == Some(BuiltinType::Boolean)).then_some(*ssa)
            })
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let (_, zero) = self
            .function
            .append_instruction(
                self.block,
                Operation::Constant(ScalarConstant::Integer(0)),
                vec![EntityType::Value(int)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let (_, negative) = self
            .function
            .append_instruction(
                self.block,
                Operation::Compare {
                    operator: ComparisonOperator::LessThan,
                    left: length,
                    right: require_value(zero[0], span)?,
                },
                vec![EntityType::Value(boolean)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let baseline = self.bindings.clone();
        let mut bindings = self.carried_bindings(&baseline, span)?;
        let mut loans = self.carried_loans(&self.borrow_bindings, span)?;
        self.carry_pending_operands(&mut bindings, &mut loans, span)?;
        let failure = self.add_carried_control_block(&bindings, &loans, span)?;
        let success = self.add_carried_control_block(&bindings, &loans, span)?;
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Conditional {
                    condition: require_value(negative[0], span)?,
                    when_true: super::super::cfg::carried_control_edge(failure, &bindings, &loans),
                    when_false: super::super::cfg::carried_control_edge(success, &bindings, &loans),
                },
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.function
            .set_terminator(failure, TerminatorKind::Abort, Origin::Source(span))
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        self.block = success;
        self.bindings = self.rebind_carried_control(&baseline, success, &bindings, &loans, span)?;
        self.borrow_bindings = self.rebind_carried_loans(success, bindings.len(), &loans, span)?;
        Ok(())
    }
}
