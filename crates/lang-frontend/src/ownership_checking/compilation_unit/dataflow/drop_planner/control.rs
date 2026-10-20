//! Control expression 分支 tail 的 usage 与析构规划。

use crate::{
    ast::{ExpressionId, StatementId},
    ownership_checking::OwnershipCheckingError,
    parser::{Statement, WhenCondition, WhenEntry},
};

use super::{
    DropExpressionUse, DropPlanner, PlannerDropFact, PlannerDropPoint, PlannerDropTarget,
    ValueState,
};

impl DropPlanner<'_, '_> {
    /// Each condition is evaluated only on the preceding fallthrough path.
    pub(super) fn when_expression(
        &mut self,
        id: ExpressionId,
        subject: Option<ExpressionId>,
        entries: &[WhenEntry],
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let usage = self.control_result_usage(id);
        if let Some(subject) = subject
            && !self.expression(subject, DropExpressionUse::Read, state)?
        {
            return Ok(false);
        }
        let mut remaining = Some(state.clone());
        let mut exits = Vec::new();
        for (entry_index, entry) in entries.iter().enumerate() {
            let mut matched = Vec::new();
            let mut alternatives = Vec::new();
            if entry.else_span.is_some() {
                matched.extend(remaining.take());
            }
            for (alternative, condition) in entry.conditions.iter().enumerate() {
                let Some(mut input) = remaining.take() else {
                    break;
                };
                if let WhenCondition::Expression(expression)
                | WhenCondition::Contains { expression, .. } = condition
                    && !self.expression(*expression, DropExpressionUse::Read, &mut input)?
                {
                    break;
                }
                matched.push(input.clone());
                alternatives.push(alternative);
                remaining = Some(input);
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
            for (alternative, incoming) in alternatives.into_iter().zip(&mut matched) {
                let exclusive = incoming
                    .values
                    .iter()
                    .rev()
                    .filter(|value| !common.contains(&value.symbol))
                    .map(|value| value.symbol)
                    .collect::<Vec<_>>();
                for symbol in exclusive {
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
            let mut branch = self.merge_resource_states(id, matched);
            if self.control_body(entry.body, usage, &mut branch)? {
                self.drop_branch_exit(id, entry_index, &mut branch);
                exits.push(branch);
            }
        }
        if let Some(mut implicit) = remaining {
            self.drop_branch_exit(id, entries.len(), &mut implicit);
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
            _ => return self.statement(id, state),
        };
        self.scope_depth += 1;
        let frame = self.scope_depth;
        let Some((&tail, prefix)) = elements.split_last() else {
            self.scope_depth -= 1;
            return Ok(true);
        };
        for &element in prefix {
            if !self.statement(element, state)? {
                self.scope_depth -= 1;
                return Ok(false);
            }
        }
        let tail_node = self.checker.parsed.ast().statements().get(tail)?;
        let continues = match tail_node.payload() {
            Statement::Expression { expression } => self.control_tail(*expression, usage, state)?,
            _ => self.statement(tail, state)?,
        };
        if continues {
            self.drop_scope(frame, PlannerDropPoint::AfterStatement(id), state);
        }
        self.scope_depth -= 1;
        Ok(continues)
    }

    fn control_tail(
        &mut self,
        expression: ExpressionId,
        usage: DropExpressionUse,
        state: &mut ValueState,
    ) -> Result<bool, OwnershipCheckingError> {
        let continues = self.expression(expression, usage, state)?;
        if continues && usage == DropExpressionUse::Read && self.is_move_only_temporary(expression)
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
        Ok(continues)
    }

    pub(super) fn control_result_usage(&self, expression: ExpressionId) -> DropExpressionUse {
        if self.is_move_only_temporary(expression) {
            DropExpressionUse::Consume
        } else {
            DropExpressionUse::Read
        }
    }
}
