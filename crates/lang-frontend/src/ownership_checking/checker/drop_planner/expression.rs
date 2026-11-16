//! Single-file expression drop planning in source evaluation order.
use super::*;

impl<'a, 'checker> DropPlanner<'a, 'checker> {
    pub(super) fn expression_inner(
        &mut self,
        id: ExpressionId,
        usage: ExpressionUse,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        if self.checker.is_constant_use(id) {
            return Ok(true);
        }
        if let Some(operation) = self.checker.typed.integer_operation(id) {
            return self.expression(operation.receiver(), ExpressionUse::Read, state);
        }
        if let Some(receiver) = self
            .checker
            .typed
            .string_operation(id)
            .map(|operation| operation.receiver())
            .or_else(|| {
                self.checker
                    .typed
                    .container_size(id)
                    .map(|size| size.receiver())
            })
            .or_else(|| self.checker.typed.map_size(id).map(|size| size.receiver()))
        {
            state.pending_calls.push(pending_call::PendingCall::new(
                id,
                self.loop_boundaries.len(),
            ));
            if !self.expression(receiver, ExpressionUse::Place, state)? {
                return Ok(false);
            }
            self.register_pending_argument(id, receiver, ParameterMode::Borrow, state)?;
            let roots = self.end_pending_calls(LoanEndPoint::CallReturn(id), state, |frame| {
                frame.call == id
            });
            self.drop_nullable_temporaries(DropPoint::CallReturn(id), state, |temporary| {
                temporary.control == id
            });
            for root in roots {
                if !self.liveness.expression_after[id.index()].contains(&root) {
                    self.drop_named_asap(DropPoint::CallReturn(id), root, state);
                }
            }
            return Ok(true);
        }
        if let Some(operation) = self.checker.typed.rc_operation(id) {
            if let Some(place) = self.checker.place(operation.receiver())? {
                let root = place.root();
                if !self.liveness.expression_after[id.index()].contains(&root) {
                    self.drop_named_asap(DropPoint::AfterExpression(id), root, state);
                }
                return Ok(true);
            }
            return self.expression(operation.receiver(), ExpressionUse::Read, state);
        }
        if let Some(descriptor) = self
            .checker
            .construction
            .descriptors
            .get(&id.index())
            .cloned()
        {
            let mut arguments = descriptor.arguments().to_vec();
            arguments.sort_by_key(|argument| argument.evaluation_index());
            for argument in arguments {
                if !self.expression(argument.argument(), ExpressionUse::Consume, state)?
                    || self.checker.is_nothing_expression(argument.argument())
                {
                    return Ok(false);
                }
                self.register_pending_argument(
                    id,
                    argument.argument(),
                    ParameterMode::Value,
                    state,
                )?;
            }
            // A completed construction takes each pending Value input; an earlier control
            // transfer instead consumes these obligations through drop_all.
            state
                .nullable_temporaries
                .retain(|temporary| temporary.control != id || !temporary.transfers_at_call);
            return Ok(true);
        }
        let node = self.checker.parsed.ast().expressions().get(id)?;
        match node.payload().clone() {
            Expression::Error
            | Expression::This
            | Expression::Literal(_)
            | Expression::SuperMember { .. } => Ok(true),
            Expression::Lambda { .. } => {
                for capture in self.checker.captures_of(id) {
                    if capture.mode() == ClosureCaptureMode::Owned
                        && capture.effect() == ClosureCaptureEffect::Move
                        && let ClosureCaptureSource::Symbol(symbol) = capture.source()
                    {
                        self.take_named(id, symbol, state);
                    }
                }
                Ok(true)
            }
            Expression::Name => {
                if let Some(symbol) = self.checker.reference_symbol(node.span()) {
                    match usage {
                        ExpressionUse::Consume => {
                            self.take_named(id, symbol, state);
                        }
                        ExpressionUse::Read => {
                            if !self.liveness.expression_after[id.index()].contains(&symbol) {
                                self.drop_named_asap(DropPoint::AfterExpression(id), symbol, state);
                            }
                        }
                        ExpressionUse::Place => {}
                    }
                }
                Ok(true)
            }
            Expression::Group { expression } => self.expression(expression, usage, state),
            Expression::String { parts } => {
                for part in parts {
                    if let StringPart::Interpolation { expression, .. } = part {
                        if !self.expression(expression, ExpressionUse::Read, state)? {
                            state
                                .nullable_temporaries
                                .retain(|temporary| temporary.control != id);
                            return Ok(false);
                        }
                        if self.is_move_only_temporary(expression) {
                            state.nullable_temporaries.push(NullableTemporary {
                                versions: Vec::new(),
                                closures: Vec::new(),
                                transfers_at_call: false,
                                control: id,
                                subject: expression,
                                origin: self
                                    .checker
                                    .parsed
                                    .ast()
                                    .expressions()
                                    .get(expression)?
                                    .span(),
                                loop_depth: self.loop_boundaries.len(),
                                prior_symbols: state
                                    .values
                                    .iter()
                                    .map(|value| value.symbol)
                                    .collect(),
                            });
                        }
                    }
                }
                // Interpolation has consumed its inputs; the outer String result remains owned.
                // This completion must not release named owners that are live after the String.
                for index in (0..state.nullable_temporaries.len()).rev() {
                    if state.nullable_temporaries[index].control == id {
                        let temporary = state.nullable_temporaries.remove(index);
                        self.push_fact(DropFact::new(
                            DropPoint::AfterExpression(id),
                            DropTarget::Temporary(temporary.subject),
                            temporary.origin,
                        ));
                    }
                }
                Ok(true)
            }
            Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                if !self.expression(condition, ExpressionUse::Read, state)? {
                    return Ok(false);
                }
                let base = state.clone();
                let mut branch_states = Vec::new();
                let mut then_state = base.clone();
                self.enter_branch(id, 2, 0, &mut then_state)?;
                let usage = self.checker.control_result_usage(id);
                if self.control_body(then_branch, usage, &mut then_state)? {
                    self.drop_branch_exit(id, 0, &mut then_state);
                    branch_states.push(then_state);
                }
                if let Some(else_branch) = else_branch {
                    let mut else_state = base;
                    self.enter_branch(id, 2, 1, &mut else_state)?;
                    if self.control_body(else_branch, usage, &mut else_state)? {
                        self.drop_branch_exit(id, 1, &mut else_state);
                        branch_states.push(else_state);
                    }
                } else {
                    let mut implicit = base;
                    self.enter_branch(id, 2, 1, &mut implicit)?;
                    self.drop_branch_exit(id, 1, &mut implicit);
                    branch_states.push(implicit);
                }
                let continues = !branch_states.is_empty();
                *state = self.merge_value_states(branch_states);
                Ok(continues)
            }
            Expression::When {
                subject, entries, ..
            } => {
                if let Some(subject) = subject
                    && !self.expression(subject, ExpressionUse::Read, state)?
                {
                    return Ok(false);
                }
                let plan = self.checker.typed.nullable_when(id).cloned();
                if let Some(plan) = &plan
                    && plan.category()
                        == crate::type_checking::NullableWhenSubjectCategory::Temporary
                    && self.is_move_only_temporary(plan.subject())
                {
                    state.nullable_temporaries.push(NullableTemporary {
                        versions: Vec::new(),
                        closures: Vec::new(),
                        transfers_at_call: false,
                        control: id,
                        subject: plan.subject(),
                        origin: self
                            .checker
                            .parsed
                            .ast()
                            .expressions()
                            .get(plan.subject())?
                            .span(),
                        loop_depth: self.loop_boundaries.len(),
                        prior_symbols: state.values.iter().map(|value| value.symbol).collect(),
                    });
                }
                let mut remaining = Some(state.clone());
                let mut branch_states = Vec::new();
                for (index, entry) in entries.iter().enumerate() {
                    let descriptor = plan.as_ref().and_then(|plan| plan.entries().get(index));
                    let mut matched = Vec::new();
                    let mut matched_alternatives = Vec::new();
                    if entry.else_span.is_some() {
                        matched.extend(remaining.take());
                    }
                    for (alternative_index, condition) in entry.conditions.iter().enumerate() {
                        let Some(mut input) = remaining.take() else {
                            break;
                        };
                        if let WhenCondition::Expression(expression)
                        | WhenCondition::Contains { expression, .. } = condition
                            && !self.expression(*expression, ExpressionUse::Read, &mut input)?
                        {
                            break;
                        }
                        let alternative = descriptor
                            .and_then(|entry| entry.alternatives().get(alternative_index));
                        let can_match = alternative
                            .is_none_or(|alternative| !alternative.match_domain().is_empty());
                        let can_continue = alternative
                            .is_none_or(|alternative| !alternative.fallthrough_domain().is_empty());
                        if can_match {
                            let mut success = input.clone();
                            if can_continue {
                                self.enter_when_alternative(
                                    id,
                                    index,
                                    alternative_index,
                                    true,
                                    &mut success,
                                )?;
                            }
                            matched.push(success);
                            matched_alternatives.push(alternative_index);
                        }
                        if can_continue {
                            if can_match {
                                self.enter_when_alternative(
                                    id,
                                    index,
                                    alternative_index,
                                    false,
                                    &mut input,
                                )?;
                            }
                            remaining = Some(input);
                        }
                    }
                    if matched.is_empty() {
                        continue;
                    }
                    // A successful ownership check guarantees that an owner missing on
                    // another incoming edge cannot be read by the shared body. Release
                    // each remaining obligation on its own edge before intersecting states.
                    let common = matched[0]
                        .values
                        .iter()
                        .filter(|value| {
                            matched
                                .iter()
                                .all(|incoming| incoming.position(value.symbol).is_some())
                        })
                        .map(|value| value.symbol)
                        .collect::<std::collections::BTreeSet<_>>();
                    for (alternative, incoming) in
                        matched_alternatives.into_iter().zip(&mut matched)
                    {
                        let exclusive = incoming
                            .values
                            .iter()
                            .rev()
                            .filter(|value| !common.contains(&value.symbol))
                            .map(|value| value.symbol)
                            .collect::<Vec<_>>();
                        for symbol in exclusive {
                            self.drop_named_asap(
                                DropPoint::WhenAlternativeMatch {
                                    control: id,
                                    entry: index,
                                    alternative,
                                },
                                symbol,
                                incoming,
                            );
                        }
                    }
                    let mut branch = self.merge_value_states(matched);
                    if self.control_body(
                        entry.body,
                        self.checker.control_result_usage(id),
                        &mut branch,
                    )? {
                        self.drop_branch_exit(id, index, &mut branch);
                        branch_states.push(branch);
                    }
                }
                if let Some(mut implicit) = remaining {
                    self.drop_branch_exit(id, entries.len(), &mut implicit);
                    branch_states.push(implicit);
                }
                let continues = !branch_states.is_empty();
                *state = self.merge_value_states(branch_states);
                Ok(continues)
            }
            Expression::Return { value, .. } => {
                if let Some(value) = value
                    && !self.expression(value, ExpressionUse::Consume, state)?
                {
                    return Ok(false);
                }
                self.drop_all(DropPoint::ControlTransfer(id), state);
                Ok(false)
            }
            Expression::Break { .. } | Expression::Continue { .. } => {
                let boundary = self.loop_boundaries.last().copied().unwrap_or(0);
                self.drop_deeper_than(
                    boundary,
                    self.loop_boundaries.len(),
                    DropPoint::ControlTransfer(id),
                    state,
                );
                if state
                    .iterations
                    .last()
                    .is_some_and(|frame| frame.loop_depth == self.loop_boundaries.len())
                {
                    if matches!(node.payload(), Expression::Break { .. }) {
                        self.finish_iteration(
                            DropPoint::ControlTransfer(id),
                            IterationExitKind::Break(id),
                            state,
                        );
                    } else {
                        self.end_iteration_element(
                            DropPoint::ControlTransfer(id),
                            IterationExitKind::Continue(id),
                            state,
                        );
                    }
                }
                Ok(false)
            }
            Expression::NonNullAssert { operand, .. } => {
                self.expression(operand, ExpressionUse::Consume, state)
            }
            Expression::Prefix { operand, .. }
            | Expression::Cast {
                expression: operand,
                ..
            }
            | Expression::TypeTest {
                expression: operand,
                ..
            } => self.expression(operand, ExpressionUse::Read, state),
            Expression::Propagate { value, .. } => {
                self.expression(value, ExpressionUse::Read, state)?;
                // Err 是独立退出边；正常路径继续持有原状态。
                self.drop_all(DropPoint::ControlTransfer(id), &mut state.clone());
                Ok(true)
            }
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } if self.uses_string_binary_operands(left, operator, right) => {
                self.string_binary(left, right, id, state)
            }
            Expression::Binary {
                left,
                operator: BinaryOperator::Elvis,
                right,
                ..
            } => self.elvis(id, left, right, state),
            Expression::Binary {
                left,
                operator: BinaryOperator::LogicalAnd | BinaryOperator::LogicalOr,
                right,
                ..
            } => self.short_circuit(id, left, right, state),
            Expression::Binary {
                left,
                operator,
                right,
                ..
            } => {
                if !self.expression(left, ExpressionUse::Read, state)?
                    || !self.expression(right, ExpressionUse::Read, state)?
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
            } => {
                if self.checker.typed.map_put(target).is_some() {
                    return self.map_assignment(id, target, state);
                }
                if let Some(descriptor) = self.checker.element_place_descriptor(target)? {
                    self.expression(descriptor.receiver(), ExpressionUse::Place, state)?;
                    self.expression(descriptor.index(), ExpressionUse::Read, state)?;
                    if !self.expression(value, ExpressionUse::Consume, state)? {
                        return Ok(false);
                    }
                    if self.checker.typed.copyability(descriptor.element_type())
                        == Some(Copyability::MoveOnly)
                    {
                        self.push_fact(DropFact::new(
                            DropPoint::AfterReplacement(id),
                            DropTarget::ReplacedElement(id),
                            self.checker.parsed.ast().expressions().get(target)?.span(),
                        ));
                    }
                    if let Some(temporary) = self.checker.temporary_element_owner(target)? {
                        self.push_fact(DropFact::new(
                            DropPoint::AfterExpression(id),
                            DropTarget::Temporary(temporary),
                            self.checker
                                .parsed
                                .ast()
                                .expressions()
                                .get(temporary)?
                                .span(),
                        ));
                    } else if let Some(place) = self.checker.place(target)?
                        && !self.liveness.expression_after[id.index()].contains(&place.root())
                    {
                        self.drop_named_asap(DropPoint::AfterExpression(id), place.root(), state);
                    }
                    return Ok(true);
                }
                let root = self
                    .checker
                    .place(target)?
                    .filter(|place| place.is_root())
                    .map(|place| place.root());
                if let Some(root) = root {
                    state.replacements.push(root);
                }
                let continues = self.expression(value, ExpressionUse::Consume, state)?;
                if root.is_some() {
                    state.replacements.pop();
                }
                if !continues {
                    return Ok(false);
                }
                if let Some(place) = self.checker.place(target)?
                    && place.is_root()
                {
                    let symbol = place.root();
                    let snapshot = if self.checker.is_move_only_variable(symbol) {
                        self.save_result_snapshot(value, state)?
                    } else {
                        None
                    };
                    if operator != AssignmentOperator::Assign {
                        self.expression(target, ExpressionUse::Read, state)?;
                    }
                    let old = state.remove_value(symbol);
                    let previous = state.closures.remove(&symbol).unwrap_or_default();
                    if let Some(old) = &old {
                        // The RHS result still holds its captures while the old environment drops.
                        self.drop_closure_owner(
                            DropFact::new(
                                DropPoint::AfterExpression(value),
                                DropTarget::Named(symbol),
                                old.origin,
                            ),
                            old.condition,
                            &old.versions,
                            previous,
                            state,
                        );
                    }
                    let closures = std::mem::take(&mut state.result_closures);
                    if self.checker.is_move_only_variable(symbol) {
                        let versions = self.bind_result_owners(node.span(), state);
                        state.insert(OwnedValue {
                            versions,
                            condition: state.path,
                            symbol,
                            origin: node.span(),
                            declaration: self.checker.names.symbols()[symbol.index()].span(),
                            scope_depth: old
                                .map(|value| value.scope_depth)
                                .or_else(|| self.binding_depths.get(&symbol).copied())
                                .unwrap_or(self.scope_depth),
                        });
                        if !closures.is_empty() {
                            state.closures.insert(symbol, closures);
                        }
                        self.commit_snapshot(snapshot, value, symbol);
                        if !self.liveness.expression_after[id.index()].contains(&symbol) {
                            self.drop_named_asap(DropPoint::AfterExpression(id), symbol, state);
                        }
                    }
                }
                Ok(true)
            }
            Expression::Member { receiver, .. } => {
                if let Some(place) = self.checker.place(id)? {
                    let root = place.root();
                    if usage == ExpressionUse::Consume && place.is_root() {
                        self.take_named(id, root, state);
                    } else if usage != ExpressionUse::Place
                        && !self.liveness.expression_after[id.index()].contains(&root)
                    {
                        self.drop_named_asap(DropPoint::AfterExpression(id), root, state);
                    }
                    Ok(true)
                } else {
                    self.expression(receiver, ExpressionUse::Read, state)
                }
            }
            Expression::Call {
                callee, arguments, ..
            } => {
                state.pending_calls.push(pending_call::PendingCall::new(
                    id,
                    self.loop_boundaries.len(),
                ));
                let modes = self.checker.calls_by_expression.get(&id.index()).cloned();
                match self
                    .checker
                    .receivers_by_expression
                    .get(&id.index())
                    .copied()
                {
                    Some(receiver) => {
                        if let CallReceiverOrigin::Expression(expression) = receiver.origin() {
                            let usage = if receiver.mode() == ParameterMode::Value {
                                ExpressionUse::Consume
                            } else {
                                ExpressionUse::Place
                            };
                            if !self.expression(expression, usage, state)? {
                                return Ok(false);
                            }
                            self.register_pending_argument(id, expression, receiver.mode(), state)?;
                        } else {
                            // Implicit this has no operand evaluation, but its loan still ends
                            // on normal call return or on an argument's control-transfer edge.
                            self.register_pending_argument(id, callee, receiver.mode(), state)?;
                        }
                    }
                    None => {
                        if !self.expression(callee, ExpressionUse::Place, state)? {
                            return Ok(false);
                        }
                        self.register_pending_callee(id, callee, state)?;
                    }
                }
                for (index, argument) in arguments.into_iter().enumerate() {
                    let mode = modes
                        .as_ref()
                        .and_then(|modes| modes.get(index))
                        .copied()
                        .unwrap_or(ParameterMode::Borrow);
                    let usage = if mode == ParameterMode::Value {
                        ExpressionUse::Consume
                    } else {
                        ExpressionUse::Place
                    };
                    if !self.expression(argument.value, usage, state)? {
                        return Ok(false);
                    }
                    self.register_pending_argument(id, argument.value, mode, state)?;
                }
                if let Some((callee, closure)) = state
                    .pending_calls
                    .iter()
                    .rev()
                    .find(|frame| frame.call == id)
                    .and_then(|frame| {
                        let environment = frame.closure_environment?;
                        let root = *frame.callees.first()?;
                        state
                            .values
                            .iter()
                            .find(|value| value.symbol == root)
                            .filter(|value| {
                                self.conditions.and(state.path, value.condition) == state.path
                                    && value.versions.iter().any(|version| {
                                        version.owner == environment.0
                                            && self.conditions.and(state.path, version.condition)
                                                == state.path
                                    })
                            })
                            .map(|_| environment)
                    })
                {
                    self.cleanup.push((
                        DropPoint::CallEntry(id),
                        IterationCleanupAction::PassClosureEnvironment { callee, closure },
                    ));
                }
                if self.checker.is_nothing_expression(id) {
                    return Ok(false);
                }
                self.commit_ownership_primitive(id, state)?;
                self.continue_range_temporary(id, state);
                let roots = self
                    .end_pending_calls(LoanEndPoint::CallReturn(id), state, |call| call.call == id);
                let copied_source = self
                    .checker
                    .call_descriptors
                    .get(&id.index())
                    .filter(|call| call.borrow_return().is_some())
                    .and_then(|_| {
                        self.checker
                            .borrow_results
                            .call_uses
                            .iter()
                            .find_map(|fact| {
                                if fact.expression() == id
                                    && fact.end_point()
                                        == DropPoint::AfterExpression(fact.argument())
                                    && let LoanTarget::Place(place) = fact.origin()
                                {
                                    Some((place.root(), fact.end_point()))
                                } else {
                                    None
                                }
                            })
                    });
                // Closure cleanup can recursively try to drop the borrowed source.
                self.call_return_copy_source = copied_source.map(|(root, _)| root);
                // Value parameters now own their arguments; only borrowed temporaries expire here.
                state
                    .nullable_temporaries
                    .retain(|temporary| temporary.control != id || !temporary.transfers_at_call);
                self.drop_nullable_temporaries(DropPoint::CallReturn(id), state, |temporary| {
                    temporary.control == id
                });
                let mut copied_drop = None;
                for root in roots {
                    if !self.liveness.expression_after[id.index()].contains(&root) {
                        if let Some(copy) = copied_source.filter(|(source, _)| *source == root) {
                            copied_drop = Some(copy);
                        } else {
                            self.drop_named_asap(DropPoint::CallReturn(id), root, state);
                        }
                    }
                }
                self.call_return_copy_source = None;
                if let Some((root, point)) = copied_drop {
                    self.drop_named_asap(point, root, state);
                }
                Ok(true)
            }
            Expression::Index { receiver, index } => {
                if self.checker.typed.map_get(id).is_some() {
                    return self.map_subscript(id, state);
                }
                if !self.expression(receiver, ExpressionUse::Place, state)? {
                    return Ok(false);
                }
                // Index 求值可能 return/abort；此前已求值的 backing owner 仍有清理义务。
                if let Some(owner) = self.checker.temporary_element_owner(id)? {
                    let origin = self.checker.parsed.ast().expressions().get(owner)?.span();
                    let version = self.temporary_owner_version(owner, origin, state);
                    state.nullable_temporaries.push(NullableTemporary {
                        versions: vec![version],
                        closures: Vec::new(),
                        transfers_at_call: false,
                        control: id,
                        subject: owner,
                        origin,
                        loop_depth: self.loop_boundaries.len(),
                        prior_symbols: state.values.iter().map(|value| value.symbol).collect(),
                    });
                }
                if !self.expression(index, ExpressionUse::Read, state)? {
                    return Ok(false);
                }
                state
                    .nullable_temporaries
                    .retain(|temporary| temporary.control != id);
                if usage != ExpressionUse::Place
                    && let Some(temporary) = self.checker.temporary_element_owner(id)?
                {
                    self.push_fact(DropFact::new(
                        DropPoint::AfterExpression(id),
                        DropTarget::Temporary(temporary),
                        self.checker
                            .parsed
                            .ast()
                            .expressions()
                            .get(temporary)?
                            .span(),
                    ));
                } else if usage != ExpressionUse::Place
                    && let Some(place) = self.checker.place(id)?
                    && !self.liveness.expression_after[id.index()].contains(&place.root())
                {
                    self.drop_named_asap(DropPoint::AfterExpression(id), place.root(), state);
                }
                Ok(true)
            }
            Expression::CallableReference { receiver, .. } => {
                if let Some(receiver) = receiver {
                    self.expression(receiver, ExpressionUse::Read, state)
                } else {
                    Ok(true)
                }
            }
        }
    }
}
