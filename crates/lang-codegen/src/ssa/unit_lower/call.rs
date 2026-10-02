//! compilation-unit source call 的 Value delivery 与同步 shared-Borrow lowering。

use lang_frontend::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::{
        LoanKind as FrontendLoanKind, UnitDropPoint, UnitLoanTarget, UnitValueDeliveryKind,
        UnitValueDeliverySource,
    },
    parser::{Expression, Statement},
    source::Span,
    type_checking::{
        BuiltinType, Copyability, ExpressionCategory, ParameterMode, UnitCallDescriptor,
        UnitCallReceiverOrigin, UnitCallTarget, UnitCallableTarget, UnitExpressionId,
    },
};

use super::{
    LoweredValue, UnitExpressionLowerer, builtin_type, lowering_error, require_value,
    resolve_concrete_type, span_key,
};
use crate::ssa::{
    LoweringError, LoweringErrorKind,
    model::{
        EntityId, EntityType, LoanId, LoanKind, Operation, Origin, PlaceAccess, PlaceId, SsaTypeId,
    },
    unit_plan::resolve_unit_call_instance,
};

pub(super) struct LoweredCallArguments {
    pub(super) arguments: Vec<EntityId>,
    pub(super) created_loans: Vec<(LoanId, Span)>,
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn lower_name(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let symbol = self
            .references
            .get(&span_key(span))
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if let Some(value) = self.bindings.get(&symbol).copied() {
            return Ok(value);
        }
        let loan = self
            .borrow_bindings
            .get(&symbol)
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?;
        let id = UnitExpressionId::new(self.source_unit, expression);
        let ty = self
            .typed
            .expression_type(id)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if self.typed.copyability(ty) != Copyability::Copyable {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let ty = self.expression_ssa_type(expression, span)?;
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::Read {
                    source: PlaceAccess::Loan(loan),
                },
                vec![EntityType::Value(ty)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        Ok(LoweredValue::Value(require_value(results[0], span)?))
    }

    pub(super) fn lower_call(
        &mut self,
        expression: ExpressionId,
        callee_expression: ExpressionId,
        arguments: &[lang_frontend::parser::CallArgument],
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let call = UnitExpressionId::new(self.source_unit, expression);
        let descriptor = self
            .typed
            .call(call)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let function_value = descriptor.target() == UnitCallTarget::FunctionValue;
        if self.constant_owned.is_none()
            && arguments.iter().any(|argument| {
                self.argument_contains_control_transfer(argument.value, function_value)
            })
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if descriptor.aborts() {
            let Some(lowered) = self.lower_call_arguments(call, arguments, descriptor, span)?
            else {
                return Ok(LoweredValue::Diverged);
            };
            if !matches!(lowered.arguments.as_slice(), [EntityId::Loan(_)]) {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            self.function
                .set_terminator(
                    self.block,
                    crate::ssa::model::TerminatorKind::Abort,
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            return Ok(LoweredValue::Diverged);
        }
        if descriptor.prints_line() {
            let Some(LoweredCallArguments {
                arguments: lowered,
                created_loans,
            }) = self.lower_call_arguments(call, arguments, descriptor, span)?
            else {
                return Ok(LoweredValue::Diverged);
            };
            let [EntityId::Loan(loan)] = lowered.as_slice() else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            self.function
                .append_instruction(
                    self.block,
                    Operation::PrintString { value: *loan },
                    Vec::new(),
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            for (loan, end_span) in created_loans.into_iter().rev() {
                self.function
                    .append_instruction(
                        self.block,
                        Operation::BorrowEnd { loan },
                        Vec::new(),
                        Origin::Source(end_span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, end_span))?;
            }
            self.emit_borrow_argument_expression_drops(call)?;
            self.emit_drops(UnitDropPoint::CallReturn(call))?;
            return Ok(LoweredValue::Unit);
        }
        if descriptor.target() == UnitCallTarget::FunctionValue {
            return self.lower_function_value_call(
                expression,
                callee_expression,
                arguments,
                descriptor,
                span,
            );
        }
        let target = match descriptor.target() {
            UnitCallTarget::Declaration(target) => UnitCallableTarget::Declaration(target),
            UnitCallTarget::Symbol(target) => UnitCallableTarget::Symbol(target),
            UnitCallTarget::External(_)
            | UnitCallTarget::FunctionValue
            | UnitCallTarget::StructuralComponent(_) => {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
        };
        if arguments.len() != descriptor.arguments().len()
            || descriptor
                .arguments()
                .iter()
                .any(|argument| argument.mode() == ParameterMode::Inout)
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        let type_arguments = descriptor
            .instance()
            .type_arguments()
            .iter()
            .map(|ty| {
                resolve_concrete_type(self.typed, *ty, self.substitutions, self.static_self, span)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let receiver = descriptor
            .receiver()
            .map(|receiver| {
                resolve_concrete_type(
                    self.typed,
                    receiver.ty(),
                    self.substitutions,
                    self.static_self,
                    span,
                )
            })
            .transpose()?;
        let resolved = resolve_unit_call_instance(
            self.typed,
            self.owned,
            target,
            type_arguments,
            receiver,
            span,
        )?;
        let callee = self
            .function_ids
            .get(resolved.key())
            .copied()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let pending_this = self.constant_owned.is_some()
            && receiver.is_some_and(|ty| self.typed.copyability(ty) == Copyability::MoveOnly)
            && descriptor.receiver().is_some_and(|receiver| {
                receiver.mode() == ParameterMode::Value
                    && (matches!(receiver.origin(), UnitCallReceiverOrigin::ImplicitThis(_))
                        || self.owned.conditional_receiver_delivery(call).is_some())
            });
        let pending_receiver = descriptor.receiver().and_then(|descriptor| {
            if self.constant_owned.is_some()
                && descriptor.mode() == ParameterMode::Value
                && self.owned.conditional_receiver_delivery(call).is_none()
                && receiver.is_some_and(|ty| self.typed.copyability(ty) == Copyability::MoveOnly)
                && let UnitCallReceiverOrigin::Expression(origin) = descriptor.origin()
            {
                Some(origin)
            } else {
                None
            }
        });
        let receiver = self.lower_call_receiver(call, descriptor, span)?;
        let receiver = if resolved.delegation().is_empty() {
            receiver
        } else {
            self.lower_delegated_call_receiver(
                call,
                descriptor,
                receiver,
                resolved.delegation(),
                span,
            )?
        };
        let mut receiver = receiver;
        if let (Some(origin), Some(receiver)) = (pending_receiver, &receiver) {
            // Value receiver 在实参全部完成前仍由当前调用方持有。
            self.temporaries
                .insert(origin, require_value(receiver.entity, span)?);
        }
        let receiver_start = self.pending_operands.len();
        if let Some(receiver) = &receiver {
            self.pending_operands.push(receiver.entity);
            self.pending_operands.extend(
                receiver
                    .created_loans
                    .iter()
                    .map(|(loan, _)| EntityId::Loan(*loan)),
            );
            if let Some(writeback) = &receiver.writeback {
                self.pending_operands
                    .push(EntityId::Value(writeback.original));
                self.pending_operands.push(EntityId::Place(writeback.place));
            }
        }
        // Receiver 帧先于实参帧；reservation 尚无 loan，提前退出只结束实际建立的 loan。
        if let Some(receiver) = &receiver {
            self.pending_call_frames
                .push(super::call_lifetimes::PendingCallFrame {
                    loop_depth: self.loops.len(),
                    pending_start: receiver_start,
                    created_loans: (receiver_start + 1
                        ..receiver_start + 1 + receiver.created_loans.len())
                        .collect(),
                });
        }
        let lowered_arguments = self.lower_call_arguments(call, arguments, descriptor, span);
        if receiver.is_some() {
            self.pending_call_frames.pop();
        }
        let Some(LoweredCallArguments {
            arguments,
            created_loans,
        }) = lowered_arguments?
        else {
            self.pending_operands.truncate(receiver_start);
            return Ok(LoweredValue::Diverged);
        };
        if let Some(receiver) = &mut receiver {
            let mut rebound = self.pending_operands[receiver_start..].iter().copied();
            receiver.entity = rebound
                .next()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            for (loan, _) in &mut receiver.created_loans {
                let Some(EntityId::Loan(replacement)) = rebound.next() else {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                };
                *loan = replacement;
            }
            if let Some(writeback) = &mut receiver.writeback {
                writeback.original = require_value(
                    rebound
                        .next()
                        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?,
                    span,
                )?;
                let Some(EntityId::Place(place)) = rebound.next() else {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
                };
                // RootPlaceTake 尚只验证直接 owner/place；跨块关系留待独立扩展。
                if place != writeback.place
                    && matches!(
                        writeback.kind,
                        super::receiver::ReceiverWritebackKind::MoveOnly
                    )
                {
                    return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                }
                writeback.place = place;
            }
        }
        self.pending_operands.truncate(receiver_start);
        if let Some(receiver) = &mut receiver {
            // Borrow 实参仍存活；只有完整实参求值成功后才激活 receiver。
            self.activate_call_receiver(call, receiver, span)?;
        }
        if let (Some(origin), Some(receiver)) = (pending_receiver, &receiver) {
            self.take_owned_temporary_origin(origin, require_value(receiver.entity, span)?, span)?;
        }
        if pending_this {
            let receiver = receiver
                .as_ref()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            self.take_owned_receiver(require_value(receiver.entity, span)?, span)?;
        }
        let return_type = resolve_concrete_type(
            self.typed,
            descriptor.return_type(),
            self.substitutions,
            self.static_self,
            span,
        )?;
        let result_types = if builtin_type(self.typed, return_type) == Some(BuiltinType::Unit) {
            Vec::new()
        } else {
            vec![EntityType::Value(
                *self
                    .type_ids
                    .get(&return_type)
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?,
            )]
        };
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::DirectCall {
                    callee,
                    receiver: receiver.as_ref().map(|receiver| receiver.entity),
                    arguments,
                },
                result_types,
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        for (loan, end_span) in created_loans.into_iter().rev() {
            self.function
                .append_instruction(
                    self.block,
                    Operation::BorrowEnd { loan },
                    Vec::new(),
                    Origin::Source(end_span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, end_span))?;
        }
        if let Some(receiver) = receiver {
            for (loan, end_span) in receiver.created_loans.into_iter().rev() {
                self.function
                    .append_instruction(
                        self.block,
                        Operation::BorrowEnd { loan },
                        Vec::new(),
                        Origin::Source(end_span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, end_span))?;
            }
            if let Some(writeback) = receiver.writeback {
                if self.bindings.get(&writeback.symbol).copied()
                    != Some(LoweredValue::Value(writeback.original))
                {
                    return Err(lowering_error(
                        LoweringErrorKind::MissingFact,
                        writeback.span,
                    ));
                }
                let operation = match writeback.kind {
                    super::receiver::ReceiverWritebackKind::Copyable => Operation::Read {
                        source: PlaceAccess::Place(writeback.place),
                    },
                    super::receiver::ReceiverWritebackKind::MoveOnly => Operation::RootPlaceTake {
                        owner: writeback.original,
                        place: writeback.place,
                    },
                };
                let (_, values) = self
                    .function
                    .append_instruction(
                        self.block,
                        operation,
                        vec![EntityType::Value(writeback.target)],
                        Origin::Source(writeback.span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, writeback.span))?;
                let value = require_value(values[0], writeback.span)?;
                self.bindings
                    .insert(writeback.symbol, LoweredValue::Value(value));
            }
        }
        self.emit_borrow_argument_expression_drops(call)?;
        self.emit_drops(UnitDropPoint::CallReturn(call))?;
        match results.as_slice() {
            [] => Ok(LoweredValue::Unit),
            [result] => Ok(LoweredValue::Value(require_value(*result, span)?)),
            _ => Err(lowering_error(LoweringErrorKind::InvalidModel, span)),
        }
    }

    fn emit_borrow_argument_expression_drops(
        &mut self,
        call: UnitExpressionId,
    ) -> Result<(), LoweringError> {
        let arguments = self
            .owned
            .loans()
            .iter()
            .filter(|loan| loan.call() == call)
            .map(|loan| loan.argument())
            .collect::<Vec<_>>();
        for argument in arguments {
            self.emit_drops(UnitDropPoint::AfterExpression(argument))?;
        }
        Ok(())
    }

    pub(super) fn lower_call_arguments(
        &mut self,
        call: UnitExpressionId,
        arguments: &[lang_frontend::parser::CallArgument],
        descriptor: &UnitCallDescriptor,
        span: Span,
    ) -> Result<Option<LoweredCallArguments>, LoweringError> {
        self.pending_call_frames
            .push(super::call_lifetimes::PendingCallFrame {
                loop_depth: self.loops.len(),
                pending_start: self.pending_operands.len(),
                created_loans: Vec::new(),
            });
        let result = self.lower_call_arguments_in_frame(call, arguments, descriptor, span);
        self.pending_call_frames.pop();
        result
    }

    fn lower_call_arguments_in_frame(
        &mut self,
        call: UnitExpressionId,
        arguments: &[lang_frontend::parser::CallArgument],
        descriptor: &UnitCallDescriptor,
        span: Span,
    ) -> Result<Option<LoweredCallArguments>, LoweringError> {
        if arguments.len() != descriptor.arguments().len() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let pending_start = self.pending_operands.len();
        let mut ordered = vec![None; descriptor.arguments().len()];
        let mut created_loans = Vec::new();
        for (argument_index, argument) in arguments.iter().enumerate() {
            let mapping = descriptor
                .arguments()
                .iter()
                .find(|mapping| mapping.argument_index() == argument_index)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
            let entity = match mapping.mode() {
                ParameterMode::Value => {
                    match self.lower_value_argument(
                        call,
                        argument.value,
                        mapping.parameter_type(),
                        argument.span,
                    )? {
                        LoweredValue::Value(value) => {
                            let argument_id =
                                UnitExpressionId::new(self.source_unit, argument.value);
                            if self.constant_owned.is_some()
                                && self.typed.expression_type(argument_id).is_some_and(|ty| {
                                    self.typed.copyability(ty) == Copyability::MoveOnly
                                })
                            {
                                let origin = self
                                    .constant_materialization_origin(argument_id, argument.span)?
                                    .unwrap_or(argument_id);
                                self.temporaries.insert(origin, value);
                            }
                            EntityId::Value(value)
                        }
                        LoweredValue::Diverged => {
                            self.pending_operands.truncate(pending_start);
                            return Ok(None);
                        }
                        LoweredValue::Unit => {
                            return Err(lowering_error(
                                LoweringErrorKind::InvalidModel,
                                argument.span,
                            ));
                        }
                    }
                }
                ParameterMode::Borrow => {
                    let argument_id = UnitExpressionId::new(self.source_unit, argument.value);
                    let argument_type =
                        self.typed.expression_type(argument_id).ok_or_else(|| {
                            lowering_error(LoweringErrorKind::MissingFact, argument.span)
                        })?;
                    let argument_type = resolve_concrete_type(
                        self.typed,
                        argument_type,
                        self.substitutions,
                        self.static_self,
                        argument.span,
                    )?;
                    if builtin_type(self.typed, argument_type) == Some(BuiltinType::Nothing) {
                        return match self.lower(argument.value)? {
                            LoweredValue::Diverged => {
                                self.pending_operands.truncate(pending_start);
                                Ok(None)
                            }
                            LoweredValue::Value(_) | LoweredValue::Unit => Err(lowering_error(
                                LoweringErrorKind::InvalidModel,
                                argument.span,
                            )),
                        };
                    }
                    let parameter_type = resolve_concrete_type(
                        self.typed,
                        mapping.parameter_type(),
                        self.substitutions,
                        self.static_self,
                        argument.span,
                    )?;
                    let target = self.type_ids.get(&parameter_type).copied().ok_or_else(|| {
                        lowering_error(LoweringErrorKind::MissingFact, argument.span)
                    })?;
                    let (loan, created, end_span) = self.lower_borrow_argument(
                        call,
                        argument.value,
                        target,
                        argument.span,
                        span,
                    )?;
                    if created {
                        created_loans.push((self.pending_operands.len(), end_span));
                        self.pending_call_frames
                            .last_mut()
                            .expect("call frame is active")
                            .created_loans
                            .push(self.pending_operands.len());
                    }
                    EntityId::Loan(loan)
                }
                ParameterMode::Inout => {
                    return Err(lowering_error(
                        LoweringErrorKind::UnsupportedNode,
                        argument.span,
                    ));
                }
            };
            let pending_index = self.pending_operands.len();
            self.pending_operands.push(entity);
            let slot = ordered
                .get_mut(mapping.parameter_index())
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
            if slot.replace(pending_index).is_some() {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    argument.span,
                ));
            }
        }
        let arguments = ordered
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let arguments = arguments
            .into_iter()
            .map(|index| self.pending_operands[index])
            .collect::<Vec<_>>();
        if self.constant_owned.is_some() {
            // 正常提交的 Value 实参由 callee 接管；Borrow owner 留到 CallReturn 清理。
            self.temporaries
                .retain(|_, value| !arguments.contains(&EntityId::Value(*value)));
        }
        let created_loans = created_loans
            .into_iter()
            .map(|(index, end_span)| {
                let EntityId::Loan(loan) = self.pending_operands[index] else {
                    return Err(lowering_error(LoweringErrorKind::InvalidModel, end_span));
                };
                Ok((loan, end_span))
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.pending_operands.truncate(pending_start);
        Ok(Some(LoweredCallArguments {
            arguments,
            created_loans,
        }))
    }

    fn lower_value_argument(
        &mut self,
        call: UnitExpressionId,
        argument: ExpressionId,
        parameter_type: lang_frontend::type_checking::UnitTypeId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        let value = match self.lower(argument)? {
            LoweredValue::Value(value) => value,
            LoweredValue::Unit => self.materialize_unit_value(argument, span)?,
            LoweredValue::Diverged => return Ok(LoweredValue::Diverged),
        };
        let argument = UnitExpressionId::new(self.source_unit, argument);
        let mut deliveries = self
            .owned
            .value_deliveries()
            .iter()
            .filter(|delivery| delivery.call() == call && delivery.argument() == argument);
        let delivery = deliveries
            .next()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let ty = self
            .typed
            .expression_type(argument)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let expected = match (
            self.typed.expression_category(argument),
            self.typed.copyability(ty),
        ) {
            (Some(ExpressionCategory::Temporary), _) => UnitValueDeliveryKind::Temporary,
            (Some(ExpressionCategory::Place), Copyability::Copyable) => UnitValueDeliveryKind::Copy,
            (Some(ExpressionCategory::Place), Copyability::MoveOnly) => UnitValueDeliveryKind::Move,
            _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        if deliveries.next().is_some() || delivery.kind() != expected {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        match expected {
            UnitValueDeliveryKind::Copy => {}
            UnitValueDeliveryKind::Move => {
                let place = delivery
                    .place()
                    .filter(|place| place.is_root())
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                let actual = self.direct_place_symbol(argument.expression(), span)?;
                if actual != place.root()
                    || self.bindings.get(&actual).copied() != Some(LoweredValue::Value(value))
                {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
            }
            UnitValueDeliveryKind::Temporary => {
                let origin = self
                    .constant_materialization_origin(argument, span)?
                    .unwrap_or(argument);
                if delivery.source() != &UnitValueDeliverySource::Temporary(origin) {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
            }
        }
        let (value, transferred) =
            self.adapt_owned_value_to_expected(argument.expression(), value, parameter_type, span)?;
        match expected {
            UnitValueDeliveryKind::Copy if !transferred => {}
            UnitValueDeliveryKind::Move => {
                let place = delivery
                    .place()
                    .filter(|place| place.is_root())
                    .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
                if !transferred {
                    self.take_owned_binding(place.root(), value, span)?;
                }
            }
            UnitValueDeliveryKind::Temporary => {
                if !transferred && self.typed.copyability(ty) == Copyability::MoveOnly {
                    self.take_owned_temporary(value, span)?;
                }
            }
            UnitValueDeliveryKind::Copy => {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        Ok(LoweredValue::Value(value))
    }

    pub(super) fn lower_borrow_argument(
        &mut self,
        call: UnitExpressionId,
        argument: ExpressionId,
        target: SsaTypeId,
        span: Span,
        call_span: Span,
    ) -> Result<(LoanId, bool, Span), LoweringError> {
        let argument_id = UnitExpressionId::new(self.source_unit, argument);
        let mut facts = self
            .owned
            .loans()
            .iter()
            .filter(|fact| fact.call() == call && fact.argument() == argument_id);
        let fact = facts
            .next()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if facts.next().is_some()
            || fact.kind() != FrontendLoanKind::Shared
            || fact.end_span() != call_span
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if let UnitLoanTarget::This(owner) = fact.target() {
            return self.lower_this_borrow_argument(
                *owner,
                argument,
                target,
                fact.begin_span(),
                fact.end_span(),
            );
        }
        if let UnitLoanTarget::Place(place) = fact.target()
            && place.is_root()
            && self.direct_name_symbol(argument, span)? == Some(place.root())
            && let Some(loan) = self.borrow_bindings.get(&place.root()).copied()
        {
            let expected = EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            };
            if self
                .function
                .entity(EntityId::Loan(loan))
                .map(|data| data.ty)
                != Some(expected)
            {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            return Ok((loan, false, fact.end_span()));
        }
        let place = self.lower_borrow_place(argument, fact.target(), target, span)?;
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }],
                Origin::Source(fact.begin_span()),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, fact.begin_span()))?;
        let EntityId::Loan(loan) = results[0] else {
            return Err(lowering_error(
                LoweringErrorKind::InvalidModel,
                fact.begin_span(),
            ));
        };
        Ok((loan, true, fact.end_span()))
    }

    fn lower_borrow_place(
        &mut self,
        argument: ExpressionId,
        loan_target: &UnitLoanTarget,
        target: SsaTypeId,
        span: Span,
    ) -> Result<PlaceId, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(argument)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if let Expression::Group { expression } = node.payload() {
            return self.lower_borrow_place(*expression, loan_target, target, span);
        }
        if let Some(operation) = self
            .typed
            .rc_operation(UnitExpressionId::new(self.source_unit, argument))
            && operation.kind() == lang_frontend::type_checking::RcOperationKind::Value
        {
            let receiver = operation.receiver().expression();
            let owner = match self.lower(receiver)? {
                LoweredValue::Value(owner) => EntityId::Value(owner),
                _ => return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
            };
            let (_, places) = self
                .function
                .append_instruction(
                    self.block,
                    Operation::SharedPayloadPlace { owner },
                    vec![EntityType::Place(target)],
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            let EntityId::Place(place) = places[0] else {
                return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
            };
            return Ok(place);
        }
        if let Some(place) =
            self.lower_borrowed_container_element(argument, loan_target, target, span)?
        {
            return Ok(place);
        }
        let owner = match loan_target {
            UnitLoanTarget::Place(place) if place.is_root() => self
                .bindings
                .get(&place.root())
                .and_then(|value| match value {
                    LoweredValue::Value(value) => Some(*value),
                    LoweredValue::Unit | LoweredValue::Diverged => None,
                })
                .ok_or_else(|| lowering_error(LoweringErrorKind::UnsupportedNode, span))?,
            UnitLoanTarget::Temporary(temporary) => {
                if temporary.source_unit() != self.source_unit {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
                match self.lower(temporary.expression())? {
                    LoweredValue::Value(value) => value,
                    LoweredValue::Unit => {
                        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                    }
                    LoweredValue::Diverged => {
                        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
                    }
                }
            }
            UnitLoanTarget::Place(_) | UnitLoanTarget::This(_) => {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
        };
        let (_, results) = self
            .function
            .append_instruction(
                self.block,
                Operation::RootPlace { owner },
                vec![EntityType::Place(target)],
                Origin::Source(span),
            )
            .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        let EntityId::Place(place) = results[0] else {
            return Err(lowering_error(LoweringErrorKind::InvalidModel, span));
        };
        Ok(place)
    }

    fn direct_name_symbol(
        &self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<Option<UnitSymbolId>, LoweringError> {
        let node = self
            .parsed
            .ast()
            .expressions()
            .get(expression)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        match node.payload() {
            Expression::Name => Ok(self.references.get(&span_key(node.span())).copied()),
            Expression::Group { expression } => self.direct_name_symbol(*expression, span),
            _ => Ok(None),
        }
    }

    fn argument_contains_control_transfer(
        &self,
        argument: ExpressionId,
        function_value: bool,
    ) -> bool {
        let Ok(argument) = self.parsed.ast().expressions().get(argument) else {
            return true;
        };
        self.parsed
            .ast()
            .expressions()
            .iter()
            .filter(|(_, candidate)| {
                span_contains(argument.span(), candidate.span())
                    && matches!(
                        candidate.payload(),
                        Expression::Return { .. }
                            | Expression::Break { .. }
                            | Expression::Continue { .. }
                    )
            })
            .any(|(_, candidate)| {
                let nested_lambda = self.parsed.ast().expressions().iter().any(|(_, boundary)| {
                    matches!(boundary.payload(), Expression::Lambda { .. })
                        && span_contains(argument.span(), boundary.span())
                        && strictly_contains(boundary.span(), candidate.span())
                });
                if nested_lambda {
                    return false;
                }
                match candidate.payload() {
                    Expression::Return { .. } => true,
                    Expression::Break { .. } | Expression::Continue { .. } => {
                        function_value
                            || !self.parsed.ast().statements().iter().any(|(_, boundary)| {
                                let body = match boundary.payload() {
                                    Statement::While { body, .. }
                                    | Statement::Loop { body, .. }
                                    | Statement::For { body, .. } => *body,
                                    _ => return false,
                                };
                                self.parsed.ast().statements().get(body).is_ok_and(|body| {
                                    span_contains(argument.span(), body.span())
                                        && strictly_contains(body.span(), candidate.span())
                                })
                            })
                    }
                    _ => false,
                }
            })
    }
}

fn span_contains(owner: Span, child: Span) -> bool {
    owner.source_id() == child.source_id()
        && owner.start() <= child.start()
        && child.end() <= owner.end()
}

fn strictly_contains(owner: Span, child: Span) -> bool {
    span_contains(owner, child) && (owner.start() < child.start() || child.end() < owner.end())
}
