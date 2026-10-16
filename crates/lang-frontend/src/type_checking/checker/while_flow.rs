//! SPEC-0330：while 的头部保守失效与条件 true facts；不推导出口新事实。
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

    pub(super) fn check_while_flow(
        &mut self,
        condition: ExpressionId,
        body: StatementId,
    ) -> Result<(), TypeCheckingError> {
        let mut writes = self.expression_flow_writes(condition)?;
        writes.extend(
            collect_statement_mutation_targets(self.ast(), body)?
                .into_iter()
                .filter_map(|target| self.stable_flow_key(target)),
        );
        let mut head = self.flow_facts.clone();
        head.retain(|key, _| !writes.contains(key));
        self.flow_facts = head.clone();
        let boolean = self.builtin(BuiltinType::Boolean);
        self.check_expression(condition, Some(boolean), None)?;
        let (true_facts, _) = self.condition_facts(condition)?;
        self.flow_facts = flow::extend_facts(&self.flow_facts, &true_facts);
        let result = self.check_loop_body(body);
        self.flow_facts = head;
        result.map(|_| ())
    }
}
