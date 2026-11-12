//! Evaluated call/interpolation operands remain owned until completion or control transfer.
use super::{
    DropExpressionUse, DropPlanner, OwnedThis, OwnershipCheckingError, PlannerDropFact,
    PlannerDropPoint, PlannerDropTarget, UnitCallArgumentOwnershipKind, ValueState,
};
use crate::{
    ast::ExpressionId,
    name_resolution::UnitSymbolId,
    ownership_checking::{LoanKind, UnitLoanTarget},
    source::Span,
    type_checking::{Copyability, UnitExpressionId},
};

#[derive(Clone, Debug)]
pub(super) struct PendingTemporary {
    pub(super) owner: Option<crate::ownership_checking::UnitOwnerId>,
    replace_call_consumer:
        Option<super::owner_transport::replace_call_consumer::CallConsumerCarrier>,
    pub(super) control: ExpressionId,
    target: PendingOwner,
    origin: Span,
    transfers_at_call: bool,
    pub(super) loop_depth: usize,
    prior_symbols: Vec<UnitSymbolId>,
    closure: Option<ExpressionId>,
    iteration_scopes: Vec<crate::type_checking::UnitStatementId>,
}

impl PendingTemporary {
    pub(super) fn holds_borrowed_temporary(
        &self,
        call: ExpressionId,
        argument: ExpressionId,
    ) -> bool {
        self.control == call
            && !self.transfers_at_call
            && matches!(self.target, PendingOwner::Temporary(owner) if owner == argument)
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PendingCaptureBorrow {
    pub(super) call: ExpressionId,
    pub(super) symbol: UnitSymbolId,
    pub(super) loop_depth: usize,
}

#[derive(Clone, Copy, Debug)]
enum PendingOwner {
    Temporary(ExpressionId),
    This(OwnedThis),
}

impl DropPlanner<'_, '_> {
    /// Constructor operands use the same pending-owner lifetime as Value call arguments.
    pub(super) fn construction_operands(
        &mut self,
        construction: ExpressionId,
        descriptor: &crate::type_checking::UnitConstructionDescriptor,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let mut arguments = descriptor.arguments().to_vec();
        arguments.sort_by_key(|argument| argument.evaluation_index());
        for argument in arguments {
            if !self.expression(
                argument.argument().expression(),
                DropExpressionUse::Consume,
                state,
            )? || self.checker.is_nothing_expression(argument.argument())
            {
                return Ok(false);
            }
            self.register_value_argument(construction, argument.argument().expression(), state)?;
        }
        self.finish_pending_temporaries(
            construction,
            PlannerDropPoint::AfterExpression(construction),
            state,
        );
        Ok(true)
    }

    pub(super) fn register_value_argument(
        &mut self,
        call: ExpressionId,
        argument: ExpressionId,
        state: &mut ValueState,
    ) -> Result<(), OwnershipCheckingError> {
        let unit = self.checker.unit_expression(argument);
        if self.checker.typed.expression_type(unit).is_some_and(|ty| {
            self.checker.typed.copyability(ty) == Copyability::MoveOnly
                && !self.is_stateless_object_type(ty)
        }) {
            let origin = self
                .checker
                .parsed
                .ast()
                .expressions()
                .get(argument)?
                .span();
            self.register_pending_temporary(call, argument, origin, true, state)?;
        }
        Ok(())
    }

    pub(super) fn register_borrow_temporary_argument(
        &mut self,
        call: ExpressionId,
        argument: ExpressionId,
        kind: UnitCallArgumentOwnershipKind,
        state: &mut ValueState,
    ) -> Result<(), OwnershipCheckingError> {
        // 保留既有 element backing 与直接 temporary 的选择顺序和独立生命周期路径。
        let mut temporary = self
            .checker
            .temporary_element_owner(argument)?
            .map(UnitExpressionId::expression)
            .or_else(|| {
                (kind == UnitCallArgumentOwnershipKind::SharedLoan
                    && self.is_move_only_temporary(argument))
                .then_some(argument)
            });
        if temporary.is_none() {
            temporary = self.call_temporary_field_parent(call, argument, kind)?;
        }
        if let Some(temporary) = temporary {
            let origin = self
                .checker
                .parsed
                .ast()
                .expressions()
                .get(temporary)?
                .span();
            self.hold_pending_capture_borrows(call, argument, None, state);
            self.register_pending_temporary(call, temporary, origin, false, state)?;
        }
        Ok(())
    }

    pub(super) fn capture_sources_at_call(
        &self,
        call: ExpressionId,
        operand: ExpressionId,
    ) -> Option<&std::collections::BTreeSet<UnitSymbolId>> {
        self.checker.call_capture_sources.get(&(
            self.checker.unit_expression(call),
            self.checker.unit_expression(operand),
        ))
    }

    pub(super) fn hold_pending_capture_borrows(
        &self,
        call: ExpressionId,
        operand: ExpressionId,
        holder: Option<UnitSymbolId>,
        state: &mut ValueState,
    ) {
        let Some(sources) = self.capture_sources_at_call(call, operand) else {
            return;
        };
        let loop_depth = self.loop_boundaries.len();
        for symbol in holder.into_iter().chain(sources.iter().copied()) {
            if state.position(symbol).is_some()
                && !state
                    .pending_capture_borrows
                    .iter()
                    .any(|borrow| borrow.call == call && borrow.symbol == symbol)
            {
                state.pending_capture_borrows.push(PendingCaptureBorrow {
                    call,
                    symbol,
                    loop_depth,
                });
            }
        }
    }

    /// The complete parent can need cleanup even when the loan targets a copied field value.
    pub(super) fn call_temporary_field_parent(
        &self,
        call: ExpressionId,
        argument: ExpressionId,
        kind: UnitCallArgumentOwnershipKind,
    ) -> Result<Option<ExpressionId>, OwnershipCheckingError> {
        if kind != UnitCallArgumentOwnershipKind::SharedLoan
            || self.checker.borrow_result_call(argument).is_some()
            || self.checker.range_use(argument).is_some()
        {
            return Ok(None);
        }
        let Some(receiver) = self.checker.temporary_field_owner(argument)? else {
            return Ok(None);
        };
        if !self.is_move_only_temporary(receiver.expression()) {
            return Ok(None);
        }
        let argument = self.checker.unit_expression(argument);
        let call = self.checker.unit_expression(call);
        Ok(self.checker.loans.iter().find_map(|loan| {
            if loan.call() != call || loan.argument() != argument || loan.kind() != LoanKind::Shared
            {
                return None;
            }
            let UnitLoanTarget::Temporary(owner) = loan.target() else {
                return None;
            };
            let copied_field = *owner == argument
                && self
                    .checker
                    .contracts_by_call
                    .get(&call)
                    .is_some_and(|contracts| {
                        contracts.iter().any(|contract| {
                            contract.argument() == argument
                                && self.checker.copy_to_nullable_borrow(*contract)
                        })
                    });
            (*owner == receiver || copied_field).then_some(receiver.expression())
        }))
    }

    pub(super) fn register_pending_temporary(
        &self,
        control: ExpressionId,
        expression: ExpressionId,
        origin: Span,
        transfers_at_call: bool,
        state: &mut ValueState,
    ) -> Result<(), OwnershipCheckingError> {
        let closure = self.closure_origin(expression, state)?;
        self.register_pending_owner(
            control,
            PendingOwner::Temporary(expression),
            origin,
            transfers_at_call,
            closure,
            state,
        );
        Ok(())
    }

    pub(super) fn register_pending_this(&self, control: ExpressionId, state: &mut ValueState) {
        if let Some(receiver) = state.this.take() {
            self.register_pending_owner(
                control,
                PendingOwner::This(receiver),
                receiver.origin,
                true,
                None,
                state,
            );
        }
    }

    fn push_pending_drop(&mut self, point: PlannerDropPoint, pending: PendingTemporary) {
        if let Some(closure) = pending.closure {
            for capture in self
                .checker
                .captures_of(closure)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                if capture.mode() == crate::ownership_checking::ClosureCaptureMode::Owned
                    && capture.effect() == crate::ownership_checking::ClosureCaptureEffect::Move
                {
                    self.push_fact(PlannerDropFact::new(
                        point,
                        PlannerDropTarget::Captured {
                            closure,
                            source: capture.source(),
                        },
                        capture.reference_span(),
                    ));
                } else if capture.mode() == crate::ownership_checking::ClosureCaptureMode::Shared {
                    self.iteration_actions.push((
                        point,
                        crate::ownership_checking::UnitIterationCleanupAction::EndCaptureLoan {
                            closure: self.checker.unit_expression(closure),
                            source: capture.source(),
                        },
                    ));
                }
            }
        }
        match pending.target {
            PendingOwner::Temporary(expression) => {
                let target = crate::ownership_checking::UnitDropTarget::Temporary(
                    self.checker
                        .constant_temporary_origin(expression)
                        .map_or(self.checker.unit_expression(expression), |(owner, _)| owner),
                );
                self.iteration_temporary_scopes.extend(
                    pending
                        .iteration_scopes
                        .iter()
                        .map(|statement| (*statement, target, true)),
                );
                self.push_fact(PlannerDropFact::new(
                    point,
                    PlannerDropTarget::Temporary(expression),
                    pending.origin,
                ));
            }
            PendingOwner::This(receiver) => {
                self.push_this_fact_in_iteration_scopes(point, receiver, &pending.iteration_scopes)
            }
        }
    }

    fn register_pending_owner(
        &self,
        control: ExpressionId,
        target: PendingOwner,
        origin: Span,
        transfers_at_call: bool,
        closure: Option<ExpressionId>,
        state: &mut ValueState,
    ) {
        state.pending_temporaries.push(PendingTemporary {
            owner: None,
            replace_call_consumer: None,
            control,
            target,
            origin,
            transfers_at_call,
            loop_depth: self.loop_boundaries.len(),
            prior_symbols: state.values.iter().map(|value| value.symbol).collect(),
            closure,
            iteration_scopes: state
                .iterations
                .iter()
                .map(|frame| frame.statement)
                .collect(),
        });
    }

    pub(super) fn finish_pending_temporaries(
        &mut self,
        control: ExpressionId,
        point: PlannerDropPoint,
        state: &mut ValueState,
    ) {
        for index in (0..state.pending_temporaries.len()).rev() {
            if state.pending_temporaries[index].control != control {
                continue;
            }
            let mut pending = state.pending_temporaries.remove(index);
            if pending.transfers_at_call
                && let PendingOwner::Temporary(argument) = pending.target
                && let Some(carrier) = pending.replace_call_consumer.take()
                && let Some(consumer) = carrier.transfer(
                    self.checker.unit_expression(control),
                    self.checker.unit_expression(argument),
                    pending.owner.as_ref(),
                )
                && let Some(transaction) = self.owner_loop.as_mut()
            {
                transaction.complete_replace_call_consumer(consumer);
            }
            if let PendingOwner::This(receiver) = pending.target
                && receiver.conditional_type.is_some()
            {
                // 模板保留义务，具体 MoveOnly 已交付时由后端跳过；Copyable 不析构。
                state.this = Some(receiver);
            } else if !pending.transfers_at_call {
                self.push_pending_drop(point, pending);
            }
        }
    }

    /// End abandoned call protection before unwinding newer locals and operand owners.
    pub(super) fn drop_pending_temporaries(
        &mut self,
        point: PlannerDropPoint,
        state: &mut ValueState,
        selected: impl Fn(&PendingTemporary) -> bool,
    ) {
        state.pending_borrows.retain(|(call, _)| {
            !state
                .pending_temporaries
                .iter()
                .any(|pending| pending.control == *call && selected(pending))
        });
        for index in (0..state.pending_temporaries.len()).rev() {
            if !selected(&state.pending_temporaries[index]) {
                continue;
            }
            let pending = state.pending_temporaries.remove(index);
            let newer = state
                .values
                .iter()
                .filter(|value| !pending.prior_symbols.contains(&value.symbol))
                .map(|value| value.symbol)
                .collect::<Vec<_>>();
            for symbol in newer.into_iter().rev() {
                self.drop_named(point, symbol, state);
            }
            self.push_pending_drop(point, pending);
        }
    }
}

impl DropPlanner<'_, '_> {
    pub(super) fn register_pending_identity(
        &mut self,
        call: ExpressionId,
        argument: ExpressionId,
        owner: Option<crate::ownership_checking::UnitOwnerId>,
        state: &mut ValueState,
    ) {
        if self.owner_choice.is_some() {
            let registered = self.pending_value_registered(call, argument, state);
            if let Some(choice) = &mut self.owner_choice {
                choice.replacement_pending(
                    self.checker.unit_expression(call),
                    self.checker.unit_expression(argument),
                    registered,
                );
            }
        }
        self.pending_replace_return(call, argument, state);
        let Some(owner) = owner else {
            return;
        };
        if let Some(pending) = state.pending_temporaries.iter_mut().rev().find(|pending| {
            pending.control == call
                && matches!(pending.target, PendingOwner::Temporary(id) if id == argument)
                && pending.transfers_at_call
        }) {
            pending.owner = Some(owner.clone());
            self.owner_event(
                crate::ownership_checking::UnitOwnerTransportEvent::Pending {
                    call: self.checker.unit_expression(call),
                    argument: self.checker.unit_expression(argument),
                    owner,
                },
            );
        } else {
            self.reject_owner_expression(
                argument,
                crate::ownership_checking::UnitOwnerUnsupportedReason::MissingOwner,
            );
        }
    }
    pub(super) fn pending_value_registered(
        &self,
        call: ExpressionId,
        argument: ExpressionId,
        state: &ValueState,
    ) -> bool {
        state
            .pending_temporaries
            .iter()
            .filter(|pending| {
                pending.control == call
                    && matches!(pending.target, PendingOwner::Temporary(id) if id == argument)
                    && pending.transfers_at_call
            })
            .count()
            == 1
    }

    pub(super) fn take_pending_identity(
        &self,
        call: ExpressionId,
        argument: ExpressionId,
        state: &mut ValueState,
    ) -> Option<crate::ownership_checking::UnitOwnerId> {
        state
            .pending_temporaries
            .iter_mut()
            .find(|pending| {
                pending.control == call
                    && matches!(pending.target, PendingOwner::Temporary(id) if id == argument)
                    && pending.transfers_at_call
            })
            .and_then(|pending| pending.owner.take())
    }
}

impl DropPlanner<'_, '_> {
    pub(super) fn register_replace_call_consumer(
        &mut self,
        call: ExpressionId,
        argument: ExpressionId,
        carrier: Option<super::owner_transport::replace_consumer::ReplaceConsumerCarrier>,
        state: &mut ValueState,
    ) {
        let Some(super::owner_transport::replace_consumer::ReplaceConsumerCarrier::Call(carrier)) =
            carrier
        else {
            return;
        };
        if self.pending_value_registered(call, argument, state)
            && let Some(pending) = state.pending_temporaries.iter_mut().find(|pending| {
                pending.control == call
                    && matches!(pending.target, PendingOwner::Temporary(id) if id == argument)
                    && pending.transfers_at_call
            })
            && pending.replace_call_consumer.is_none()
            && carrier.matches_pending(
                self.checker.unit_expression(call),
                self.checker.unit_expression(argument),
                pending.owner.as_ref(),
            )
        {
            pending.replace_call_consumer = Some(carrier);
        }
    }
}
