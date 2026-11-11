//! A terminal If has two actual arms; owner snapshots precede state discard, never a phi.
use super::*;

pub(super) fn boolean_parameter(
    checker: &Checker<'_>,
    expression: ExpressionId,
    parameters: &BTreeSet<UnitSymbolId>,
) -> Option<UnitSymbolId> {
    if checker.ordinary_condition_outcomes(expression) != (true, true) {
        return None;
    }
    let mut current = expression;
    loop {
        if !exact_type(checker, current, BuiltinType::Boolean) {
            return None;
        }
        let node = checker.parsed.ast().expressions().get(current).ok()?;
        match node.payload() {
            Expression::Group { expression } => current = *expression,
            Expression::Prefix {
                operator: crate::parser::PrefixOperator::Not,
                operand,
                ..
            } => current = *operand,
            Expression::Name => {
                return checker
                    .reference_symbol(node.span())
                    .filter(|symbol| parameters.contains(symbol));
            }
            _ => return None,
        }
    }
}

fn admit_branch(checker: &Checker<'_>, body: StatementId) -> Option<LoopBranchCompletion> {
    let ast = checker.parsed.ast();
    let Statement::ControlBody { elements } = ast.statements().get(body).ok()?.payload() else {
        return None;
    };
    if elements.is_empty() {
        return Some(LoopBranchCompletion {
            body: UnitStatementId::new(checker.source_unit, body),
            exit: LoopBranchExit::Fallthrough,
        });
    }
    let [statement] = elements.as_slice() else {
        return None;
    };
    let Statement::Expression { expression } = ast.statements().get(*statement).ok()?.payload()
    else {
        return None;
    };
    if !exact_type(checker, *expression, BuiltinType::Nothing) {
        return None;
    }
    let jump = checker.unit_expression(*expression);
    let exit = match ast.expressions().get(*expression).ok()?.payload() {
        Expression::Continue { .. } => LoopBranchExit::Continue(jump),
        Expression::Break { .. } => LoopBranchExit::Break(jump),
        _ => return None,
    };
    Some(LoopBranchCompletion {
        body: UnitStatementId::new(checker.source_unit, body),
        exit,
    })
}

pub(super) fn admit_split(
    checker: &Checker<'_>,
    control: ExpressionId,
    parameters: &BTreeSet<UnitSymbolId>,
    loop_parameter: UnitSymbolId,
) -> Option<LoopBodyCompletion> {
    let Expression::If {
        condition,
        then_branch,
        else_branch: Some(else_branch),
        ..
    } = checker
        .parsed
        .ast()
        .expressions()
        .get(control)
        .ok()?
        .payload()
    else {
        return None;
    };
    if boolean_parameter(checker, *condition, parameters)? == loop_parameter {
        return None;
    }
    Some(LoopBodyCompletion::Conditional {
        control: checker.unit_expression(control),
        condition: checker.unit_expression(*condition),
        branches: [
            admit_branch(checker, *then_branch)?,
            admit_branch(checker, *else_branch)?,
        ],
    })
}

impl LoopTransaction {
    pub(super) fn expected_jump(&self) -> Option<LoopBodyCompletion> {
        match self.expected {
            LoopBodyCompletion::ConditionalBreak { jump, .. }
                if self.split_entered
                    && self.active_branch == Some(0)
                    && !self.branch_completed[0] =>
            {
                Some(LoopBodyCompletion::Break(jump))
            }
            LoopBodyCompletion::Continue(_) | LoopBodyCompletion::Break(_)
                if self.active_branch.is_none() =>
            {
                Some(self.expected)
            }
            LoopBodyCompletion::Conditional { branches, .. } if self.split_entered => {
                let branch = self.active_branch?;
                (!self.branch_completed[branch])
                    .then_some(branches[branch].completion())
                    .flatten()
            }
            _ => None,
        }
    }
}

impl DropPlanner<'_, '_> {
    // Permit only Not nodes of the already admitted If condition in its live body sink.
    // This is a syntax gate, not fabricated condition evaluation or owner evidence.
    pub(in super::super) fn owner_loop_condition_supported(&self, id: ExpressionId) -> bool {
        let Some(transaction) = self.owner_loop.as_ref() else {
            return false;
        };
        let LoopBodyCompletion::Conditional { condition, .. } = transaction.expected else {
            return false;
        };
        let Some((identity, boundary)) = &transaction.active else {
            return false;
        };
        if transaction.split_entered
            || transaction.active_branch.is_some()
            || transaction.pending_jump.is_some()
            || transaction.completed.is_some()
            || transaction.branch_completed != [false, false]
            || self.owner_branch_depth != 0
            || self.owner_choice.is_some()
            || self.loop_boundaries.as_slice() != [*boundary]
            || self.scope_depth != boundary + 1
            || !self.owner_observation.as_ref().is_some_and(|observation| {
                std::sync::Arc::ptr_eq(&observation.identity, identity)
                    && observation.value_observation_v1().is_some()
            })
        {
            return false;
        }
        let mut current = condition.expression();
        loop {
            if !exact_type(self.checker, current, BuiltinType::Boolean) {
                return false;
            }
            let Ok(node) = self.checker.parsed.ast().expressions().get(current) else {
                return false;
            };
            match node.payload() {
                Expression::Group { expression } => current = *expression,
                Expression::Prefix {
                    operator: crate::parser::PrefixOperator::Not,
                    operand,
                    ..
                } => {
                    if current == id {
                        return true;
                    }
                    current = *operand;
                }
                _ => return false,
            }
        }
    }

    pub(in super::super) fn begin_owner_loop_split(
        &mut self,
        control: ExpressionId,
        condition: ExpressionId,
        outcomes: (bool, bool),
        state: &ValueState,
    ) {
        if outcomes != (true, true) || self.owner_branch_depth != 0 {
            return;
        }
        let Some(transaction) = self.owner_loop.as_mut() else {
            return;
        };
        let LoopBodyCompletion::Conditional {
            control: expected,
            condition: expected_condition,
            ..
        } = transaction.expected
        else {
            return;
        };
        let Some((identity, boundary)) = &transaction.active else {
            return;
        };
        if expected != self.checker.unit_expression(control)
            || expected_condition != self.checker.unit_expression(condition)
            || transaction.split_entered
            || transaction.completed.is_some()
            || self.loop_boundaries.as_slice() != [*boundary]
            || self.scope_depth != boundary + 1
        {
            return;
        }
        if self.owner_observation.as_ref().is_some_and(|observation| {
            std::sync::Arc::ptr_eq(&observation.identity, identity)
                && (observation.value_observation_v1().is_some()
                    || (transaction
                        .normal
                        .as_ref()
                        .is_some_and(|n| n.mixed_active())
                        && observation.status == UnitOwnerObservationStatus::Complete))
        }) && transaction.split_input_boundary(state)
        {
            transaction.split_entered = true;
        }
    }

    pub(in super::super) fn begin_owner_loop_arm(
        &mut self,
        control: ExpressionId,
        branch: usize,
        body: StatementId,
        state: &ValueState,
    ) {
        if self.owner_branch_depth != 1 {
            return;
        }
        let Some(transaction) = self.owner_loop.as_mut() else {
            return;
        };
        let LoopBodyCompletion::Conditional {
            control: expected,
            branches,
            ..
        } = transaction.expected
        else {
            return;
        };
        let Some(expected_branch) = branches.get(branch) else {
            return;
        };
        if transaction.split_entered
            && transaction.active_branch.is_none()
            && transaction.pending_jump.is_none()
            && !transaction.branch_completed[branch]
            && expected == self.checker.unit_expression(control)
            && expected_branch.body == UnitStatementId::new(self.checker.source_unit, body)
            && transaction.split_arm_entry_boundary(
                branch,
                self.checker.unit_expression(control),
                UnitStatementId::new(self.checker.source_unit, body),
                state,
            )
        {
            transaction.active_branch = Some(branch);
        }
    }

    pub(in super::super) fn finish_owner_loop_arm(
        &mut self,
        control: ExpressionId,
        branch: usize,
        tail: Option<ExpressionId>,
        result: &EvaluatedOwner,
        state: &ValueState,
    ) {
        let Some(transaction) = self.owner_loop.as_mut() else {
            return;
        };
        let LoopBodyCompletion::Conditional {
            control: expected,
            branches,
            ..
        } = transaction.expected
        else {
            return;
        };
        if expected != self.checker.unit_expression(control)
            || transaction.active_branch != Some(branch)
        {
            return;
        }
        let pending = transaction.pending_jump.take();
        transaction.active_branch = None;
        let Some(expected_branch) = branches.get(branch) else {
            return;
        };
        let Some((active, boundary)) = &transaction.active else {
            return;
        };
        let completion_matches = if expected_branch.continues() {
            result.continues && tail.is_none() && pending.is_none()
        } else {
            !result.continues
                && tail.map(|tail| self.checker.unit_expression(tail)) == expected_branch.jump()
                && pending.is_some_and(|(identity, completed)| {
                    std::sync::Arc::ptr_eq(&identity, active)
                        && Some(completed) == expected_branch.completion()
                })
        };
        if completion_matches
            && result.owner.is_none()
            && self.scope_depth == boundary + 1
            && self.owner_branch_depth == 1
            && self.loop_boundaries.as_slice() == [*boundary]
            && self.owner_observation.as_ref().is_some_and(|observation| {
                observation.status == UnitOwnerObservationStatus::Complete
                    && std::sync::Arc::ptr_eq(active, &observation.identity)
            })
            && transaction.split_arm_completion_boundary(branch, result.continues, state)
        {
            transaction.branch_completed[branch] = true;
        }
    }

    pub(in super::super) fn finish_owner_loop_split(
        &mut self,
        control: ExpressionId,
        then_events: &[UnitOwnerTransportEvent],
        else_events: &[UnitOwnerTransportEvent],
        continuations: usize,
    ) -> bool {
        if !then_events.is_empty() || !else_events.is_empty() {
            return false;
        }
        let Some(transaction) = self.owner_loop.as_mut() else {
            return false;
        };
        let LoopBodyCompletion::Conditional {
            control: expected,
            branches,
            ..
        } = transaction.expected
        else {
            return false;
        };
        let Some((identity, _)) = &transaction.active else {
            return false;
        };
        if transaction.split_entered
            && continuations == branches.iter().filter(|branch| branch.continues()).count()
            && transaction.branch_completed == [true, true]
            && transaction.split_inputs_complete()
            && transaction.active_branch.is_none()
            && transaction.pending_jump.is_none()
            && transaction.completed.is_none()
            && expected == self.checker.unit_expression(control)
            && self.owner_observation.as_ref().is_some_and(|observation| {
                observation.status == UnitOwnerObservationStatus::Complete
                    && std::sync::Arc::ptr_eq(identity, &observation.identity)
            })
        {
            transaction.completed = Some((identity.clone(), transaction.expected));
            true
        } else {
            false
        }
    }
}
