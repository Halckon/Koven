//! Source-order call operands, loans, writebacks and final dispatch.
use lang_frontend::{
    ast::ExpressionId,
    ownership_checking::UnitDropPoint,
    parser::CallArgument,
    source::Span,
    type_checking::{ParameterMode, UnitCallTarget, UnitTypeKind},
};

use super::{Builder, Lowered};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::EntityType,
    refine::{
        body::{RAbort, ROp, RTerm, Slot},
        build::receiver,
    },
};

impl Builder<'_, '_> {
    pub(super) fn call(
        &mut self,
        expression: ExpressionId,
        arguments: &[CallArgument],
        span: Span,
    ) -> Result<Lowered, LoweringError> {
        let call = self.unit(expression);
        let descriptor = self
            .inputs
            .typed
            .call(call)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
            .clone();
        if descriptor.arguments().len() != arguments.len() {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        if let Some(receiver) =
            crate::ssa::lowering_support::non_completing_call_receiver(self.inputs.typed, call)
        {
            if receiver.source_unit() != self.inputs.source_unit {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
            if self.expression(receiver.expression())? != Lowered::Diverged {
                return Err(error(LoweringErrorKind::InvalidModel, span));
            }
            for argument in arguments {
                self.unreachable_drop_scopes.push(self.span(argument.value));
            }
            return Ok(Lowered::Diverged);
        }
        let is_extension = match descriptor.target() {
            UnitCallTarget::Declaration(id) => self
                .inputs
                .typed
                .signatures()
                .declaration(id)
                .and_then(|signature| signature.callable())
                .is_some_and(|callable| {
                    callable.range_extension().is_some() || callable.text_extension().is_some()
                }),
            _ => false,
        };
        let receiver = if is_extension {
            None
        } else {
            self.receiver_before_arguments(call, &descriptor, span)?
        };
        if let Some(receiver::Receiver::Ready { ref loans, .. }) = receiver {
            self.pending_call_loans.extend(loans.iter().copied());
        }
        let ext_offset = usize::from(is_extension);
        let mut call_args = Vec::with_capacity(arguments.len() + ext_offset);
        if is_extension
            && let Some(rec) = descriptor.receiver()
            && let lang_frontend::type_checking::UnitCallReceiverOrigin::Expression(expression) =
                rec.origin()
            && expression.source_unit() == self.inputs.source_unit
        {
            call_args.push((
                0,
                expression.expression(),
                self.span(expression.expression()),
                rec.mode(),
                rec.ty(),
            ));
        }
        let mut sorted_args: Vec<_> = descriptor.arguments().iter().collect();
        sorted_args.sort_by_key(|argument| argument.argument_index());
        for argument in sorted_args {
            let call_argument = arguments
                .get(argument.argument_index())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            call_args.push((
                argument.parameter_index() + ext_offset,
                call_argument.value,
                call_argument.span,
                argument.mode(),
                argument.parameter_type(),
            ));
        }
        let mut lowered_slots: Vec<Option<Slot>> = vec![None; call_args.len()];
        let mut loans = Vec::new();
        let mut writebacks = Vec::new();
        let mut view_reads = Vec::new();
        let mut delivered = Vec::new();
        let mut call_args = call_args.into_iter();
        while let Some((param_slot, value_expression, call_arg_span, mode, parameter_type)) =
            call_args.next()
        {
            if let Some((slot, borrowed)) = self.nullable_when_argument(
                expression,
                value_expression,
                mode,
                parameter_type,
                call_arg_span,
            )? {
                lowered_slots[param_slot] = Some(slot);
                if borrowed {
                    view_reads.push(value_expression);
                } else {
                    delivered.push(value_expression);
                }
                continue;
            }
            if mode == ParameterMode::Inout {
                let (loan, writeback) = self.inout_argument(value_expression)?;
                lowered_slots[param_slot] = Some(loan);
                if let Some(writeback) = writeback {
                    loans.push(loan);
                    self.pending_call_loans.push(loan);
                    writebacks.push(writeback);
                }
                continue;
            }
            if mode == ParameterMode::Borrow
                && let Some((loan, created)) = self.lower_nested_borrow_argument(
                    call,
                    value_expression,
                    parameter_type,
                    call_arg_span,
                )?
            {
                loans.extend(created.iter().copied());
                loans.push(loan);
                self.pending_call_loans.extend(created);
                self.pending_call_loans.push(loan);
                lowered_slots[param_slot] = Some(loan);
                continue;
            }
            if mode == ParameterMode::Borrow
                && let Some((loan, created)) = self.lower_temporary_field_argument(
                    call,
                    value_expression,
                    parameter_type,
                    call_arg_span,
                )?
            {
                loans.extend(created.iter().copied());
                self.pending_call_loans.extend(created);
                lowered_slots[param_slot] = Some(loan);
                continue;
            }
            if mode == ParameterMode::Borrow
                && let Some((loan, created, read_expr)) = self.lower_short_range_argument(
                    value_expression,
                    self.c(parameter_type),
                    call_arg_span,
                )?
            {
                loans.extend(created.iter().copied());
                self.pending_call_loans.extend(created);
                lowered_slots[param_slot] = Some(loan);
                view_reads.extend(read_expr);
                continue;
            }
            if mode == ParameterMode::Borrow
                && let Some(&view) = self.inputs.views.get(&self.c(parameter_type))
                && self
                    .inputs
                    .typed
                    .expression_falls_through(self.unit(value_expression))
                    != Some(false)
            {
                // ADR-0034: a Borrow function parameter receives a callable view.
                let (lent, created, read_argument) =
                    self.callable_argument(value_expression, view, span)?;
                loans.extend(created.iter().copied());
                self.pending_call_loans.extend(created);
                lowered_slots[param_slot] = Some(lent);
                view_reads.extend(read_argument);
                continue;
            }
            if mode == ParameterMode::Borrow
                && let Some(loan) =
                    self.borrowed_move_only_loan(value_expression, self.c(parameter_type))?
            {
                self.binding_consumer_argument(call, value_expression, loan, call_arg_span)?;
                self.child_consumer_argument(call, value_expression, loan, call_arg_span)?;
                lowered_slots[param_slot] = Some(loan);
                view_reads.push(value_expression);
                continue;
            }
            if mode == ParameterMode::Borrow
                && let Some((loan, created, field_receiver)) =
                    self.borrowed_string_field_loan(value_expression, self.c(parameter_type), span)?
            {
                loans.extend(created.iter().copied());
                self.pending_call_loans.extend(created);
                lowered_slots[param_slot] = Some(loan);
                view_reads.extend(field_receiver);
                view_reads.push(value_expression);
                continue;
            }
            if mode == ParameterMode::Borrow
                && let Some((loan, created, rc_receiver)) =
                    self.borrowed_rc_payload_loan(value_expression, self.c(parameter_type), span)?
            {
                loans.extend(created.iter().copied());
                self.pending_call_loans.extend(created);
                lowered_slots[param_slot] = Some(loan);
                view_reads.extend(rc_receiver);
                view_reads.push(value_expression);
                continue;
            }
            if mode == ParameterMode::Borrow
                && let Some((loan, created, elem_arg)) = self.borrowed_container_element_loan(
                    value_expression,
                    self.c(parameter_type),
                    span,
                )?
            {
                loans.extend(created.iter().copied());
                self.pending_call_loans.extend(created);
                lowered_slots[param_slot] = Some(loan);
                view_reads.extend(elem_arg);
                continue;
            }
            if mode == ParameterMode::Borrow
                && let Some(view) = self.narrowed_view(value_expression)?
            {
                // A MoveOnly non-null use borrows through the condition's proof view.
                lowered_slots[param_slot] = Some(view);
                view_reads.push(value_expression);
                continue;
            }
            if mode == ParameterMode::Borrow
                && let Some(loan) =
                    self.reborrow_call_argument(expression, value_expression, parameter_type)?
            {
                loans.push(loan);
                self.pending_call_loans.push(loan);
                lowered_slots[param_slot] = Some(loan);
                continue;
            }
            let saved_expected = self.expected_nullable;
            if matches!(
                self.inputs.typed.types().get(self.c(parameter_type)),
                Some(UnitTypeKind::Nullable(_))
            ) {
                self.expected_nullable = self.ssa_type(self.c(parameter_type), span).ok();
            } else {
                self.expected_nullable = None;
            }
            let value = self.required(value_expression);
            self.expected_nullable = saved_expected;
            let Some(value) = value? else {
                for (_, skipped, ..) in call_args {
                    self.unreachable_drop_scopes.push(self.span(skipped));
                }
                return Ok(Lowered::Diverged);
            };
            match mode {
                ParameterMode::Value => {
                    let mut value = self.adapt(value, value_expression, self.c(parameter_type))?;
                    value = self.snapshot_binding_if_name(value_expression, value)?;
                    // Delivered at the call: a later argument's jump still drops it.
                    self.repoint_temporary(value_expression, value);
                    delivered.push(value_expression);
                    lowered_slots[param_slot] = Some(value);
                }
                ParameterMode::Borrow => {
                    let loan = self.named_argument_loan(
                        call,
                        value_expression,
                        value,
                        parameter_type,
                        call_arg_span,
                        span,
                    )?;
                    loans.push(loan);
                    self.pending_call_loans.push(loan);
                    lowered_slots[param_slot] = Some(loan);
                }
                ParameterMode::Inout => {
                    return Err(error(LoweringErrorKind::InvalidModel, span));
                }
            }
        }
        let lowered = lowered_slots
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
        for value_expression in delivered {
            self.deliver(value_expression);
        }
        let mut receiver_loans = Vec::new();
        let mut invoked_callee = None;
        let return_type = self.c(descriptor.return_type());
        let result = if descriptor.target() == UnitCallTarget::FunctionValue {
            let (result, callee) = self.invoke_callable(expression, lowered, return_type, span)?;
            if result == Lowered::Diverged {
                return Ok(result);
            }
            invoked_callee = callee;
            result
        } else if descriptor.aborts() {
            let [message] = lowered.as_slice() else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            self.terminate(RTerm::Abort(RAbort::Error { message: *message }), span)?;
            return Ok(Lowered::Diverged);
        } else if let Some(kind) = descriptor.text_primitive() {
            self.text_primitive_call(expression, kind, &lowered, span)?
        } else if descriptor.prints_line() {
            let [loan] = lowered.as_slice() else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            self.push(ROp::PrintString { value: *loan }, Vec::new(), span)?;
            Lowered::Unit
        } else {
            if !matches!(
                descriptor.target(),
                UnitCallTarget::Declaration(_) | UnitCallTarget::Symbol(_)
            ) {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let resolved = self
                .inputs
                .source_plan
                .call_site(self.inputs.source_token, call)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let key = resolved
                .dispatch_key()
                .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?;
            let callee = self
                .inputs
                .function_ids
                .get(key)
                .copied()
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            let (receiver, r_loans) = match receiver {
                Some(receiver) => {
                    let (operand, r_loans, r_wb) = self.receiver_at_call(receiver, span)?;
                    writebacks.extend(r_wb);
                    (Some(operand), r_loans)
                }
                None => (None, Vec::new()),
            };
            receiver_loans = r_loans;
            let op = ROp::DirectCall {
                callee,
                receiver,
                arguments: lowered,
            };
            if self.is_nothing(return_type) {
                self.push(op, Vec::new(), span)?;
                self.terminate(RTerm::Unreachable, span)?;
                return Ok(Lowered::Diverged);
            }
            if self.is_unit(return_type) {
                self.owner_unit_call(call, op, span)?;
                Lowered::Unit
            } else if let Some(&callable) = self.inputs.callable_returns.get(&callee) {
                Lowered::Value(self.value(op, EntityType::Value(callable), span)?)
            } else {
                Lowered::Value(self.typed_value(op, expression)?)
            }
        };
        for loan in loans.into_iter().rev() {
            self.end_short_call_loan(loan, span)?;
            self.pending_call_loans.retain(|&active| active != loan);
        }
        for loan in receiver_loans.into_iter().rev() {
            self.push(ROp::BorrowEnd { loan }, Vec::new(), span)?;
            self.pending_call_loans.retain(|&active| active != loan);
        }
        for writeback in writebacks {
            self.write_back(writeback)?;
        }
        if let Some(callee) = invoked_callee {
            self.drops(UnitDropPoint::AfterExpression(self.unit(callee)))?;
        }
        self.drops(UnitDropPoint::CallReturn(call))?;
        if let Some(receiver_desc) = descriptor.receiver()
            && receiver_desc.mode() != ParameterMode::Value
            && let lang_frontend::type_checking::UnitCallReceiverOrigin::Expression(receiver_expr) =
                receiver_desc.origin()
            && receiver_expr.source_unit() == self.inputs.source_unit
        {
            self.drops(UnitDropPoint::AfterExpression(receiver_expr))?;
        }
        for argument in view_reads {
            if !self.nullable_when_observe_drops(argument)? {
                self.drops(UnitDropPoint::AfterExpression(self.unit(argument)))?;
            }
        }
        Ok(result)
    }
}
