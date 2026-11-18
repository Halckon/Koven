//! Source-qualified unit expression drop planning in evaluation order.
use super::*;

impl<'a, 'checker> DropPlanner<'a, 'checker> {
    /// Keep the temporary handle through a field chain or its consuming shared call.
    fn read_temporary_rc_projection(
        &mut self,
        expression: ExpressionId,
        receiver: ExpressionId,
        owner: UnitExpressionId,
        usage: DropExpressionUse,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let continues = self.expression(receiver, DropExpressionUse::Place, state)?;
        if continues && usage != DropExpressionUse::Place {
            let origin = self
                .checker
                .parsed
                .ast()
                .expressions()
                .get(owner.expression())?
                .span();
            self.register_rc_projection_temporary(
                self.rc_projection_scope.unwrap_or(expression),
                owner.expression(),
                origin,
                state,
            );
        }
        Ok(continues)
    }

    pub(super) fn expression_inner(
        &mut self,
        id: ExpressionId,
        usage: DropExpressionUse,
        state: &mut ValueState,
        owner_result: &mut Option<crate::ownership_checking::UnitOwnerId>,
        replace_consumer: &mut Option<owner_transport::replace_consumer::ReplaceConsumerCarrier>,
    ) -> Result<bool, OwnershipCheckingError> {
        if self.checker.is_constant_use(id) {
            return Ok(true);
        }
        if let Some(operation) = self
            .checker
            .typed
            .integer_operation(self.checker.unit_expression(id))
        {
            return self.expression(
                operation.receiver().expression(),
                DropExpressionUse::Read,
                state,
            );
        }
        if let Some(receiver_id) = self
            .checker
            .typed
            .string_operation(self.checker.unit_expression(id))
            .map(|operation| operation.receiver())
            .or_else(|| {
                self.checker
                    .typed
                    .container_size(self.checker.unit_expression(id))
                    .map(|size| size.receiver())
            })
            .or_else(|| {
                self.checker
                    .typed
                    .map_size(self.checker.unit_expression(id))
                    .map(|size| size.receiver())
            })
        {
            let receiver = receiver_id.expression();
            if !self.expression(receiver, DropExpressionUse::Place, state)? {
                return Ok(false);
            }
            self.observe_string_clone_return(id);
            for loan in self.checker.loans.iter().filter(|loan| {
                loan.call() == self.checker.unit_expression(id) && loan.argument() == receiver_id
            }) {
                match loan.target() {
                    crate::ownership_checking::UnitLoanTarget::Place(place) => {
                        if !self.liveness.expression_after[id.index()].contains(&place.root()) {
                            self.drop_named_asap(
                                PlannerDropPoint::CallReturn(id),
                                place.root(),
                                state,
                            );
                        }
                    }
                    // A borrowed receiver binding does not introduce another owner.
                    crate::ownership_checking::UnitLoanTarget::This(_) => {}
                    crate::ownership_checking::UnitLoanTarget::Temporary(owner) => {
                        let owner = owner.expression();
                        self.push_fact(PlannerDropFact::new(
                            PlannerDropPoint::CallReturn(id),
                            PlannerDropTarget::Temporary(owner),
                            self.checker.parsed.ast().expressions().get(owner)?.span(),
                        ));
                    }
                }
            }
            return Ok(true);
        }
        if let Some(operation) = self
            .checker
            .typed
            .rc_operation(self.checker.unit_expression(id))
        {
            if operation.receiver().source_unit() != self.checker.source_unit {
                return Err(OwnershipCheckingError::InvalidUnitArgumentPlace {
                    source_unit: self.checker.source_unit.index(),
                    expression: id.index(),
                });
            }
            let receiver = operation.receiver().expression();
            if let Some(place) = self.checker.loan_place(receiver)? {
                let root = place.root();
                if !self.liveness.expression_after[id.index()].contains(&root) {
                    self.drop_named_asap(PlannerDropPoint::AfterExpression(id), root, state);
                }
                return Ok(true);
            }
            if let Some(owner) = self.checker.temporary_rc_payload_owner(id)? {
                return self.read_temporary_rc_projection(id, receiver, owner, usage, state);
            }
            if operation.kind() == crate::type_checking::RcOperationKind::Share
                && let Some(owner) = self
                    .checker
                    .temporary_expression_origin(receiver)?
                    .or(self.checker.temporary_rc_payload_owner(receiver)?)
            {
                let continues = self.expression(receiver, DropExpressionUse::Place, state)?;
                if continues {
                    let origin = self
                        .checker
                        .parsed
                        .ast()
                        .expressions()
                        .get(owner.expression())?
                        .span();
                    self.push_fact(PlannerDropFact::new(
                        PlannerDropPoint::AfterExpression(id),
                        PlannerDropTarget::Temporary(owner.expression()),
                        origin,
                    ));
                }
                return Ok(continues);
            }
            return self.expression(receiver, DropExpressionUse::Read, state);
        }
        if let Some(descriptor) = self
            .checker
            .construction_descriptors
            .get(&self.checker.unit_expression(id))
            .cloned()
        {
            return self.construction_operands(id, &descriptor, state);
        }
        if let Some(plan) = self.checker.short_circuit_plan(id)? {
            use super::super::super::constant::UnitShortCircuitRhs;
            if (!self.checker.constant_control || plan.rhs == UnitShortCircuitRhs::Conditional)
                && self
                    .checker
                    .typed
                    .expression_falls_through(self.checker.unit_expression(id))
                    != Some(false)
            {
                return self.short_circuit(id, plan, state);
            }
            if !self.expression(plan.left.expression(), DropExpressionUse::Read, state)? {
                return Ok(false);
            }
            return match plan.rhs {
                UnitShortCircuitRhs::Never => Ok(true),
                UnitShortCircuitRhs::Always => self.owner_short_circuit_rhs(plan, state),
                UnitShortCircuitRhs::Conditional => {
                    let mut skipped = state.clone();
                    self.drop_branch_exit(id, 1 - plan.rhs_branch, &mut skipped);
                    let mut branches = vec![skipped];
                    if self.owner_short_circuit_rhs(plan, state)? {
                        self.drop_branch_exit(id, plan.rhs_branch, state);
                        branches.push(state.clone());
                    }
                    *state = self.merge_resource_states(id, branches);
                    Ok(true)
                }
            };
        }
        let usage = if self
            .checker
            .borrow_return_origins
            .iter()
            .any(|fact| fact.expression() == self.checker.unit_expression(id))
            || self.checker.is_range_return_expression(id)
        {
            DropExpressionUse::Read
        } else {
            usage
        };
        let node = self.checker.parsed.ast().expressions().get(id)?;
        match node.payload().clone() {
            Expression::Error | Expression::Literal(_) | Expression::SuperMember { .. } => Ok(true),
            Expression::This => {
                if usage == DropExpressionUse::Consume {
                    state.this = None;
                }
                Ok(true)
            }
            Expression::Lambda { .. } => {
                *owner_result = self.form_capture_owner(id, usage, state);
                Ok(true)
            }
            Expression::Name => {
                if let Some(symbol) = self.checker.reference_symbol(node.span()) {
                    match usage {
                        DropExpressionUse::Consume => {
                            let value = self.take_named(id, symbol, state);
                            *replace_consumer = state.replace_consumer.take().and_then(|carrier| {
                                carrier.take(self.checker.unit_expression(id), value.as_ref())
                            });
                            *owner_result = self.moved_owner(id, value);
                        }
                        DropExpressionUse::Read => {
                            if !self.liveness.expression_after[id.index()].contains(&symbol) {
                                self.drop_named_asap(
                                    PlannerDropPoint::AfterExpression(id),
                                    symbol,
                                    state,
                                );
                            }
                        }
                        DropExpressionUse::Place => {}
                    }
                }
                Ok(true)
            }
            Expression::Group { expression } => {
                let result = self.evaluate_owner_expression(expression, usage, state)?;
                *owner_result = result.owner;
                Ok(result.continues)
            }
            Expression::String { parts } => {
                for part in parts {
                    if let StringPart::Interpolation { expression, .. } = part {
                        if !self.expression(expression, DropExpressionUse::Read, state)? {
                            state
                                .pending_temporaries
                                .retain(|pending| pending.control != id);
                            return Ok(false);
                        }
                        if self.is_move_only_temporary(expression) {
                            let origin = self
                                .checker
                                .parsed
                                .ast()
                                .expressions()
                                .get(expression)?
                                .span();
                            self.register_pending_temporary(id, expression, origin, false, state)?;
                        }
                    }
                }
                *owner_result = self.define_string_owner(id);
                self.replace_return_string(id, state);
                // Release only interpolation inputs; the outer String and live named owners remain.
                self.finish_pending_temporaries(id, PlannerDropPoint::AfterExpression(id), state);
                Ok(true)
            }
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.owner_if_expression(id, condition, then_branch, else_branch, state),
            Expression::When {
                subject, entries, ..
            } => self.when_expression(id, subject, &entries, state),
            Expression::Return { value, .. } => self.owner_return_expression(id, value, state),
            jump @ (Expression::Break { .. } | Expression::Continue { .. }) => self
                .owner_loop_jump_expression(id, matches!(jump, Expression::Continue { .. }), state),
            Expression::NonNullAssert { operand, .. } => {
                self.expression(operand, DropExpressionUse::Consume, state)
            }
            Expression::Prefix { operand, .. }
            | Expression::Cast {
                expression: operand,
                ..
            }
            | Expression::TypeTest {
                expression: operand,
                ..
            } => self.expression(operand, DropExpressionUse::Read, state),
            Expression::Propagate { value, .. } => {
                self.expression(value, DropExpressionUse::Read, state)?;
                for owned in state.values.iter().rev() {
                    self.push_fact(PlannerDropFact::new(
                        PlannerDropPoint::ControlTransfer(id),
                        PlannerDropTarget::Named(owned.symbol),
                        owned.origin,
                    ));
                }
                if let Some(receiver) = state.this {
                    self.push_this_fact(PlannerDropPoint::ControlTransfer(id), receiver);
                }
                Ok(true)
            }
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } if self
                .checker
                .uses_string_binary_operands(left, operator, right) =>
            {
                let continues = self.string_binary(left, right, id, state)?;
                if continues {
                    *owner_result = self.completed_entry_concat(id);
                }
                Ok(continues)
            }
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } => {
                if !self.expression(left, DropExpressionUse::Read, state)?
                    || !self.expression(right, DropExpressionUse::Read, state)?
                {
                    return Ok(false);
                }
                self.drop_nullable_comparison_temporary(id, left, operator, right)?;
                Ok(true)
            }
            Expression::Assignment {
                target,
                operator,
                value,
                ..
            } => self.assignment(id, target, operator, value, state),
            Expression::Member { receiver, .. } => {
                if let Some(place) = self.checker.place(id)? {
                    let root = place.root();
                    if usage == DropExpressionUse::Consume && place.is_root() {
                        let _ = self.take_named(id, root, state);
                    } else if usage != DropExpressionUse::Place
                        && !self.liveness.expression_after[id.index()].contains(&root)
                    {
                        self.drop_named_asap(PlannerDropPoint::AfterExpression(id), root, state);
                    }
                    Ok(true)
                } else if let Some(owner) = self.checker.temporary_rc_payload_owner(id)? {
                    self.read_temporary_rc_projection(id, receiver, owner, usage, state)
                } else {
                    let receiver_use = if usage == DropExpressionUse::Place {
                        DropExpressionUse::Place
                    } else {
                        DropExpressionUse::Read
                    };
                    self.expression(receiver, receiver_use, state)
                }
            }
            Expression::Call {
                callee, arguments, ..
            } => {
                let loan_region = self.enter_loan_region(id, state);
                let returns = !self
                    .checker
                    .is_nothing_expression(self.checker.unit_expression(id));
                let receiver_contract = self
                    .checker
                    .receiver_contracts_by_call
                    .get(&self.checker.unit_expression(id))
                    .copied();
                let contracts = self
                    .checker
                    .contracts_by_call
                    .get(&self.checker.unit_expression(id));
                let mut borrowed_roots = Vec::new();
                if let Some(contract) = receiver_contract {
                    match contract.source() {
                        UnitCallReceiverOrigin::Expression(receiver) => {
                            let receiver = receiver.expression();
                            match contract.kind() {
                                UnitCallArgumentOwnershipKind::Value => {
                                    let conditional_static_self =
                                        self.checker.expression_is_this(receiver)?
                                            && matches!(
                                                self.checker
                                                    .typed
                                                    .types()
                                                    .get(contract.receiver_type()),
                                                Some(UnitTypeKind::StaticSelf(_))
                                            );
                                    let usage = if conditional_static_self {
                                        DropExpressionUse::Place
                                    } else {
                                        DropExpressionUse::Consume
                                    };
                                    if !self.expression(receiver, usage, state)? {
                                        return Ok(false);
                                    }
                                    if conditional_static_self {
                                        self.register_pending_this(id, state);
                                    } else {
                                        self.register_value_argument(id, receiver, state)?;
                                    }
                                }
                                UnitCallArgumentOwnershipKind::SharedLoan
                                | UnitCallArgumentOwnershipKind::ExclusiveLoan => {
                                    if let Some(place) = self.checker.place(receiver)? {
                                        if !self.expression(
                                            receiver,
                                            DropExpressionUse::Place,
                                            state,
                                        )? {
                                            return Ok(false);
                                        }
                                        if state.position(place.root()).is_some() {
                                            borrowed_roots.push(place.root());
                                            state.pending_borrows.push((id, place.root()));
                                            self.hold_pending_capture_borrows(
                                                id,
                                                receiver,
                                                Some(place.root()),
                                                state,
                                            );
                                        }
                                    } else {
                                        if !self.expression(
                                            receiver,
                                            DropExpressionUse::Place,
                                            state,
                                        )? {
                                            return Ok(false);
                                        }
                                        if contract.kind()
                                            == UnitCallArgumentOwnershipKind::SharedLoan
                                            && let Some(temporary) = self
                                                .checker
                                                .temporary_expression_origin(receiver)?
                                                .map(UnitExpressionId::expression)
                                                .filter(|temporary| {
                                                    self.is_move_only_temporary(*temporary)
                                                })
                                        {
                                            let origin = self
                                                .checker
                                                .parsed
                                                .ast()
                                                .expressions()
                                                .get(temporary)?
                                                .span();
                                            self.hold_pending_capture_borrows(
                                                id, receiver, None, state,
                                            );
                                            self.register_pending_temporary(
                                                id, temporary, origin, false, state,
                                            )?;
                                        }
                                    }
                                }
                            }
                        }
                        UnitCallReceiverOrigin::ImplicitThis(_) => {
                            if contract.kind() == UnitCallArgumentOwnershipKind::Value {
                                self.register_pending_this(id, state);
                            }
                        }
                    }
                    self.register_iteration_receiver_loan(id, state);
                } else if self
                    .checker
                    .typed
                    .call(self.checker.unit_expression(id))
                    .is_some_and(|call| {
                        call.target() == crate::type_checking::UnitCallTarget::FunctionValue
                    })
                    && let Some(place) = self.checker.place(callee)?
                {
                    if !self.expression(callee, DropExpressionUse::Place, state)? {
                        return Ok(false);
                    }
                    // callee 的最后一次源码读取先于实参，但调用完成前仍需保留其 owner/capture。
                    if state.position(place.root()).is_some() {
                        borrowed_roots.push(place.root());
                        state.pending_borrows.push((id, place.root()));
                        self.hold_pending_capture_borrows(id, callee, Some(place.root()), state);
                    }
                } else {
                    if !self.expression(callee, DropExpressionUse::Read, state)? {
                        return Ok(false);
                    }
                    if self.capture_sources_at_call(id, callee).is_some() {
                        self.register_borrow_temporary_argument(
                            id,
                            callee,
                            UnitCallArgumentOwnershipKind::SharedLoan,
                            state,
                        )?;
                    }
                }
                for (index, argument) in arguments.into_iter().enumerate() {
                    let kind = contracts
                        .and_then(|contracts| contracts.get(index))
                        .map(|contract| contract.kind())
                        .unwrap_or(UnitCallArgumentOwnershipKind::SharedLoan);
                    match kind {
                        UnitCallArgumentOwnershipKind::Value => {
                            let result = self.evaluate_owner_expression(
                                argument.value,
                                DropExpressionUse::Consume,
                                state,
                            )?;
                            if !result.continues {
                                return Ok(false);
                            }
                            self.register_value_argument(id, argument.value, state)?;
                            self.register_pending_identity(id, argument.value, result.owner, state);
                            self.register_replace_call_consumer(
                                id,
                                argument.value,
                                result.replace_consumer,
                                state,
                            );
                        }
                        UnitCallArgumentOwnershipKind::SharedLoan
                        | UnitCallArgumentOwnershipKind::ExclusiveLoan => {
                            // Rc operations can schedule the final handle release during
                            // operand evaluation. A borrowed payload keeps it through the call.
                            if kind == UnitCallArgumentOwnershipKind::SharedLoan
                                && contracts.and_then(|items| items.get(index)).is_some_and(
                                    |contract| !self.checker.copy_to_nullable_borrow(*contract),
                                )
                                && let Some(receiver) =
                                    self.checker.rc_value_receiver(argument.value)?
                                && let Some(place) = self.checker.place(receiver)?
                                && state.position(place.root()).is_some()
                                && !borrowed_roots.contains(&place.root())
                            {
                                borrowed_roots.push(place.root());
                                state.pending_borrows.push((id, place.root()));
                            }
                            if let Some(place) = self.checker.place(argument.value)? {
                                let root = place.root();
                                if !self.expression(
                                    argument.value,
                                    DropExpressionUse::Place,
                                    state,
                                )? {
                                    return Ok(false);
                                }
                                if state.position(root).is_some() && !borrowed_roots.contains(&root)
                                {
                                    borrowed_roots.push(root);
                                    state.pending_borrows.push((id, root));
                                    self.hold_pending_capture_borrows(
                                        id,
                                        argument.value,
                                        Some(root),
                                        state,
                                    );
                                }
                            } else {
                                if !self.expression(
                                    argument.value,
                                    DropExpressionUse::Place,
                                    state,
                                )? {
                                    return Ok(false);
                                }
                                self.register_borrow_temporary_argument(
                                    id,
                                    argument.value,
                                    kind,
                                    state,
                                )?;
                            }
                        }
                    }
                    if kind != UnitCallArgumentOwnershipKind::Value {
                        self.register_iteration_pending_loans(id, argument.value, state);
                    }
                }
                if !returns {
                    return Ok(false);
                }
                if let Some(plan) = self
                    .checker
                    .ownership_primitives
                    .iter()
                    .find(|plan| plan.descriptor().expression() == self.checker.unit_expression(id))
                    .cloned()
                {
                    *owner_result = self.commit_owner_primitive(id, state);
                    // Both roots remain initialized. Their old owners are returned/swapped,
                    // never destroyed by assignment replacement glue.
                    for place in plan.places() {
                        if let Some(index) = state.position(place.root()) {
                            state.values[index].origin = node.span();
                            #[cfg(any(test, feature = "d10-catalog-experiment"))]
                            self.origin_applied(id, place.root());
                        }
                    }
                    self.commit_choice_primitive(id, state);
                    self.commit_replace_return(id, state);
                }
                let copied_source = self.continue_borrow_call_source(id, state);
                self.finish_call_borrows(id, &mut borrowed_roots, state);
                self.finish_direct_call_loans(id, state);
                self.observe_borrow_call_handoff(id, state);
                self.transport_borrow_call_source(id, state);
                self.transport_borrow_binding_source(id, state);
                self.transport_borrow_forward_source(id, state);
                self.finish_borrow_call_transports(id, state);
                self.finish_call_pending_loans(id, state);
                self.continue_range_source(id, state);
                // Closure cleanup can recursively try to drop the borrowed source.
                self.call_return_copy_source = copied_source.map(|(root, _)| root);
                self.finish_pending_temporaries(id, PlannerDropPoint::CallReturn(id), state);
                let mut copied_drop = None;
                for root in borrowed_roots {
                    if !self.liveness.expression_after[id.index()].contains(&root) {
                        if let Some(copy) = copied_source.filter(|(source, _)| *source == root) {
                            copied_drop = Some(copy);
                        } else {
                            self.drop_named_asap(PlannerDropPoint::CallReturn(id), root, state);
                        }
                    }
                }
                self.call_return_copy_source = None;
                if let Some((root, point)) = copied_drop {
                    self.drop_named_asap(point, root, state);
                }
                self.iteration_actions.complete_region(loan_region);
                *replace_consumer = self.finish_replace_return_call(id, state);
                Ok(true)
            }
            Expression::Index { receiver, index } => {
                if self
                    .checker
                    .typed
                    .map_get(self.checker.unit_expression(id))
                    .is_some()
                {
                    return self.map_subscript(id, state);
                }
                if !self.expression(receiver, DropExpressionUse::Place, state)? {
                    return Ok(false);
                }
                // The backing owner already exists if evaluating the index exits early.
                if let Some(owner) = self.checker.temporary_element_owner(id)? {
                    let origin = self
                        .checker
                        .parsed
                        .ast()
                        .expressions()
                        .get(owner.expression())?
                        .span();
                    self.register_pending_temporary(id, owner.expression(), origin, false, state)?;
                }
                if !self.expression(index, DropExpressionUse::Read, state)? {
                    return Ok(false);
                }
                state
                    .pending_temporaries
                    .retain(|pending| pending.control != id);
                if usage != DropExpressionUse::Place
                    && let Some(temporary) = self
                        .checker
                        .temporary_element_owner(id)?
                        .map(UnitExpressionId::expression)
                {
                    self.push_fact(PlannerDropFact::new(
                        PlannerDropPoint::AfterExpression(id),
                        PlannerDropTarget::Temporary(temporary),
                        self.checker
                            .parsed
                            .ast()
                            .expressions()
                            .get(temporary)?
                            .span(),
                    ));
                } else if usage != DropExpressionUse::Place
                    && let Some(place) = self.checker.place(id)?
                    && !self.liveness.expression_after[id.index()].contains(&place.root())
                {
                    self.drop_named_asap(
                        PlannerDropPoint::AfterExpression(id),
                        place.root(),
                        state,
                    );
                }
                Ok(true)
            }
            Expression::CallableReference { receiver, .. } => {
                if let Some(receiver) = receiver {
                    self.expression(receiver, DropExpressionUse::Read, state)
                } else {
                    Ok(true)
                }
            }
        }
    }
}
