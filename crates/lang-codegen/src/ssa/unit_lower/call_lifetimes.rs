//! 未提交的同步调用前缀；ABI 地址槽随本帧结束，源码 owner 由 frontend drop facts 决定。
use super::*;

pub(super) mod nullable_child;
mod nullable_text;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingCallFrame {
    pub(super) nullable_children: Vec<nullable_child::PendingNullableChild>,
    pub(super) call: UnitExpressionId,
    pub(super) receiver: bool,
    pub(super) loan_arguments: Vec<(UnitExpressionId, Vec<usize>)>,
    pub(super) loop_depth: usize,
    pub(super) pending_start: usize,
    pub(super) created_loans: Vec<usize>,
    /// (owner, shared-loan) slots for sealed source addresses with no user storage LoanFact.
    pub(super) abi_slots: Vec<(usize, usize)>,
    /// 同一父 owner 的并行字段 loan 尚无 projection-aware alias 合同。
    pub(super) field_replace_owner: Option<usize>,
    /// 保留共享字段 loan 的稳定 root，使后续 sibling exclusive loan 明确拒绝。
    pub(super) shared_field_roots: Vec<UnitSymbolId>,
    /// 独占 root 的 owner 槽位须与 binding 保持同一 CFG identity。
    pub(super) exclusive_root_owners: Vec<usize>,
}

impl UnitExpressionLowerer<'_> {
    pub(super) fn drop_abi_call_owners(
        &mut self,
        owners: Vec<ValueId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        for owner in owners.into_iter().rev() {
            self.function
                .append_instruction(
                    self.block,
                    Operation::Drop { owner },
                    Vec::new(),
                    Origin::Source(span),
                )
                .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
        }
        Ok(())
    }

    /// Iteration facts authorize the same CFG-safe call prefixes through both unit views.
    /// Span containment selects the capability slice, never an ownership or cleanup action.
    pub(super) fn supports_control_prefix(&self, span: Span) -> bool {
        self.constant_owned.is_some()
            || self.owned.borrow_results().range_uses().iter().any(|fact| {
                matches!(fact.site(), lang_frontend::ownership_checking::RangeUseSite::Call(call)
                    if call.source_unit() == self.source_unit
                        && self.parsed.ast().expressions().get(call.expression()).is_ok_and(|node| node.span() == span))
            })
            || self.typed.sequential_iterations().iter().any(|descriptor| {
                descriptor.statement().source_unit() == self.source_unit
                    && self
                        .statement_span(descriptor.statement().statement())
                        .is_ok_and(|iteration| {
                            (iteration.start() <= span.start() && span.end() <= iteration.end())
                                || (span.start() <= iteration.start()
                                    && iteration.end() <= span.end())
                        })
            })
    }

    /// 声明调用复用已封闭的 root/temporary 实参前缀事务；可信文本的 Borrow receiver
    /// 另复用既有 receiver 帧。不授予普通 receiver、字段实参或借用结果新能力。
    pub(super) fn supports_direct_control_prefix(
        &self,
        call: UnitExpressionId,
        arguments: &[lang_frontend::parser::CallArgument],
        descriptor: &lang_frontend::type_checking::UnitCallDescriptor,
        span: Span,
    ) -> Result<bool, LoweringError> {
        use lang_frontend::ownership_checking::{
            LoanKind as FrontendLoanKind, UnitCallArgumentOwnershipKind, UnitLoanTarget,
            UnitValueDeliveryKind, UnitValueDeliverySource,
        };
        use lang_frontend::type_checking::{
            CallableResultSource, UnitCallReceiverOrigin, UnitCallTarget,
        };

        let UnitCallTarget::Declaration(target) = descriptor.target() else {
            return Ok(false);
        };
        let supported_receiver = descriptor.receiver().is_none_or(|receiver| {
            receiver.mode() == ParameterMode::Borrow
                && matches!(receiver.origin(), UnitCallReceiverOrigin::Expression(_))
                && builtin_type(self.typed, receiver.ty()) == Some(BuiltinType::String)
                && self
                    .typed
                    .signatures()
                    .declaration(target)
                    .and_then(|declaration| declaration.callable())
                    .and_then(|callable| callable.text_extension())
                    .is_some_and(|binding| {
                        binding.callable() == UnitCallableTarget::Declaration(target)
                            && binding.receiver_type() == receiver.ty()
                    })
        });
        if !supported_receiver
            || descriptor.result_source() != CallableResultSource::Owned
            || descriptor.aborts()
            || descriptor.prints_line()
            || self.thunk_expression.is_some()
        {
            return Ok(false);
        }
        if call.source_unit() != self.source_unit
            || descriptor.expression() != call
            || self.typed.call(call) != Some(descriptor)
            || arguments.len() != descriptor.arguments().len()
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let route = self
            .source_plan
            .call_site(self.source_token, call)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if route.key().target() != UnitCallableTarget::Declaration(target)
            || !self.function_ids.contains_key(route.key())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if !route.delegation().is_empty() || !route.key().callable_arguments().is_empty() {
            return Ok(false);
        }
        let contracts = self
            .owned
            .call_argument_contracts_for(call)
            .collect::<Vec<_>>();
        if contracts.len() != arguments.len() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let mut seen_parameters = vec![false; arguments.len()];
        for (index, argument) in arguments.iter().enumerate() {
            let mut mappings = descriptor
                .arguments()
                .iter()
                .filter(|mapping| mapping.argument_index() == index);
            let mapping = mappings
                .next()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
            let Some(seen) = seen_parameters.get_mut(mapping.parameter_index()) else {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    argument.span,
                ));
            };
            if mappings.next().is_some() || std::mem::replace(seen, true) {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    argument.span,
                ));
            }
            let argument_id = UnitExpressionId::new(self.source_unit, argument.value);
            let argument_span = self
                .parsed
                .ast()
                .expressions()
                .get(argument.value)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, argument.span))?
                .span();
            let mut matches = contracts
                .iter()
                .filter(|contract| contract.argument() == argument_id);
            let contract = matches
                .next()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, argument.span))?;
            let expected_kind = match mapping.mode() {
                ParameterMode::Value => UnitCallArgumentOwnershipKind::Value,
                ParameterMode::Borrow => UnitCallArgumentOwnershipKind::SharedLoan,
                ParameterMode::Inout => UnitCallArgumentOwnershipKind::ExclusiveLoan,
            };
            if matches.next().is_some()
                || contract.parameter_index() != mapping.parameter_index()
                || contract.parameter_type() != mapping.parameter_type()
                || contract.category() != mapping.category()
                || contract.kind() != expected_kind
                || contract.crosses_thread() != mapping.crosses_thread()
                || contract.argument_span() != argument_span
                || contract.call_span() != span
                || self.typed.expression_category(argument_id) != Some(mapping.category())
            {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    argument.span,
                ));
            }
            if mapping.mode() == ParameterMode::Inout || contract.crosses_thread() {
                return Ok(false);
            }
            let mut loans = self
                .owned
                .loans()
                .iter()
                .filter(|fact| fact.call() == call && fact.argument() == argument_id);
            let loan = loans.next();
            let mut deliveries = self
                .owned
                .value_deliveries()
                .iter()
                .filter(|fact| fact.call() == call && fact.argument() == argument_id);
            let delivery = deliveries.next();
            if loans.next().is_some() || deliveries.next().is_some() {
                return Err(lowering_error(
                    LoweringErrorKind::MissingFact,
                    argument.span,
                ));
            }
            // Nothing/不可达后缀没有成功交付事实；实际完成的实参仍由原 consumer 严格核验。
            match mapping.mode() {
                ParameterMode::Value => {
                    if loan.is_some() {
                        return Err(lowering_error(
                            LoweringErrorKind::MissingFact,
                            argument.span,
                        ));
                    }
                    if let Some(delivery) = delivery {
                        if delivery.span() != argument_span
                            || delivery.parameter_span() != contract.parameter_span()
                        {
                            return Err(lowering_error(
                                LoweringErrorKind::MissingFact,
                                argument.span,
                            ));
                        }
                        let root_or_temporary = match (delivery.kind(), delivery.source()) {
                            (
                                UnitValueDeliveryKind::Copy | UnitValueDeliveryKind::Move,
                                UnitValueDeliverySource::Place(place),
                            ) => {
                                self.direct_control_prefix_root(argument_id, place, argument_span)?
                            }
                            (
                                UnitValueDeliveryKind::Temporary,
                                UnitValueDeliverySource::Temporary(origin),
                            ) => *origin == argument_id,
                            _ => false,
                        };
                        if !root_or_temporary {
                            return Ok(false);
                        }
                    }
                }
                ParameterMode::Borrow => {
                    if delivery.is_some() {
                        return Err(lowering_error(
                            LoweringErrorKind::MissingFact,
                            argument.span,
                        ));
                    }
                    if let Some(loan) = loan {
                        if loan.kind() != FrontendLoanKind::Shared
                            || loan.begin_span() != contract.loan_begin_span()
                            || loan.end_span() != span
                            || loan.parameter_span() != contract.parameter_span()
                        {
                            return Err(lowering_error(
                                LoweringErrorKind::MissingFact,
                                argument.span,
                            ));
                        }
                        let root_or_temporary = match loan.target() {
                            UnitLoanTarget::Place(place) => {
                                self.direct_control_prefix_root(argument_id, place, argument_span)?
                            }
                            UnitLoanTarget::Temporary(origin) => {
                                mapping.category() == ExpressionCategory::Temporary
                                    && *origin == argument_id
                            }
                            UnitLoanTarget::This(_) => false,
                        };
                        if !root_or_temporary {
                            return Ok(false);
                        }
                    }
                }
                ParameterMode::Inout => return Ok(false),
            }
        }
        Ok(true)
    }

    fn direct_control_prefix_root(
        &self,
        argument: UnitExpressionId,
        place: &lang_frontend::ownership_checking::UnitOwnershipPlace,
        span: Span,
    ) -> Result<bool, LoweringError> {
        if !place.is_root() || place.root().source_unit() != self.source_unit {
            return Ok(false);
        }
        let mut expression = argument.expression();
        loop {
            let node = self
                .parsed
                .ast()
                .expressions()
                .get(expression)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            match node.payload() {
                Expression::Group { expression: inner } => expression = *inner,
                Expression::Name => break,
                _ => return Ok(false),
            }
        }
        if self.direct_name_symbol(expression, span)? != Some(place.root()) {
            return Ok(false);
        }
        let symbol = self
            .names
            .names()
            .source_units()
            .get(place.root().source_unit().index())
            .and_then(|source| {
                source
                    .resolution()
                    .symbols()
                    .get(place.root().symbol().index())
            })
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        Ok(matches!(
            symbol.kind(),
            SymbolKind::Variable | SymbolKind::ValueParameter
        ) && !symbol.is_synthetic())
    }

    pub(super) fn end_pending_abi_call_slots(
        &mut self,
        minimum_loop_depth: usize,
        span: Span,
    ) -> Result<(), LoweringError> {
        for frame in self
            .pending_call_frames
            .iter()
            .rev()
            .filter(|frame| frame.loop_depth >= minimum_loop_depth)
        {
            for &(owner, loan) in frame.abi_slots.iter().rev() {
                let Some(EntityId::Loan(loan)) = self.pending_operands.get(loan).copied() else {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                };
                let owner = require_value(
                    *self
                        .pending_operands
                        .get(owner)
                        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?,
                    span,
                )?;
                self.function
                    .append_instruction(
                        self.block,
                        Operation::BorrowEnd { loan },
                        Vec::new(),
                        Origin::Source(span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
                self.function
                    .append_instruction(
                        self.block,
                        Operation::Drop { owner },
                        Vec::new(),
                        Origin::Source(span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            }
        }
        Ok(())
    }

    /// 槽位在 CFG 重绑定后仍稳定；不可保存旧 block 的 LoanId。
    pub(super) fn end_pending_call_loans(
        &mut self,
        minimum_loop_depth: usize,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_nullable_proofs(&self.bindings, &self.nullable_proofs, span)?;
        self.validate_pending_nullable_children(
            &self.pending_call_frames,
            &self.pending_operands,
            &self.nullable_proofs,
            span,
        )?;
        let mut retained = self.pending_operands.len();
        for frame_index in (0..self.pending_call_frames.len()).rev() {
            let frame = &self.pending_call_frames[frame_index];
            if frame.loop_depth < minimum_loop_depth {
                continue;
            }
            retained = retained.min(frame.pending_start);
            let ending_slots = frame
                .created_loans
                .iter()
                .rev()
                .copied()
                .collect::<Vec<_>>();
            for index in ending_slots {
                let Some(EntityId::Loan(loan)) = self.pending_operands.get(index).copied() else {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                };
                self.end_short_call_loan(loan, span)?;
                self.pending_call_frames[frame_index]
                    .nullable_children
                    .retain(|child| child.slot != index);
            }
            for &(owner, _) in self.pending_call_frames[frame_index].abi_slots.iter().rev() {
                let value = require_value(
                    *self
                        .pending_operands
                        .get(owner)
                        .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?,
                    span,
                )?;
                self.function
                    .append_instruction(
                        self.block,
                        Operation::Drop { owner: value },
                        Vec::new(),
                        Origin::Source(span),
                    )
                    .map_err(|_| lowering_error(LoweringErrorKind::InvalidModel, span))?;
            }
        }
        self.pending_operands.truncate(retained);
        Ok(())
    }
}
