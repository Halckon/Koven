//! 调用实参、receiver 与 range 来源续接的完整所有权检查。
use super::*;

impl Checker<'_> {
    pub(super) fn check_call(
        &mut self,
        id: ExpressionId,
        callee: ExpressionId,
        arguments: Vec<crate::parser::CallArgument>,
        state: State,
    ) -> Result<Flows, OwnershipCheckingError> {
        let diagnostic_count = self.diagnostics.len();
        let receiver = self.receivers_by_expression.get(&id.index()).copied();
        let mut receiver_expression = None;
        let mut flows = match receiver.map(CallReceiverDescriptor::origin) {
            Some(CallReceiverOrigin::Expression(expression)) => {
                receiver_expression = Some(expression);
                let usage =
                    if receiver.is_some_and(|receiver| receiver.mode() == ParameterMode::Value) {
                        ExpressionUse::Consume
                    } else {
                        ExpressionUse::Place
                    };
                let previous = self.allowed_borrow_call;
                let range = receiver.is_some_and(|r| r.mode() == ParameterMode::Borrow)
                    && self.range_expression_is_proven(expression);
                if range {
                    self.allowed_borrow_call = Some(expression);
                }
                let checked = self.check_expression(expression, state, usage);
                self.allowed_borrow_call = previous;
                let mut flows = checked?;
                if range && self.diagnostics.len() == diagnostic_count && flows.next.is_some() {
                    self.record_range_use(
                        expression,
                        crate::ownership_checking::RangeUseSite::Call(id),
                    );
                    self.continue_range_source(
                        expression,
                        loan::ActiveLoanOwner::Call(id),
                        &mut flows,
                    );
                }
                flows
            }
            Some(CallReceiverOrigin::ImplicitThis(_)) => {
                if receiver.is_some_and(|receiver| receiver.mode() == ParameterMode::Inout)
                    && self.current_receiver_mode != Some(ParameterMode::Inout)
                {
                    self.diagnostics.push(Diagnostic::new(
                        self.sources,
                        Severity::Error,
                        self.immutable_inout_code,
                        "current this cannot supply an inout receiver",
                        self.parsed.ast().expressions().get(id)?.span(),
                    )?);
                } else if receiver.is_some_and(|receiver| {
                    receiver.mode() == ParameterMode::Value
                        && self.typed.copyability(receiver.ty()) == Some(Copyability::MoveOnly)
                }) && self.current_receiver_mode != Some(ParameterMode::Value)
                    && self.access_this(
                        AccessKind::Move,
                        self.parsed.ast().expressions().get(callee)?.span(),
                        &state,
                    )?
                {
                    self.diagnostics.push(Diagnostic::new(
                        self.sources,
                        Severity::Error,
                        self.borrowed_move_code,
                        "cannot move this from a non-owning receiver",
                        self.parsed.ast().expressions().get(id)?.span(),
                    )?);
                }
                Flows::next(state)
            }
            None => self.check_expression(callee, state, ExpressionUse::Read)?,
        };
        if self.diagnostics.len() == diagnostic_count
            && let (Some(receiver), Some(expression)) = (receiver, receiver_expression)
        {
            let span = self.parsed.ast().expressions().get(expression)?.span();
            self.apply_argument_contract(
                id,
                crate::parser::CallArgument {
                    span,
                    named_prefix: None,
                    mode_marker: None,
                    value: expression,
                },
                receiver.mode(),
                true,
                &mut flows,
            )?;
        } else if let Some(receiver) = receiver
            && let CallReceiverOrigin::ImplicitThis(nominal) = receiver.origin()
            && self.diagnostics.len() == diagnostic_count
        {
            self.apply_this_contract(id, callee, nominal, receiver.mode(), true, &mut flows)?;
        }
        self.hold_call_closures(id, receiver_expression.unwrap_or(callee), &mut flows)?;
        let modes = self.calls_by_expression.get(&id.index()).cloned();
        let cross_thread = self.cross_thread_by_expression.get(&id.index()).cloned();
        let argument_expressions = arguments
            .iter()
            .map(|argument| argument.value)
            .collect::<Vec<_>>();
        for (index, argument) in arguments.into_iter().enumerate() {
            let mode = modes.as_ref().and_then(|modes| modes.get(index)).copied();
            let crosses_thread = cross_thread
                .as_ref()
                .and_then(|effects| effects.get(index))
                .copied()
                .unwrap_or(false);
            let usage = match mode {
                Some(ParameterMode::Value) => ExpressionUse::Consume,
                Some(ParameterMode::Borrow | ParameterMode::Inout) => ExpressionUse::Place,
                None => ExpressionUse::Read,
            };
            let argument_diagnostics = self.diagnostics.len();
            let previous = self.allowed_borrow_call;
            let range = mode == Some(ParameterMode::Borrow)
                && !crosses_thread
                && self.range_expression_is_proven(argument.value);
            if range {
                self.allowed_borrow_call = Some(argument.value);
            }
            flows = if mode == Some(ParameterMode::Value) && !crosses_thread {
                self.chain_escaping_expression(flows, argument.value, usage)?
            } else {
                self.chain_expression(flows, argument.value, usage)?
            };
            self.allowed_borrow_call = previous;
            if range && self.diagnostics.len() == argument_diagnostics && flows.next.is_some() {
                self.record_range_use(
                    argument.value,
                    crate::ownership_checking::RangeUseSite::Call(id),
                );
                self.continue_range_source(
                    argument.value,
                    loan::ActiveLoanOwner::Call(id),
                    &mut flows,
                );
            }
            self.hold_call_closures(id, argument.value, &mut flows)?;
            if crosses_thread
                && self.diagnostics.len() == argument_diagnostics
                && let Some(next) = flows.next.as_ref()
            {
                self.check_cross_thread_delivery(argument.value, next)?;
            }
            if self.diagnostics.len() == argument_diagnostics
                && let Some(mode) = mode
            {
                self.apply_argument_contract(id, argument, mode, false, &mut flows)?;
            }
        }
        if self.diagnostics.len() == diagnostic_count {
            self.activate_receiver(id, &mut flows)?;
        }
        if self.diagnostics.len() == diagnostic_count
            && let Some(next) = flows.next.as_ref()
        {
            self.record_ownership_primitive(id, &argument_expressions, next)?;
        }
        self.end_call_loans(id, &mut flows);
        self.finish_call_closures(id, &mut flows);
        for expression in receiver_expression
            .into_iter()
            .chain(receiver.is_none().then_some(callee))
            .chain(argument_expressions)
        {
            self.release_last_closure_use(expression, &mut flows)?;
        }
        if flows.next.is_some()
            && self.diagnostics.len() == diagnostic_count
            && self
                .typed
                .aggregate_projection(id)
                .is_some_and(|projection| {
                    projection.kind() == AggregateProjectionKind::StructuralComponent
                })
        {
            let callee = self.parsed.ast().expressions().get(callee)?;
            if let Expression::Member { name_span, .. } = callee.payload() {
                self.reject_partial_move(id, *name_span)?;
            }
        }
        if self.is_nothing_expression(id) {
            flows.next = None;
        }
        Ok(flows)
    }
}
