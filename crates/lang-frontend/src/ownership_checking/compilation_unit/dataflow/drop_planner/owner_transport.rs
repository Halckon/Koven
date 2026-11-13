//! Identity is transported at existing planner transitions, never reconstructed from drops.
mod branch;
mod capture;
pub(super) mod choice;
mod entry_concat;
pub(super) mod loops;
pub(super) mod replace_call_consumer;
pub(super) mod replace_consumer;
pub(super) mod replace_return_consumer;
#[cfg(test)]
mod tests;

use super::*;
use crate::ownership_checking::{
    UnitOwnerCallable, UnitOwnerDefinition, UnitOwnerId, UnitOwnerObservation,
    UnitOwnerObservationStatus, UnitOwnerOrigin, UnitOwnerTransportEvent,
    UnitOwnerUnsupportedReason,
};
use crate::type_checking::{OwnershipPrimitiveKind, UnitItemId};

/// One evaluation frame owns its result. Nested calls cannot overwrite their parent's slot.
pub(super) struct EvaluatedOwner {
    pub(super) continues: bool,
    pub(super) replace_consumer: Option<replace_consumer::ReplaceConsumerCarrier>,
    pub(super) owner: Option<UnitOwnerId>,
}

impl DropPlanner<'_, '_> {
    fn owner_observation_active(&self) -> bool {
        self.owner_observation
            .as_ref()
            .is_some_and(|observation| observation.status == UnitOwnerObservationStatus::Complete)
    }

    pub(super) fn begin_owner_function(&mut self, item: ItemId) {
        if !self.owner_observation_enabled {
            return;
        }
        let identity = std::sync::Arc::new(());
        self.owner_controls = loops::ControlStack::default();
        self.owner_choice = choice::ChoiceTransaction::begin(self.checker, item, identity.clone());
        self.owner_loop = loops::ControlTransactions::begin(self.checker, item, identity.clone());
        self.owner_branch_depth = 0;
        self.owner_observation = Some(UnitOwnerObservation {
            identity,
            callable: UnitOwnerCallable::Function(UnitItemId::new(self.checker.source_unit, item)),
            status: UnitOwnerObservationStatus::Complete,
            definitions: Vec::new(),
            events: Vec::new(),
            choice_value: None,
            loop_value: None,
            invariant_loop_value: None,
            closed_loop_batch: None,
            counter_tail: None,
            counter_batch: None,
            control_bridge: None,
            private_control_chain: None,
            nested_normal: None,
            multi_root_boundary: None,
            open_loop_value: None,
        });
    }

    pub(super) fn begin_unsupported_owner(
        &mut self,
        callable: UnitOwnerCallable,
        point: crate::ownership_checking::UnitDropPoint,
    ) {
        if !self.owner_observation_enabled {
            return;
        }
        self.owner_controls = loops::ControlStack::default();
        self.owner_choice = None;
        self.owner_loop.clear();
        self.owner_branch_depth = 0;
        self.owner_observation = Some(UnitOwnerObservation {
            identity: std::sync::Arc::new(()),
            callable,
            status: UnitOwnerObservationStatus::Unsupported {
                point,
                reason: UnitOwnerUnsupportedReason::Callable,
            },
            definitions: Vec::new(),
            events: Vec::new(),
            choice_value: None,
            loop_value: None,
            invariant_loop_value: None,
            closed_loop_batch: None,
            counter_tail: None,
            counter_batch: None,
            control_bridge: None,
            private_control_chain: None,
            nested_normal: None,
            multi_root_boundary: None,
            open_loop_value: None,
        });
    }

    pub(super) fn finish_owner_observation(&mut self) {
        // Unbalanced/invalid control entry cannot publish a sealed loop product.
        if !self.owner_controls.balanced() {
            if let Some(observation) = &self.owner_observation {
                let point = match observation.callable {
                    UnitOwnerCallable::Function(item) | UnitOwnerCallable::Deinit(item) => {
                        crate::ownership_checking::UnitDropPoint::FunctionEntry(item)
                    }
                    UnitOwnerCallable::Lambda(expression) => {
                        crate::ownership_checking::UnitDropPoint::LambdaEntry(expression)
                    }
                };
                self.reject_owner(point, UnitOwnerUnsupportedReason::InconsistentTransport);
            }
            self.owner_loop.clear();
        }
        if let Some(control) = self.owner_loop.publication_blocker() {
            self.reject_owner(
                crate::ownership_checking::UnitDropPoint::LoopExit(control),
                UnitOwnerUnsupportedReason::Syntax,
            );
        }
        let choice = self
            .owner_choice
            .take()
            .and_then(choice::ChoiceTransaction::finish);
        let nested_normal = self.owner_loop.take_nested_normal();
        let private_control_chain = self.owner_loop.take_private_chain();
        let control_bridge = self.owner_loop.take_control_bridge();
        let counter_batch = self.owner_loop.take_counter_batch();
        let counter_tail = self.owner_loop.counter_tail_evidence();
        let multi_root_boundary = self.owner_loop.take_root_boundary();
        let closed_loop_batch = self.owner_loop.take_closed_batch();
        let (loop_value, invariant_loop_value, open_loop_value) = self
            .owner_loop
            .take()
            .map(loops::LoopTransaction::finish)
            .unwrap_or_default();
        if let Some(mut observation) = self.owner_observation.take() {
            observation.counter_tail = counter_tail.map(Box::new);
            observation.counter_batch = counter_batch.map(Box::new);
            observation.control_bridge = control_bridge.map(Box::new);
            observation.private_control_chain = private_control_chain.map(Box::new);
            observation.nested_normal = nested_normal.map(Box::new);
            observation.closed_loop_batch = closed_loop_batch.map(Box::new);
            observation.multi_root_boundary = multi_root_boundary.map(Box::new);
            observation.open_loop_value = open_loop_value;
            observation.loop_value = loop_value.map(Box::new);
            observation.invariant_loop_value = invariant_loop_value.map(Box::new);
            observation.choice_value = choice.map(Box::new);
            if !observation.is_consistent() {
                let point = match observation.callable {
                    UnitOwnerCallable::Function(item) | UnitOwnerCallable::Deinit(item) => {
                        crate::ownership_checking::UnitDropPoint::FunctionEntry(item)
                    }
                    UnitOwnerCallable::Lambda(expression) => {
                        crate::ownership_checking::UnitDropPoint::LambdaEntry(expression)
                    }
                };
                observation.status = UnitOwnerObservationStatus::Unsupported {
                    point,
                    reason: UnitOwnerUnsupportedReason::InconsistentTransport,
                };
                observation.counter_tail = None;
                observation.counter_batch = None;
                observation.control_bridge = None;
                observation.private_control_chain = None;
                observation.nested_normal = None;
                observation.multi_root_boundary = None;
                observation.definitions.clear();
                observation.events.clear();
            }
            self.owner_observations.push(observation);
        }
    }

    fn reject_owner(
        &mut self,
        point: crate::ownership_checking::UnitDropPoint,
        reason: UnitOwnerUnsupportedReason,
    ) {
        if let Some(observation) = &mut self.owner_observation
            && observation.status == UnitOwnerObservationStatus::Complete
        {
            observation.status = UnitOwnerObservationStatus::Unsupported { point, reason };
            observation.definitions.clear();
            observation.events.clear();
        }
    }

    pub(super) fn reject_owner_callable(&mut self, item: ItemId) {
        self.reject_owner(
            PlannerDropPoint::FunctionEntry(item).into_unit(self.checker.source_unit),
            UnitOwnerUnsupportedReason::Callable,
        );
    }

    pub(super) fn reject_owner_expression(
        &mut self,
        expression: ExpressionId,
        reason: UnitOwnerUnsupportedReason,
    ) {
        self.reject_owner(
            PlannerDropPoint::AfterExpression(expression).into_unit(self.checker.source_unit),
            reason,
        );
    }

    pub(super) fn owner_event(&mut self, event: UnitOwnerTransportEvent) {
        self.iteration_actions.multi_region_owner_event(&event);
        self.observe_distinct_auxiliary_owner_event(&event);
        self.iteration_actions
            .observe_distinct_region_owner_event(&event);
        self.iteration_actions
            .observe_later_region_owner_event(&event);
        #[cfg(test)]
        if let Some(probe) = &mut self.owner_hook_probe {
            probe.push(event.clone());
        }
        if let Some(observation) = &mut self.owner_observation
            && observation.status == UnitOwnerObservationStatus::Complete
        {
            observation.events.push(event);
        }
    }

    fn define_owner(&mut self, origin: UnitOwnerOrigin) -> Option<UnitOwnerId> {
        let observation = self.owner_observation.as_mut()?;
        if observation.status != UnitOwnerObservationStatus::Complete {
            return None;
        }
        let id = UnitOwnerId {
            identity: std::sync::Arc::clone(&observation.identity),
            callable: observation.callable,
            index: observation.definitions.len(),
        };
        observation.definitions.push(UnitOwnerDefinition {
            id: id.clone(),
            origin,
        });
        Some(id)
    }

    pub(super) fn parameter_owner(
        &mut self,
        item: ItemId,
        symbol: UnitSymbolId,
    ) -> Option<UnitOwnerId> {
        // First slice has no resource/closure/nullable/component parameter provenance.
        if self
            .checker
            .typed
            .symbol_type(symbol)
            .and_then(|ty| self.checker.typed.types().get(ty))
            != Some(&UnitTypeKind::Builtin(BuiltinType::String))
        {
            self.reject_owner_callable(item);
            return None;
        }
        self.define_owner(UnitOwnerOrigin::Parameter(symbol))
    }

    pub(super) fn define_string_owner(&mut self, expression: ExpressionId) -> Option<UnitOwnerId> {
        self.owner_loop
            .segment_literal(self.checker.unit_expression(expression));
        self.record_root_literal(expression);
        if let Some(choice) = &mut self.owner_choice {
            choice.string_completed(self.checker.unit_expression(expression));
        }
        self.define_owner(UnitOwnerOrigin::String(
            self.checker.unit_expression(expression),
        ))
    }

    pub(super) fn moved_owner(
        &mut self,
        expression: ExpressionId,
        value: Option<OwnedValue>,
    ) -> Option<UnitOwnerId> {
        if self.is_string_expression(expression) {
            self.owner_loop
                .segment_move(self.checker.unit_expression(expression), value.as_ref());
        }
        self.move_root_suffix(expression, value.as_ref());
        self.take_conditional_return(expression, value.as_ref());
        self.take_open_loop_return(expression, value.as_ref());
        if let Some(choice) = &mut self.owner_choice {
            choice.moved(
                self.checker.unit_expression(expression),
                value.as_ref().map(|value| value.symbol),
            );
        }
        let owner = value.as_ref().and_then(|value| value.owner.clone());
        if let (Some(value), Some(owner)) = (value, owner.as_ref()) {
            self.owner_event(UnitOwnerTransportEvent::Move {
                expression: self.checker.unit_expression(expression),
                symbol: value.symbol,
                owner: owner.clone(),
            });
        }
        if owner.is_none() && self.is_string_expression(expression) {
            self.reject_owner_expression(expression, UnitOwnerUnsupportedReason::MissingOwner);
        }
        owner
    }

    pub(super) fn bound_owner(
        &mut self,
        symbol: UnitSymbolId,
        initializer: ExpressionId,
        owner: Option<UnitOwnerId>,
    ) -> Option<UnitOwnerId> {
        self.owner_loop
            .segment_bind(symbol, self.checker.unit_expression(initializer));
        self.bind_root_suffix(symbol, initializer);
        self.bind_root_old_token(symbol, initializer);
        if let Some(owner) = owner.as_ref() {
            self.owner_event(UnitOwnerTransportEvent::Binding {
                symbol,
                initializer: self.checker.unit_expression(initializer),
                owner: owner.clone(),
            });
        } else {
            self.reject_owner_expression(initializer, UnitOwnerUnsupportedReason::MissingOwner);
        }
        owner
    }

    pub(super) fn inserted_owner(&mut self, symbol: UnitSymbolId, initializer: ExpressionId) {
        if let Some(choice) = &mut self.owner_choice {
            choice.bound(symbol, self.checker.unit_expression(initializer));
        }
    }

    pub(super) fn check_owner_statement(
        &mut self,
        id: StatementId,
        state: &ValueState,
    ) -> Result<(), OwnershipCheckingError> {
        self.owner_loop
            .segment_statement(UnitStatementId::new(self.checker.source_unit, id), state);
        self.begin_nested_resume_statement(id, state);
        self.begin_root_suffix_statement(id, state);
        self.prepare_owner_loop_input(id, state);
        if matches!(
            self.checker.parsed.ast().statements().get(id)?.payload(),
            Statement::While { .. }
        ) {
            self.owner_loop.record_root_entry(
                crate::type_checking::UnitStatementId::new(self.checker.source_unit, id),
                state,
                self.owner_observation.as_ref(),
            );
        }
        if !self.owner_observation_active() {
            return Ok(());
        }
        if !matches!(
            self.checker.parsed.ast().statements().get(id)?.payload(),
            Statement::Block { .. }
                | Statement::LocalVariable { .. }
                | Statement::Expression { .. }
        ) {
            self.reject_owner(
                PlannerDropPoint::AfterStatement(id).into_unit(self.checker.source_unit),
                UnitOwnerUnsupportedReason::Syntax,
            );
        }
        Ok(())
    }

    fn check_owner_expression(&mut self, id: ExpressionId) -> Result<(), OwnershipCheckingError> {
        // Unsupported owner observation may retain stale IDs in legacy ValueState.
        // Never infer preservation across a potentially mutating evaluated expression.
        if self.owner_controls.current().is_some()
            && matches!(
                self.checker.parsed.ast().expressions().get(id)?.payload(),
                Expression::Call { .. } | Expression::Assignment { .. }
            )
        {
            self.owner_loop.invalidate_root_preservation();
        }
        if !self.owner_observation_active() {
            return Ok(());
        }
        if self.checker.is_constant_use(id) {
            self.reject_owner_expression(id, UnitOwnerUnsupportedReason::Syntax);
            return Ok(());
        }
        let supported = match self.checker.parsed.ast().expressions().get(id)?.payload() {
            Expression::Name | Expression::Group { .. } | Expression::Return { .. } => true,
            Expression::Prefix {
                operator: crate::parser::PrefixOperator::Not,
                ..
            } => self.owner_loop_condition_supported(id),
            Expression::Break { .. } | Expression::Continue { .. } => {
                self.owner_loop_jump_supported(id)
            }
            Expression::If {
                else_branch: None, ..
            } => {
                self.conditional_break_expression_supported(id)
                    || self.conditional_return_expression_supported(id)
            }
            Expression::If {
                else_branch: Some(_),
                ..
            } => {
                // A both-return If has Nothing type. Its actual terminal events
                // still must prove complete cleanup in both independent replays.
                matches!(
                    self.checker
                        .typed
                        .expression_type(self.checker.unit_expression(id))
                        .and_then(|ty| self.checker.typed.types().get(ty)),
                    Some(UnitTypeKind::Builtin(
                        BuiltinType::Unit | BuiltinType::Nothing
                    ))
                )
            }
            Expression::Lambda { .. } => {
                self.owner_capture_supported(id)
                    && (self.owner_branch_depth == 0
                        || self.checker.captures_of(id).all(|capture| {
                            capture.borrow_kind()
                                == Some(crate::ownership_checking::LoanKind::Shared)
                        }))
            }
            Expression::Assignment { .. } => self.normal_assignment_supported(id),
            Expression::Binary { .. } => {
                self.entry_concat_operands(id).is_some()
                    || self.normal_control_expression_supported(id)
                    || self
                        .owner_loop
                        .private_counter_initializer(self.checker.unit_expression(id))
            }
            Expression::Literal(_) => self
                .checker
                .typed
                .expression_type(self.checker.unit_expression(id))
                .is_some_and(|ty| self.checker.typed.copyability(ty) == Copyability::Copyable),
            Expression::String { parts } => !parts
                .iter()
                .any(|part| matches!(part, StringPart::Interpolation { .. })),
            Expression::Call { .. } => {
                let supported = self.checker.ownership_primitives.iter().any(|plan| {
                    plan.descriptor().expression() == self.checker.unit_expression(id)
                        && match plan.descriptor().kind() {
                            OwnershipPrimitiveKind::Replace => plan.places().len() == 1,
                            OwnershipPrimitiveKind::Swap => plan.places().len() == 2,
                        }
                        && plan.descriptor().operands().len() == 2
                        && plan.places().iter().all(|place| place.is_root())
                        && self
                            .checker
                            .typed
                            .types()
                            .get(plan.descriptor().value_type())
                            == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                });
                if !supported {
                    self.reject_owner_expression(id, UnitOwnerUnsupportedReason::OpaqueCall);
                }
                supported
            }
            _ => false,
        };
        if !supported {
            self.reject_owner_expression(id, UnitOwnerUnsupportedReason::Syntax);
        }
        Ok(())
    }

    // The bool entrypoint preserves every legacy cleanup caller and continuation decision.
    pub(super) fn expression(
        &mut self,
        id: ExpressionId,
        usage: DropExpressionUse,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        Ok(self.evaluate_owner_expression(id, usage, state)?.continues)
    }

    pub(super) fn evaluate_owner_expression(
        &mut self,
        id: ExpressionId,
        usage: DropExpressionUse,
        state: &mut ValueState,
    ) -> Result<EvaluatedOwner, OwnershipCheckingError> {
        self.check_owner_expression(id)?;
        let mut owner = None;
        let mut replace_consumer = None;
        let continues =
            self.expression_inner(id, usage, state, &mut owner, &mut replace_consumer)?;
        self.normal_assignment_completed(id, continues);
        if let Some(choice) = &mut self.owner_choice {
            choice.expression_completed(
                self.checker.unit_expression(id),
                matches!(usage, DropExpressionUse::Consume),
                continues,
            );
        }
        let normal_binding_result =
            self.complete_normal_binding_result(id, usage, continues, owner.as_ref(), state);
        if !continues || (!self.owner_observation_active() && !normal_binding_result) {
            owner = None;
        }
        if let Some(owner) = owner.as_ref() {
            self.owner_event(UnitOwnerTransportEvent::Result {
                expression: self.checker.unit_expression(id),
                owner: owner.clone(),
            });
        }
        Ok(EvaluatedOwner {
            continues,
            owner,
            replace_consumer,
        })
    }

    // Independent of the legacy observation, which rejects the preceding String If.
    // Called only after the real checked primitive commit and root-origin update.
    pub(super) fn commit_choice_primitive(&mut self, call: ExpressionId, state: &ValueState) {
        if self.owner_choice.is_none() {
            return;
        }
        let plan = self.checker.ownership_primitives.iter().find(|plan| {
            plan.descriptor().expression() == self.checker.unit_expression(call)
                && plan.descriptor().kind() == OwnershipPrimitiveKind::Replace
                && plan.places().len() == 1
                && plan.places()[0].is_root()
                && plan.descriptor().operands().len() == 2
                && self
                    .checker
                    .typed
                    .types()
                    .get(plan.descriptor().value_type())
                    == Some(&UnitTypeKind::Builtin(BuiltinType::String))
        });
        let root = plan
            .map(|plan| plan.places()[0].root())
            .filter(|root| state.position(*root).is_some());
        let argument = plan.map(|plan| plan.descriptor().operands()[1]);
        let pending = argument.is_some_and(|argument| {
            self.pending_value_registered(call, argument.expression(), state)
        });
        if let Some(choice) = &mut self.owner_choice {
            choice.replacement_committed(
                self.checker.unit_expression(call),
                root,
                argument,
                pending,
            );
        }
    }

    pub(super) fn commit_owner_primitive(
        &mut self,
        call: ExpressionId,
        state: &mut ValueState,
    ) -> Option<UnitOwnerId> {
        self.record_root_primitive_write(call, state);
        if !self.owner_observation_active() {
            return None;
        }
        let plan =
            self.checker.ownership_primitives.iter().find(|plan| {
                plan.descriptor().expression() == self.checker.unit_expression(call)
            })?;
        if plan.descriptor().kind() == OwnershipPrimitiveKind::Swap {
            let left = plan.places().first()?.root();
            let right = plan.places().get(1)?.root();
            let owners = state
                .position(left)
                .zip(state.position(right))
                .and_then(|(l, r)| {
                    state.values[l]
                        .owner
                        .clone()
                        .zip(state.values[r].owner.clone())
                        .map(|(lo, ro)| (l, r, lo, ro))
                });
            let Some((l, r, left_owner, right_owner)) = owners else {
                self.reject_owner_expression(call, UnitOwnerUnsupportedReason::MissingOwner);
                return None;
            };
            state.values[l].owner = Some(right_owner.clone());
            state.values[r].owner = Some(left_owner.clone());
            self.owner_event(UnitOwnerTransportEvent::Swap {
                call: self.checker.unit_expression(call),
                left,
                right,
                left_owner,
                right_owner,
            });
            return None;
        }
        let root = plan.places().first()?.root();
        let argument = *plan.descriptor().operands().get(1)?;
        let root_index = state.position(root);
        let previous = root_index.and_then(|index| state.values[index].owner.clone());
        let incoming = self.take_pending_identity(call, argument.expression(), state);
        let (Some(index), Some(previous), Some(incoming)) = (root_index, previous, incoming) else {
            self.reject_owner_expression(call, UnitOwnerUnsupportedReason::MissingOwner);
            return None;
        };
        state.values[index].owner = Some(incoming.clone());
        self.owner_event(UnitOwnerTransportEvent::Replace {
            call: self.checker.unit_expression(call),
            root,
            argument,
            previous: previous.clone(),
            incoming,
        });
        Some(previous)
    }
}
