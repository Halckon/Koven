//! Capture a genuinely traversed closed While body without changing legacy cleanup.
mod conditional;
use super::*;
use crate::ownership_checking::UnitOwnerLoopObservationV1;
use crate::ownership_checking::compilation_unit::owner_transport::{
    LoopBodyCompletion, LoopBranchCompletion,
};
use crate::parser::VariableKind;
use std::collections::BTreeSet;

pub(in super::super) struct LoopTransaction {
    statement: StatementId,
    condition: ExpressionId,
    body: StatementId,
    identity: std::sync::Arc<()>,
    callable: UnitOwnerCallable,
    expected: LoopBodyCompletion,
    active: Option<(std::sync::Arc<()>, usize)>,
    completed: Option<(std::sync::Arc<()>, LoopBodyCompletion)>,
    split_entered: bool,
    active_branch: Option<usize>,
    pending_jump: Option<(std::sync::Arc<()>, LoopBodyCompletion)>,
    branch_completed: [bool; 2],
    result: Option<UnitOwnerLoopObservationV1>,
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
            || parameters
                .iter()
                .any(|parameter| parameter.mode_marker.is_some())
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
        if signature.receiver().is_some()
            || !signature.type_parameters().is_empty()
            || checker.typed.types().get(signature.return_type())
                != Some(&UnitTypeKind::Builtin(BuiltinType::Unit))
        {
            return None;
        }
        let mut parameter_symbols = BTreeSet::new();
        for parameter in parameters {
            let symbol = *checker.marker_symbol(parameter.name)?;
            if checker
                .typed
                .symbol_type(symbol)
                .and_then(|ty| checker.typed.types().get(ty))
                != Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
            {
                return None;
            }
            parameter_symbols.insert(symbol);
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
        let [statement] = elements.as_slice() else {
            return None;
        };
        let Statement::While {
            condition, body, ..
        } = ast.statements().get(*statement).ok()?.payload()
        else {
            return None;
        };
        let parameter = conditional::boolean_parameter(checker, *condition, &parameter_symbols)?;
        let Statement::Block { elements } = ast.statements().get(*body).ok()?.payload() else {
            return None;
        };
        if elements.is_empty() {
            return None;
        }
        let mut expected = LoopBodyCompletion::Fallthrough;
        let mut locals = BTreeSet::new();
        for (index, statement) in elements.iter().enumerate() {
            if let Statement::Expression { expression } =
                ast.statements().get(*statement).ok()?.payload()
            {
                if index + 1 != elements.len()
                    || locals.is_empty()
                    || !exact_type(checker, *expression, BuiltinType::Nothing)
                {
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
        let required_parameters = if matches!(expected, LoopBodyCompletion::Conditional { .. }) {
            2
        } else {
            1
        };
        if parameter_symbols.len() != required_parameters {
            return None;
        }
        Some(Self {
            statement: *statement,
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
        })
    }

    pub(in super::super) fn finish(self) -> Option<UnitOwnerLoopObservationV1> {
        self.result
    }
}

// Exhaustive destructuring makes new planner slots require an explicit boundary decision.
fn empty_boundary(state: &ValueState) -> bool {
    let ValueState {
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
    lambda.is_none()
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
        && values.is_empty()
        && replacements.is_empty()
        && pending_borrows.is_empty()
        && pending_capture_borrows.is_empty()
        && closures.is_empty()
        && closure_asap.is_empty()
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
        let Some(transaction) = &self.owner_loop else {
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
        if self.owner_loop_jump_supported(id) && empty_boundary(state) {
            let transaction = self.owner_loop.as_mut().expect("checked active loop");
            let (identity, _) = transaction.active.as_ref().expect("checked body sink");
            let jump = self.checker.unit_expression(id);
            let completed = if is_continue {
                LoopBodyCompletion::Continue(jump)
            } else {
                LoopBodyCompletion::Break(jump)
            };
            if Some(completed) == transaction.expected_jump() {
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
        if !self.expression(condition, DropExpressionUse::Read, state)? {
            return Ok(false);
        }
        let collecting = self.owner_observation_enabled
            && self.loop_boundaries.is_empty()
            && self.owner_branch_depth == 0
            && empty_boundary(state)
            && self.owner_loop.as_ref().is_some_and(|transaction| {
                transaction.statement == id
                    && transaction.condition == condition
                    && transaction.body == body
                    && transaction.result.is_none()
            });
        let mut body_transport = None;
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
                let transaction = self.owner_loop.as_mut().expect("checked loop transaction");
                transaction.active = None;
                let witnessed = transaction.completed.take();
                transaction.split_entered = false;
                transaction.active_branch = None;
                transaction.pending_jump = None;
                transaction.branch_completed = [false; 2];
                if empty_boundary(&body_state)
                    && let Some(collected) = collected
                {
                    let completion = match (&continued, transaction.expected, witnessed) {
                        (Ok(true), LoopBodyCompletion::Fallthrough, None) => {
                            Some(LoopBodyCompletion::Fallthrough)
                        }
                        (Ok(false), expected, Some((identity, completed)))
                            if expected == completed
                                && std::sync::Arc::ptr_eq(&identity, &collected.identity) =>
                        {
                            Some(completed)
                        }
                        _ => None,
                    };
                    if let Some(completion) = completion {
                        body_transport = collected
                            .value_observation_v1()
                            .map(|transport| (transport, completion));
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
            && empty_boundary(state)
            && let (Some(transaction), Some((transport, completion))) =
                (&mut self.owner_loop, body_transport)
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
                transaction.result = Some(value);
            }
        }
        Ok(true)
    }
}
