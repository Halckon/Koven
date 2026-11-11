//! Capture a genuinely traversed closed While body without changing legacy cleanup.
mod conditional;
mod control_scope;
mod control_transactions;
pub(in super::super) use control_scope::ControlStack;
#[cfg(test)]
pub(in super::super) use control_scope::ControlVisit;
pub(in super::super) use control_transactions::ControlTransactions;
mod invariant;
mod normal_backedge;
mod replace_break;
mod root_frame;
mod root_incoming;
use super::*;
use crate::ownership_checking::compilation_unit::owner_transport::{
    InvariantLoopBoundaries, InvariantLoopValueV1, LoopBodyCompletion, LoopBranchCompletion,
    LoopBranchExit,
};
use crate::ownership_checking::{UnitOwnerLoopObservationV1, UnitOwnerOpenLoopObservationV1};
use crate::parser::VariableKind;
use std::collections::BTreeSet;

pub(in super::super) struct LoopTransaction {
    statement: StatementId,
    condition: ExpressionId,
    body: StatementId,
    identity: std::sync::Arc<()>,
    callable: UnitOwnerCallable,
    // Closed-body / live-Break expectation only. The Return variant never reads this
    // placeholder for completion, edges or publication: it requires its own Ok(false) seal.
    expected: LoopBodyCompletion,
    active: Option<(std::sync::Arc<()>, usize)>,
    completed: Option<(std::sync::Arc<()>, LoopBodyCompletion)>,
    split_entered: bool,
    active_branch: Option<usize>,
    pending_jump: Option<(std::sync::Arc<()>, LoopBodyCompletion)>,
    branch_completed: [bool; 2],
    result: Option<UnitOwnerLoopObservationV1>,
    invariant: Option<invariant::InvariantInput>,
    invariant_result: Option<InvariantLoopValueV1>,
    normal: Option<normal_backedge::NormalInput>,
    replacing: Option<replace_break::OpenLoopInput>,
    replace_result: Option<UnitOwnerOpenLoopObservationV1>,
}

fn exact_type(checker: &Checker<'_>, expression: ExpressionId, builtin: BuiltinType) -> bool {
    !checker.is_constant_use(expression)
        && checker
            .typed
            .expression_type(checker.unit_expression(expression))
            .and_then(|ty| checker.typed.types().get(ty))
            == Some(&UnitTypeKind::Builtin(builtin))
}

fn local_value(
    checker: &Checker<'_>,
    expression: ExpressionId,
    locals: &BTreeSet<UnitSymbolId>,
) -> bool {
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        if !exact_type(checker, expression, BuiltinType::String) {
            return false;
        }
        let Ok(node) = checker.parsed.ast().expressions().get(expression) else {
            return false;
        };
        match node.payload() {
            Expression::String { parts }
                if !parts
                    .iter()
                    .any(|part| matches!(part, StringPart::Interpolation { .. })) => {}
            Expression::Name
                if checker
                    .reference_symbol(node.span())
                    .is_some_and(|symbol| locals.contains(&symbol)) => {}
            Expression::Group { expression } => pending.push(*expression),
            Expression::Call { arguments, .. } => {
                let Some(plan) = checker.ownership_primitives.iter().find(|plan| {
                    plan.descriptor().expression() == checker.unit_expression(expression)
                        && plan.descriptor().kind() == OwnershipPrimitiveKind::Replace
                        && plan.places().len() == 1
                        && plan.places()[0].is_root()
                        && locals.contains(&plan.places()[0].root())
                        && checker.typed.types().get(plan.descriptor().value_type())
                            == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                }) else {
                    return false;
                };
                if arguments.len() != 2
                    || plan.descriptor().operands().len() != 2
                    || arguments.iter().zip(plan.descriptor().operands()).any(
                        |(argument, operand)| checker.unit_expression(argument.value) != operand,
                    )
                {
                    return false;
                }
                pending.push(arguments[1].value);
            }
            _ => return false,
        }
    }
    true
}

impl LoopTransaction {
    pub(in super::super) fn begin(
        checker: &Checker<'_>,
        item: ItemId,
        identity: std::sync::Arc<()>,
    ) -> Option<Self> {
        Self::begin_for_control(checker, item, identity, None)
    }

    fn begin_for_control(
        checker: &Checker<'_>,
        item: ItemId,
        identity: std::sync::Arc<()>,
        selected_control: Option<StatementId>,
    ) -> Option<Self> {
        let ast = checker.parsed.ast();
        let Item::Function {
            name,
            parameters,
            form,
            ..
        } = ast.items().get(item).ok()?.payload()
        else {
            return None;
        };
        if !checker.parsed.roots().contains(&item)
            || !(1..=2).contains(&parameters.len())
            || checker.receiver_context(*name).is_some()
        {
            return None;
        }
        let function = *checker.marker_symbol(*name)?;
        let signature = checker
            .typed
            .signatures()
            .declarations()
            .iter()
            .find(|declaration| declaration.symbol() == function)?
            .callable()?;
        if signature.receiver().is_some() || !signature.type_parameters().is_empty() {
            return None;
        }
        let mut parameter_symbols = BTreeSet::new();
        let mut owner_parameter = None;
        let mut integer_control = false;
        for parameter in parameters {
            let symbol = *checker.marker_symbol(parameter.name)?;
            let ty = checker
                .typed
                .symbol_type(symbol)
                .and_then(|ty| checker.typed.types().get(ty));
            match (ty, parameter.mode_marker) {
                (Some(UnitTypeKind::Builtin(BuiltinType::Int)), None) => {
                    integer_control = true;
                    parameter_symbols.insert(symbol);
                }
                (Some(UnitTypeKind::Builtin(BuiltinType::Boolean)), None) => {
                    parameter_symbols.insert(symbol);
                }
                (
                    Some(UnitTypeKind::Builtin(BuiltinType::String)),
                    Some(crate::parser::ParameterModeMarker::Own(_)),
                ) if parameters.len() == 2
                    && owner_parameter.is_none()
                    && checker.is_move_only_variable(symbol) =>
                {
                    owner_parameter = Some(symbol);
                }
                _ => return None,
            }
        }
        let function_body = match form {
            FunctionForm::ImplicitUnitBlock(body)
            | FunctionForm::Explicit {
                body: FunctionBody::Block(body),
                ..
            } => *body,
            _ => return None,
        };
        let Statement::Block { elements } = ast.statements().get(function_body).ok()?.payload()
        else {
            return None;
        };
        // Only a verified member of an all-While function can be collected separately.
        let selected_elements;
        let elements = if let Some(control) = selected_control {
            if elements.len() < 2
                || !elements.contains(&control)
                || elements.iter().any(|id| {
                    !matches!(
                        ast.statements().get(*id).ok().map(|node| node.payload()),
                        Some(Statement::While { .. })
                    )
                })
            {
                return None;
            }
            selected_elements = vec![control];
            &selected_elements
        } else {
            elements
        };
        if (owner_parameter.is_none() || (integer_control && parameter_symbols.len() == 1))
            && (parameters.len() == 1
                || (integer_control
                    && parameter_symbols.iter().all(|symbol| {
                        checker
                            .typed
                            .symbol_type(*symbol)
                            .and_then(|ty| checker.typed.types().get(ty))
                            == Some(&UnitTypeKind::Builtin(BuiltinType::Int))
                    })))
            && (checker.typed.types().get(signature.return_type())
                == Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
                || (checker.typed.types().get(signature.return_type())
                    == Some(&UnitTypeKind::Builtin(BuiltinType::String))
                    && signature.result_source()
                        == crate::type_checking::CallableResultSource::Owned))
            && let Some(normal) = normal_backedge::NormalInput::admit(
                checker,
                elements,
                &parameter_symbols,
                owner_parameter,
                checker.typed.types().get(signature.return_type())
                    == Some(&UnitTypeKind::Builtin(BuiltinType::Unit)),
                UnitOwnerCallable::Function(UnitItemId::new(checker.source_unit, item)),
            )
        {
            return Some(Self {
                statement: normal.statement,
                condition: normal.condition,
                body: normal.body,
                identity,
                callable: UnitOwnerCallable::Function(UnitItemId::new(checker.source_unit, item)),
                expected: normal.expected_completion(),
                active: None,
                completed: None,
                split_entered: false,
                active_branch: None,
                pending_jump: None,
                branch_completed: [false; 2],
                result: None,
                invariant: None,
                invariant_result: None,
                replacing: None,
                replace_result: None,
                normal: Some(normal),
            });
        }
        // Int parameters are exclusive to the separately sealed normal-header contract.
        // A failed admission must never widen any legacy Boolean family.
        if integer_control {
            return None;
        }
        let replacing = replace_break::OpenLoopInput::admit(
            checker,
            elements,
            &parameter_symbols,
            owner_parameter,
        );
        // Parameter inputs belong only to the delivered Return variant. They must not
        // fall back into the pre-existing all-Boolean closed or invariant loop gates.
        if owner_parameter.is_some()
            && !replacing.as_ref().is_some_and(|input| input.is_returning())
        {
            return None;
        }
        let returns_owner = replacing.as_ref().is_some_and(|input| matches!(input.consumer_target, crate::ownership_checking::compilation_unit::owner_transport::ReplaceBreakSuffix::Return(_)));
        let result = if returns_owner {
            BuiltinType::String
        } else {
            BuiltinType::Unit
        };
        if checker.typed.types().get(signature.return_type())
            != Some(&UnitTypeKind::Builtin(result))
            || (returns_owner
                && signature.result_source() != crate::type_checking::CallableResultSource::Owned)
        {
            return None;
        }
        let (statement, mut invariant) = if let Some(input) = &replacing {
            (input.statement, None)
        } else {
            match elements.as_slice() {
                [statement] => (*statement, None),
                [prefix, statement, suffix] => (
                    *statement,
                    Some(invariant::InvariantInput::admit(
                        checker, None, *prefix, *suffix,
                    )?),
                ),
                [seed, prefix, statement, suffix] => (
                    *statement,
                    Some(invariant::InvariantInput::admit(
                        checker,
                        Some(*seed),
                        *prefix,
                        *suffix,
                    )?),
                ),
                _ => return None,
            }
        };
        let Statement::While {
            condition, body, ..
        } = ast.statements().get(statement).ok()?.payload()
        else {
            return None;
        };
        let parameter = match &replacing {
            Some(input) => input.parameter,
            None => conditional::boolean_parameter(checker, *condition, &parameter_symbols)?,
        };
        let Statement::Block { elements } = ast.statements().get(*body).ok()?.payload() else {
            return None;
        };
        if elements.is_empty() {
            return None;
        }
        let mut expected = replacing
            .as_ref()
            .map_or(LoopBodyCompletion::Fallthrough, |input| {
                if input.is_returning() {
                    LoopBodyCompletion::Fallthrough
                } else {
                    LoopBodyCompletion::Break(input.jump)
                }
            });
        let mut locals = BTreeSet::new();
        if replacing.is_none() {
            for (index, statement) in elements.iter().enumerate() {
                if let Statement::Expression { expression } =
                    ast.statements().get(*statement).ok()?.payload()
                {
                    if index + 1 != elements.len() || locals.is_empty() {
                        return None;
                    }
                    let jump = checker.unit_expression(*expression);
                    expected = match ast.expressions().get(*expression).ok()?.payload() {
                        Expression::Continue { .. } => LoopBodyCompletion::Continue(jump),
                        Expression::Break { .. } => LoopBodyCompletion::Break(jump),
                        Expression::If { .. } => conditional::admit_split(
                            checker,
                            *expression,
                            &parameter_symbols,
                            parameter,
                        )?,
                        _ => return None,
                    };
                    let result_type = if expected.continues() {
                        BuiltinType::Unit
                    } else {
                        BuiltinType::Nothing
                    };
                    if !exact_type(checker, *expression, result_type) {
                        return None;
                    }
                    continue;
                }
                let Statement::LocalVariable { declaration } =
                    ast.statements().get(*statement).ok()?.payload()
                else {
                    return None;
                };
                let Item::Variable {
                    name,
                    kind: VariableKind::Val | VariableKind::Var,
                    initializer,
                    ..
                } = ast.items().get(*declaration).ok()?.payload()
                else {
                    return None;
                };
                let symbol = *checker.marker_symbol(*name)?;
                if checker
                    .typed
                    .symbol_type(symbol)
                    .and_then(|ty| checker.typed.types().get(ty))
                    != Some(&UnitTypeKind::Builtin(BuiltinType::String))
                    || !checker.is_move_only_variable(symbol)
                    || !local_value(checker, *initializer, &locals)
                {
                    return None;
                }
                locals.insert(symbol);
            }
        }
        let required_parameters = if matches!(expected, LoopBodyCompletion::Conditional { .. }) {
            2
        } else {
            1
        };
        if parameter_symbols.len() != required_parameters {
            return None;
        }
        if let Some(input) = &mut invariant {
            match expected {
                LoopBodyCompletion::Fallthrough
                | LoopBodyCompletion::Continue(_)
                | LoopBodyCompletion::Break(_) => {}
                LoopBodyCompletion::Conditional { branches, .. }
                    if branches.iter().all(|branch| {
                        matches!(
                            branch.exit,
                            LoopBranchExit::Continue(_) | LoopBranchExit::Break(_)
                        )
                    }) =>
                {
                    input.enable_terminal_split()
                }
                LoopBodyCompletion::Conditional { branches, .. }
                    if branches
                        .iter()
                        .any(|branch| branch.exit == LoopBranchExit::Fallthrough) =>
                {
                    input.enable_joining();
                }
                _ => return None,
            }
        }
        Some(Self {
            statement,
            condition: *condition,
            body: *body,
            identity,
            callable: UnitOwnerCallable::Function(UnitItemId::new(checker.source_unit, item)),
            expected,
            active: None,
            completed: None,
            split_entered: false,
            active_branch: None,
            pending_jump: None,
            branch_completed: [false; 2],
            result: None,
            invariant,
            invariant_result: None,
            normal: None,
            replacing,
            replace_result: None,
        })
    }

    pub(in super::super) fn finish(
        self,
    ) -> (
        Option<UnitOwnerLoopObservationV1>,
        Option<InvariantLoopValueV1>,
        Option<UnitOwnerOpenLoopObservationV1>,
    ) {
        (self.result, self.invariant_result, self.replace_result)
    }
}

impl DropPlanner<'_, '_> {
    // The terminal observer uses the same active body catalog and lexical boundary as traversal.
    fn open_return_context(&self) -> Option<bool> {
        let transaction = self.owner_loop.as_ref()?;
        if !transaction.replacing.as_ref()?.is_returning() || self.owner_branch_depth != 0 {
            return None;
        }
        match &transaction.active {
            Some((identity, boundary)) => (self.owner_controls.is_current(UnitStatementId::new(
                self.checker.source_unit,
                transaction.statement,
            )) && self.loop_boundaries.as_slice() == [*boundary]
                && self.scope_depth == boundary + 1
                && transaction.completed.is_none()
                && self.owner_observation.as_ref().is_some_and(|observation| {
                    observation.status == UnitOwnerObservationStatus::Complete
                        && std::sync::Arc::ptr_eq(identity, &observation.identity)
                }))
            .then_some(true),
            None => (self.owner_controls.balanced()
                && self.loop_boundaries.is_empty()
                && self.scope_depth == 1)
                .then_some(false),
        }
    }
    pub(in super::super) fn take_open_loop_return(
        &mut self,
        expression: ExpressionId,
        value: Option<&OwnedValue>,
    ) {
        self.take_normal_return(expression, value);
        if self.open_return_context().is_none() {
            return;
        }
        let transaction = self.owner_loop.as_mut().expect("checked open loop");
        let identity = transaction.active.as_ref().map(|(identity, _)| identity);
        transaction
            .replacing
            .as_mut()
            .expect("checked prefix")
            .return_take(self.checker.unit_expression(expression), value, identity);
    }
    pub(in super::super) fn deliver_open_loop_return(
        &mut self,
        expression: ExpressionId,
        payload: ExpressionId,
        owner: Option<&UnitOwnerId>,
        state: &ValueState,
    ) {
        self.deliver_normal_return(expression, payload, state);
        let Some(body) = self.open_return_context() else {
            return;
        };
        self.owner_loop
            .as_mut()
            .expect("checked open loop")
            .replacing
            .as_mut()
            .expect("checked prefix")
            .return_deliver(
                self.checker.unit_expression(expression),
                self.checker.unit_expression(payload),
                owner,
                state,
                body,
            );
    }
    pub(in super::super) fn finish_open_loop_return(
        &mut self,
        expression: ExpressionId,
        state: &ValueState,
    ) {
        self.finish_normal_return(expression, state);
        let Some(body) = self.open_return_context() else {
            return;
        };
        let transaction = self.owner_loop.as_mut().expect("checked open loop");
        if let Some(value) = transaction
            .replacing
            .as_mut()
            .expect("checked prefix")
            .return_cleanup(self.checker.unit_expression(expression), state, body)
            && transaction.replace_result.is_none()
        {
            transaction.replace_result = Some(value);
        }
    }
}

// Exhaustive destructuring makes new planner slots require an explicit boundary decision.
fn boundary_values(state: &ValueState) -> Option<&[OwnedValue]> {
    let ValueState {
        // Pure observation metadata does not change the existing owner boundary.
        forward_scope: _,
        branch_forward_plan: _,
        replace_consumer,
        lambda,
        borrow_bindings,
        pending_borrow_results,
        borrow_initializer_sources,
        pending_loans,
        borrow_call_transports,
        borrow_binding_transports,
        borrow_forward_transports,
        iterations,
        pending_temporaries,
        values,
        replacements,
        pending_borrows,
        pending_capture_borrows,
        closures,
        closure_asap,
        this,
    } = state;
    (replace_consumer.is_none()
        && lambda.is_none()
        && this.is_none()
        && borrow_bindings.is_empty()
        && pending_borrow_results.is_empty()
        && borrow_initializer_sources.is_empty()
        && pending_loans.is_empty()
        && borrow_call_transports.is_empty()
        && borrow_binding_transports.is_empty()
        && borrow_forward_transports.is_empty()
        && iterations.is_empty()
        && pending_temporaries.is_empty()
        && replacements.is_empty()
        && pending_borrows.is_empty()
        && pending_capture_borrows.is_empty()
        && closures.is_empty()
        && closure_asap.is_empty())
    .then_some(values)
}

fn empty_boundary(state: &ValueState) -> bool {
    boundary_values(state).is_some_and(<[OwnedValue]>::is_empty)
}

impl DropPlanner<'_, '_> {
    // Preserve the ordinary jump cleanup sequence; the witness is only a final observer.
    pub(in super::super) fn owner_loop_jump_expression(
        &mut self,
        id: ExpressionId,
        is_continue: bool,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let boundary = self.loop_boundaries.last().copied().unwrap_or(0);
        self.end_iteration_pending_loans(
            PlannerDropPoint::ControlTransfer(id),
            state,
            self.loop_boundaries.len(),
        );
        self.drop_deeper_than(
            boundary,
            self.loop_boundaries.len(),
            PlannerDropPoint::ControlTransfer(id),
            state,
        );
        self.finish_jump_iteration(id, state)?;
        self.finish_owner_loop_jump(id, is_continue, state);
        Ok(false)
    }

    // Only the precise expected tail in the fresh, active body sink is admitted.
    pub(in super::super) fn owner_loop_jump_supported(&self, id: ExpressionId) -> bool {
        let Some(transaction) = self.owner_loop.as_ref() else {
            return false;
        };
        let Some((identity, boundary)) = &transaction.active else {
            return false;
        };
        let Some(expected) = transaction.expected_jump() else {
            return false;
        };
        let branch_depth = usize::from(transaction.active_branch.is_some());
        let expected = match expected {
            LoopBodyCompletion::Continue(jump) | LoopBodyCompletion::Break(jump) => jump,
            _ => return false,
        };
        expected == self.checker.unit_expression(id)
            && self.owner_controls.is_current(UnitStatementId::new(
                self.checker.source_unit,
                transaction.statement,
            ))
            && transaction.completed.is_none()
            && transaction.pending_jump.is_none()
            && self.loop_boundaries.as_slice() == [*boundary]
            && self.scope_depth == boundary + 1 + branch_depth
            && self.owner_branch_depth == branch_depth
            && self.owner_observation.as_ref().is_some_and(|observation| {
                observation.status == UnitOwnerObservationStatus::Complete
                    && std::sync::Arc::ptr_eq(&observation.identity, identity)
            })
    }

    // Invoked only after the real jump's loans, owners and iteration cleanup succeeded.
    fn finish_owner_loop_jump(&mut self, id: ExpressionId, is_continue: bool, state: &ValueState) {
        if self.owner_loop_jump_supported(id) {
            let transaction = self.owner_loop.as_mut().expect("checked active loop");
            let identity = transaction
                .active
                .as_ref()
                .expect("checked body sink")
                .0
                .clone();
            let jump = self.checker.unit_expression(id);
            let completed = if is_continue {
                LoopBodyCompletion::Continue(jump)
            } else {
                LoopBodyCompletion::Break(jump)
            };
            if Some(completed) == transaction.expected_jump()
                && transaction.jump_boundary(completed, &identity, state)
            {
                if transaction.active_branch.is_some() {
                    transaction.pending_jump = Some((identity.clone(), completed));
                } else {
                    transaction.completed = Some((identity.clone(), completed));
                }
            }
        }
    }

    pub(in super::super) fn owner_while_statement(
        &mut self,
        id: StatementId,
        condition: ExpressionId,
        body: StatementId,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let control = UnitStatementId::new(self.checker.source_unit, id);
        #[cfg(test)]
        let parent = self.owner_controls.current();
        let previous_transaction = self.owner_loop.selected();
        let entered = self.owner_controls.enter(control);
        if entered {
            self.owner_loop.select(Some(control));
            self.owner_loop
                .record_root_entry(control, state, self.owner_observation.as_ref());
        } else {
            self.owner_loop.clear();
        }
        #[cfg(test)]
        if let Some(probe) = &mut self.owner_control_probe {
            probe.push(ControlVisit {
                entering: true,
                control,
                parent,
                closed_product: None,
                incoming_roots: self.owner_loop.root_entry(control).map(<[_]>::to_vec),
            });
        }
        let result = self.owner_while_statement_scoped(id, condition, body, state);
        #[cfg(test)]
        let closed_product = self.owner_loop.as_ref().and_then(|t| t.result.clone());
        if entered {
            self.owner_controls.leave(control);
        }
        self.owner_loop.select(previous_transaction);
        #[cfg(test)]
        if let Some(probe) = &mut self.owner_control_probe {
            probe.push(ControlVisit {
                entering: false,
                control,
                parent: self.owner_controls.current(),
                closed_product,
                incoming_roots: None,
            });
        }
        result
    }

    fn owner_while_statement_scoped(
        &mut self,
        id: StatementId,
        condition: ExpressionId,
        body: StatementId,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        if !self.expression(condition, DropExpressionUse::Read, state)? {
            return Ok(false);
        }
        let collecting = self.owner_observation_enabled
            && self
                .owner_controls
                .is_current(UnitStatementId::new(self.checker.source_unit, id))
            && self.loop_boundaries.is_empty()
            && self.owner_branch_depth == 0
            && self.owner_loop.as_mut().is_some_and(|transaction| {
                transaction.statement == id
                    && transaction.condition == condition
                    && transaction.body == body
                    && transaction.result.is_none()
                    && transaction.invariant_result.is_none()
                    && transaction.replace_result.is_none()
                    && transaction.entry_boundary(state)
            });
        let mut body_transport = None;
        let mut replacement_body = None;
        if self.checker.ordinary_condition_outcomes(condition).0 {
            let mut body_state = state.clone();
            // The private body sink has a fresh catalog, never aliases the callable catalog,
            // and is not pushed as an extra callable observation.
            let saved = if collecting {
                let callable = self
                    .owner_loop
                    .as_ref()
                    .expect("checked loop transaction")
                    .callable;
                let identity = std::sync::Arc::new(());
                self.owner_loop
                    .as_mut()
                    .expect("checked loop transaction")
                    .active = Some((identity.clone(), self.scope_depth));
                Some((
                    self.owner_observation.replace(UnitOwnerObservation {
                        identity,
                        callable,
                        status: UnitOwnerObservationStatus::Complete,
                        definitions: Vec::new(),
                        events: Vec::new(),
                        choice_value: None,
                        loop_value: None,
                        invariant_loop_value: None,
                        closed_loop_batch: None,
                        open_loop_value: None,
                    }),
                    self.owner_choice.take(),
                    self.owner_branch_depth,
                ))
            } else {
                None
            };
            self.loop_boundaries.push(self.scope_depth);
            let continued = self.statement(body, &mut body_state);
            self.loop_boundaries.pop();
            if let Some((observation, choice, depth)) = saved {
                let collected = self.owner_observation.take();
                self.owner_observation = observation;
                self.owner_choice = choice;
                self.owner_branch_depth = depth;
                // Restore every sink before propagating an error or considering publication.
                if let Some(transaction) = self.owner_loop.as_mut() {
                    transaction.active = None;
                    let witnessed = transaction.completed.take();
                    transaction.split_entered = false;
                    transaction.active_branch = None;
                    transaction.pending_jump = None;
                    transaction.branch_completed = [false; 2];
                    if transaction
                        .replacing
                        .as_ref()
                        .is_some_and(|input| input.is_returning())
                    {
                        if matches!(continued, Ok(false))
                            && witnessed.is_none()
                            && transaction.body_completion_boundary(&body_state)
                            && let Some(collected) = collected
                        {
                            replacement_body = Some((collected, body_state));
                        }
                    } else if transaction.body_completion_boundary(&body_state)
                        && let Some(collected) = collected
                    {
                        let completion = match (&continued, transaction.expected, witnessed) {
                            (Ok(true), LoopBodyCompletion::Fallthrough, None) => {
                                Some(LoopBodyCompletion::Fallthrough)
                            }
                            (Ok(continues), expected, Some((identity, completed)))
                                if *continues == expected.continues()
                                    && expected != LoopBodyCompletion::Fallthrough
                                    && expected == completed
                                    && std::sync::Arc::ptr_eq(&identity, &collected.identity) =>
                            {
                                Some(completed)
                            }
                            _ => None,
                        };
                        if let Some(completion) = completion {
                            if transaction
                                .normal
                                .as_mut()
                                .is_some_and(|normal| normal.finish_role(completion))
                                || transaction.replacing.is_some()
                            {
                                replacement_body = Some((collected, body_state));
                            } else {
                                body_transport = collected
                                    .value_observation_v1()
                                    .map(|transport| (transport, completion));
                            }
                        }
                    }
                }
            }
            continued?;
        }
        if self.checker.non_fallthrough_whiles.contains(&id.index()) {
            return Ok(false);
        }
        self.drop_loop_exit(id, state);
        if collecting
            && self.owner_loop.as_ref().is_some_and(|t| t.normal.is_some())
            && let Some((collected, _)) = replacement_body.take()
            && let Some(normal) = self.owner_loop.as_mut().and_then(|t| t.normal.as_mut())
        {
            normal.finish_body(collected, state);
        }
        if collecting
            && let (Some(transaction), Some((collected, mut body_state))) =
                (self.owner_loop.as_mut(), replacement_body)
            && let Some(input) = &mut transaction.replacing
        {
            transaction.replace_result = input.finish(collected, state);
            if let (Some(boundary), Some(condition), Some(body)) = (
                transaction
                    .replace_result
                    .as_ref()
                    .and_then(|value| match value {
                        UnitOwnerOpenLoopObservationV1::BreakLive(boundary) => Some(boundary),
                        UnitOwnerOpenLoopObservationV1::ReturnDelivered(_)
                        | UnitOwnerOpenLoopObservationV1::NormalBackedge(_)
                        | UnitOwnerOpenLoopObservationV1::NormalBindDrop(_)
                        | UnitOwnerOpenLoopObservationV1::MixedBackedge(_)
                        | UnitOwnerOpenLoopObservationV1::ConditionalBreakBackedge(_)
                        | UnitOwnerOpenLoopObservationV1::ConditionalReturnBackedge(_)
                        | UnitOwnerOpenLoopObservationV1::ContinueBackedge(_) => None,
                    }),
                state.values.first(),
                body_state.values.pop(),
            ) {
                state.replace_consumer = super::replace_consumer::ReplaceConsumerCarrier::handoff(
                    condition,
                    body,
                    input.consumer_target.clone(),
                    boundary,
                );
            }
        }
        if collecting
            && let (Some(transaction), Some((transport, completion))) =
                (self.owner_loop.as_mut(), body_transport)
        {
            let statement = UnitStatementId::new(self.checker.source_unit, id);
            let condition = self.checker.unit_expression(condition);
            let body = UnitStatementId::new(self.checker.source_unit, body);
            let value = UnitOwnerLoopObservationV1 {
                identity: transaction.identity.clone(),
                callable: transaction.callable,
                body_identity: transport.catalog.identity.clone(),
                statement,
                condition,
                body,
                transport,
                completion,
                edges: completion.edges(statement, condition, body),
            };
            if value.is_consistent() {
                if let Some(invariant) = &mut transaction.invariant {
                    transaction.invariant_result = invariant.finish(value, state);
                } else if empty_boundary(state) {
                    transaction.result = Some(value);
                }
            }
        }
        Ok(true)
    }
}

impl LoopTransaction {
    pub(in super::super) fn complete_replace_consumer(
        &mut self,
        consumer: crate::ownership_checking::UnitOwnerReplaceBreakConsumerV1,
    ) {
        if let Some(UnitOwnerOpenLoopObservationV1::BreakLive(boundary)) = &mut self.replace_result
            && boundary.consumer.is_none()
        {
            boundary.consumer = Some(consumer);
            if !boundary.is_consistent() {
                boundary.consumer = None;
            }
        }
    }
}

impl LoopTransaction {
    pub(in super::super) fn complete_replace_call_consumer(
        &mut self,
        consumer: crate::ownership_checking::UnitOwnerReplaceBreakCallConsumerV1,
    ) {
        if let Some(UnitOwnerOpenLoopObservationV1::BreakLive(boundary)) = &mut self.replace_result
            && boundary.consumer.is_none()
            && boundary.call_consumer.is_none()
        {
            boundary.call_consumer = Some(consumer);
            if !boundary.is_consistent() {
                boundary.call_consumer = None;
            }
        }
    }
}

impl LoopTransaction {
    pub(in super::super) fn complete_replace_return_consumer(
        &mut self,
        consumer: crate::ownership_checking::UnitOwnerReplaceBreakReturnConsumerV1,
    ) {
        if let Some(UnitOwnerOpenLoopObservationV1::BreakLive(boundary)) = &mut self.replace_result
            && boundary.consumer.is_none()
            && boundary.call_consumer.is_none()
            && boundary.return_consumer.is_none()
        {
            boundary.return_consumer = Some(consumer);
            if !boundary.is_consistent() {
                boundary.return_consumer = None;
            }
        }
    }
}
