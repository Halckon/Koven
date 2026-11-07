//! 循环回边的保守失效。while 条件每轮检查，for source 在进入前已求值。
use super::{flow::FlowKey, *};
use crate::type_checking::flow_effects::{
    collect_expression_mutation_targets, collect_statement_mutation_targets,
};

impl Checker<'_> {
    pub(super) fn expression_flow_writes(
        &self,
        expression: ExpressionId,
    ) -> Result<BTreeSet<FlowKey>, TypeCheckingError> {
        Ok(collect_expression_mutation_targets(self.ast(), expression)?
            .into_iter()
            .filter_map(|target| self.stable_flow_key(target))
            .collect())
    }

    pub(super) fn check_repeating_flow(
        &mut self,
        condition: Option<ExpressionId>,
        body: StatementId,
    ) -> Result<(), TypeCheckingError> {
        let mut writes = collect_statement_mutation_targets(self.ast(), body)?
            .into_iter()
            .filter_map(|target| self.stable_flow_key(target))
            .collect::<BTreeSet<_>>();
        if let Some(condition) = condition {
            writes.extend(self.expression_flow_writes(condition)?);
        }
        for key in writes {
            self.flow_facts.remove(&key);
            *self.flow_versions.entry(key).or_default() += 1;
        }
        let head = self.flow_facts.clone();
        if let Some(condition) = condition {
            let boolean = self.builtin(BuiltinType::Boolean);
            self.check_expression(condition, Some(boolean), None)?;
            let (true_facts, _) = self.condition_facts(condition)?;
            self.flow_facts = flow::extend_facts(&self.flow_facts, &true_facts);
        }
        let result = self.check_loop_body(body);
        self.flow_facts = head;
        result.map(|_| ())
    }
}
