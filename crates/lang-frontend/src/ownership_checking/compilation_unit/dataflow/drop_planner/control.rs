//! Control expression 分支 tail 的 usage 与析构规划。

use std::collections::BTreeSet;

use crate::{
    ast::{ExpressionId, StatementId},
    name_resolution::UnitSymbolId,
    ownership_checking::OwnershipCheckingError,
    parser::{Statement, WhenCondition, WhenEntry},
};

use super::owner_transport::EvaluatedOwner;

use super::{
    DropExpressionUse, DropPlanner, PlannerDropFact, PlannerDropPoint, PlannerDropTarget,
    ValueState,
};

impl DropPlanner<'_, '_> {
    /// Execute the release selected by the loan checker against concrete planner owners.
    pub(super) fn drop_branch_entry(
        &mut self,
        control: ExpressionId,
        branch: usize,
        settled: &[UnitSymbolId],
        state: &mut ValueState,
    ) {
        let Some(required) = self
            .checker
            .branch_capture_releases
            .get(&(self.checker.unit_expression(control), branch))
        else {
            return;
        };
        if required.values().any(|origins| origins.len() != 1) {
            self.defer_resource(control);
            return;
        }
        let point = PlannerDropPoint::BranchEntry { control, branch };
        let first_fact = self.facts.len();
        let candidates = state
            .values
            .iter()
            .rev()
            .filter(|value| required.contains_key(&value.symbol))
            .map(|value| value.symbol)
            .collect::<Vec<_>>();
        if candidates
            .iter()
            .any(|&symbol| !self.branch_release_matches(control, branch, symbol, state))
        {
            self.defer_resource(control);
            return;
        }
        for symbol in candidates {
            self.drop_named_asap(point, symbol, state);
        }
        let released = self.facts[first_fact..]
            .iter()
            .filter_map(|fact| match (fact.point(), fact.target()) {
                (emitted, PlannerDropTarget::Named(symbol)) if emitted == point => Some(symbol),
                _ => None,
            })
            .chain(settled.iter().copied())
            .collect::<BTreeSet<_>>();
        if required.keys().any(|symbol| !released.contains(symbol)) {
            self.defer_resource(control);
        }
    }

    fn branch_release_matches(
        &self,
        control: ExpressionId,
        branch: usize,
        symbol: UnitSymbolId,
        state: &ValueState,
    ) -> bool {
        let Some(origins) = self
            .checker
            .branch_capture_releases
            .get(&(self.checker.unit_expression(control), branch))
            .and_then(|required| required.get(&symbol))
        else {
            return true;
        };
        origins.len() == 1
            && state
                .closures
                .get(&symbol)
                .is_some_and(|&closure| origins.contains(&self.checker.unit_expression(closure)))
    }

    fn settled_alternative_releases(
        &self,
        control: ExpressionId,
        entry: usize,
        states: &[ValueState],
        match_facts: &[PlannerDropFact],
    ) -> Vec<UnitSymbolId> {
        let Some(required) = self
            .checker
            .branch_capture_releases
            .get(&(self.checker.unit_expression(control), entry))
        else {
            return Vec::new();
        };
        let released = match_facts
            .iter()
            .filter_map(|fact| match (fact.point(), fact.target()) {
                (
                    PlannerDropPoint::WhenAlternativeMatch {
                        control: id,
                        entry: arm,
                        ..
                    },
                    PlannerDropTarget::Named(symbol),
                ) if id == control && arm == entry => Some(symbol),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        required
            .keys()
            .copied()
            .filter(|symbol| {
                released.contains(symbol)
                    && states.iter().all(|state| state.position(*symbol).is_none())
            })
            .collect()
    }

    /// Each condition is evaluated only on the preceding fallthrough path.
    pub(super) fn when_expression(
        &mut self,
        id: ExpressionId,
        subject: Option<ExpressionId>,
        entries: &[WhenEntry],
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        if let Some(subject) = subject
            && self.is_string_expression(subject)
        {
            return self.string_when(id, subject, entries, state);
        }
        let usage = self.control_result_usage(id);
        if let Some(subject) = subject {
            if !self.expression(subject, DropExpressionUse::Read, state)? {
                return Ok(false);
            }
            if self
                .checker
                .typed
                .nullable_when(self.checker.unit_expression(id))
                .is_some()
                && self.is_move_only_temporary(subject)
            {
                let origin = self.checker.parsed.ast().expressions().get(subject)?.span();
                self.register_pending_temporary(id, subject, origin, false, state)?;
            }
        }
        let mut remaining = Some(state.clone());
        let mut exits = Vec::new();
        let control = self.checker.unit_expression(id);
        let closed_after = self.checker.typed.when_closed_after(control);
        for (entry_index, entry) in entries
            .iter()
            .take(closed_after.unwrap_or(entries.len()))
            .enumerate()
        {
            let mut matched = Vec::new();
            let mut alternatives = Vec::new();
            if entry.else_span.is_some() {
                matched.extend(remaining.take());
            }
            let selector_count = self
                .checker
                .typed
                .when_final_selector_count(control, entry_index)
                .unwrap_or(entry.conditions.len());
            for (alternative, condition) in entry.conditions.iter().take(selector_count).enumerate()
            {
                let Some(mut input) = remaining.take() else {
                    break;
                };
                let (can_match, can_miss) = match condition {
                    WhenCondition::Expression(expression)
                    | WhenCondition::Contains { expression, .. } => {
                        if !self.expression(*expression, DropExpressionUse::Read, &mut input)? {
                            break;
                        }
                        if subject.is_none() {
                            self.checker.ordinary_condition_outcomes(*expression)
                        } else {
                            (true, true)
                        }
                    }
                    WhenCondition::TypeTest { .. } => (true, true),
                };
                if can_match {
                    matched.push(input.clone());
                    alternatives.push(alternative);
                }
                if can_miss {
                    remaining = Some(input);
                }
            }
            if matched.is_empty() {
                continue;
            }
            // Ownership checking has already forbidden a shared-body read of a moved owner.
            // Release only each incoming edge's exclusive ASAP obligations before joining.
            let common = matched[0]
                .values
                .iter()
                .filter(|value| {
                    matched
                        .iter()
                        .all(|edge| edge.position(value.symbol).is_some())
                })
                .map(|value| value.symbol)
                .collect::<std::collections::BTreeSet<_>>();
            let first_match_fact = self.facts.len();
            for (alternative, incoming) in alternatives.into_iter().zip(&mut matched) {
                let exclusive = incoming
                    .values
                    .iter()
                    .rev()
                    .filter(|value| !common.contains(&value.symbol))
                    .map(|value| value.symbol)
                    .collect::<Vec<_>>();
                for symbol in exclusive {
                    if !self.branch_release_matches(id, entry_index, symbol, incoming) {
                        self.defer_resource(id);
                        continue;
                    }
                    self.drop_named_asap(
                        PlannerDropPoint::WhenAlternativeMatch {
                            control: id,
                            entry: entry_index,
                            alternative,
                        },
                        symbol,
                        incoming,
                    );
                }
            }
            // Conditional resource obligations remain deferred, never silently intersected.
            let settled = self.settled_alternative_releases(
                id,
                entry_index,
                &matched,
                &self.facts[first_match_fact..],
            );
            let mut branch = self.merge_resource_states(id, matched);
            self.drop_branch_entry(id, entry_index, &settled, &mut branch);
            self.enter_root_branch_arm(id, entry_index);
            let evaluated = self.control_body(entry.body, usage, &mut branch);
            self.leave_root_branch_arm(id, entry_index);
            if evaluated? {
                self.drop_branch_exit(id, entry_index, &mut branch);
                self.finish_pending_temporaries(
                    id,
                    PlannerDropPoint::AfterExpression(id),
                    &mut branch,
                );
                exits.push(branch);
            }
        }
        if let Some(mut implicit) = remaining.filter(|_| closed_after.is_none()) {
            self.drop_branch_entry(id, entries.len(), &[], &mut implicit);
            self.drop_branch_exit(id, entries.len(), &mut implicit);
            self.finish_pending_temporaries(
                id,
                PlannerDropPoint::AfterExpression(id),
                &mut implicit,
            );
            exits.push(implicit);
        }
        let continues = !exits.is_empty();
        *state = self.merge_resource_states(id, exits);
        Ok(continues)
    }

    /// 让 control result 的内部 owner 交付只作用于分支 body 的最后一个表达式。
    pub(super) fn control_body(
        &mut self,
        id: StatementId,
        usage: DropExpressionUse,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        Ok(self.evaluate_control_body(id, usage, state)?.1.continues)
    }

    /// Preserve the actual tail expression and its evaluation-frame token. This does
    /// not admit result-valued control flow or add an owner/choice definition.
    /// Callers must still prove delivery before interpreting the optional token.
    pub(super) fn evaluate_control_body(
        &mut self,
        id: StatementId,
        usage: DropExpressionUse,
        state: &mut ValueState,
    ) -> Result<(Option<ExpressionId>, EvaluatedOwner), OwnershipCheckingError> {
        // A sealed branch arm returns Unit and closes its loan at a statement,
        // not at an implicit value-producing tail. Run actual statement End hooks;
        // the final independent planner End is checked before publishing the pair.
        if self
            .checker
            .no_carry_proofs
            .iter()
            .any(|proof| matches!(proof, crate::ownership_checking::loop_no_carry::Proof::Branch(_, arms) | crate::ownership_checking::loop_no_carry::Proof::CounterBranch(_, arms, _) | crate::ownership_checking::loop_no_carry::Proof::BreakBranch(_, arms, _) | crate::ownership_checking::loop_no_carry::Proof::CounterBreakBranch(_, arms, _, _) if arms.iter().any(|arm| arm.body == id)))
        {
            return Ok((
                None,
                EvaluatedOwner {
                    replace_consumer: None,
                    continues: self.statement(id, state)?,
                    owner: None,
                },
            ));
        }
        let payload = self
            .checker
            .parsed
            .ast()
            .statements()
            .get(id)?
            .payload()
            .clone();
        let elements = match payload {
            Statement::ControlBody { elements } => elements,
            Statement::Expression { expression } => {
                return self.control_tail(expression, usage, state);
            }
            _ => {
                return Ok((
                    None,
                    EvaluatedOwner {
                        replace_consumer: None,
                        continues: self.statement(id, state)?,
                        owner: None,
                    },
                ));
            }
        };
        self.scope_depth += 1;
        let frame = self.scope_depth;
        let Some((&tail, prefix)) = elements.split_last() else {
            self.scope_depth -= 1;
            return Ok((
                None,
                EvaluatedOwner {
                    replace_consumer: None,
                    continues: true,
                    owner: None,
                },
            ));
        };
        for &element in prefix {
            if !self.statement(element, state)? {
                self.scope_depth -= 1;
                return Ok((
                    None,
                    EvaluatedOwner {
                        replace_consumer: None,
                        continues: false,
                        owner: None,
                    },
                ));
            }
        }
        let tail_node = self.checker.parsed.ast().statements().get(tail)?;
        let result = match tail_node.payload() {
            Statement::Expression { expression } => self.control_tail(*expression, usage, state)?,
            _ => (
                None,
                EvaluatedOwner {
                    replace_consumer: None,
                    continues: self.statement(tail, state)?,
                    owner: None,
                },
            ),
        };
        if result.1.continues {
            self.drop_scope(frame, PlannerDropPoint::AfterStatement(id), state);
        }
        self.scope_depth -= 1;
        Ok(result)
    }

    fn control_tail(
        &mut self,
        expression: ExpressionId,
        usage: DropExpressionUse,
        state: &mut ValueState,
    ) -> Result<(Option<ExpressionId>, EvaluatedOwner), OwnershipCheckingError> {
        let mut result = self.evaluate_owner_expression(expression, usage, state)?;
        if result.continues
            && usage == DropExpressionUse::Read
            && self.is_move_only_temporary(expression)
        {
            let span = self
                .checker
                .parsed
                .ast()
                .expressions()
                .get(expression)?
                .span();
            self.push_fact(PlannerDropFact::new(
                PlannerDropPoint::AfterExpression(expression),
                PlannerDropTarget::Temporary(expression),
                span,
            ));
        }
        // Read does not deliver an owner, including a temporary just cleaned above.
        if usage != DropExpressionUse::Consume {
            result.owner = None;
        }
        Ok((Some(expression), result))
    }

    pub(super) fn control_result_usage(&self, expression: ExpressionId) -> DropExpressionUse {
        if self.is_move_only_temporary(expression) {
            DropExpressionUse::Consume
        } else {
            DropExpressionUse::Read
        }
    }
}
