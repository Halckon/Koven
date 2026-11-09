//! Capture a genuinely traversed closed While body without changing legacy cleanup.
use super::*;
use crate::ownership_checking::{UnitOwnerLoopEdgeV1 as Edge, UnitOwnerLoopObservationV1};
use crate::parser::VariableKind;
use std::collections::BTreeSet;

pub(in super::super) struct LoopTransaction {
    statement: StatementId,
    condition: ExpressionId,
    body: StatementId,
    identity: std::sync::Arc<()>,
    callable: UnitOwnerCallable,
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
            || parameters.len() != 1
            || parameters[0].mode_marker.is_some()
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
        let parameter = *checker.marker_symbol(parameters[0].name)?;
        if checker
            .typed
            .symbol_type(parameter)
            .and_then(|ty| checker.typed.types().get(ty))
            != Some(&UnitTypeKind::Builtin(BuiltinType::Boolean))
        {
            return None;
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
        let mut current = *condition;
        loop {
            if !exact_type(checker, current, BuiltinType::Boolean) {
                return None;
            }
            let node = ast.expressions().get(current).ok()?;
            match node.payload() {
                Expression::Group { expression } => current = *expression,
                Expression::Name if checker.reference_symbol(node.span()) == Some(parameter) => {
                    break;
                }
                _ => return None,
            }
        }
        if checker.ordinary_condition_outcomes(*condition) != (true, true) {
            return None;
        }
        let Statement::Block { elements } = ast.statements().get(*body).ok()?.payload() else {
            return None;
        };
        if elements.is_empty() {
            return None;
        }
        let mut locals = BTreeSet::new();
        for statement in elements {
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
        Some(Self {
            statement: *statement,
            condition: *condition,
            body: *body,
            identity,
            callable: UnitOwnerCallable::Function(UnitItemId::new(checker.source_unit, item)),
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
                Some((
                    self.owner_observation.replace(UnitOwnerObservation {
                        identity: std::sync::Arc::new(()),
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
                if matches!(&continued, Ok(true)) && empty_boundary(&body_state) {
                    body_transport = collected.and_then(|value| value.value_observation_v1());
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
            && let (Some(transaction), Some(transport)) = (&mut self.owner_loop, body_transport)
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
                edges: [
                    Edge::EnterBody(condition, body),
                    Edge::Backedge(
                        crate::ownership_checking::UnitDropPoint::AfterStatement(body),
                        condition,
                    ),
                    Edge::ConditionFalseExit(
                        condition,
                        crate::ownership_checking::UnitDropPoint::LoopExit(statement),
                    ),
                ],
            };
            if value.is_consistent() {
                transaction.result = Some(value);
            }
        }
        Ok(true)
    }
}
