//! SPEC-0330：与 single 同一保守 while / 潜在写入合同。
use super::{flow::FlowKey, *};
use crate::type_checking::flow_effects::{
    collect_expression_mutation_targets, collect_statement_mutation_targets,
};

impl BodyChecker<'_> {
    pub(super) fn expression_flow_writes(
        &self,
        source: SourceUnitId,
        expression: ExpressionId,
    ) -> Result<BTreeSet<FlowKey>, CompilationUnitTypeError> {
        Ok(
            collect_expression_mutation_targets(self.file(source).ast(), expression)
                .map_err(TypeCheckingError::from)?
                .into_iter()
                .filter_map(|target| self.stable_flow_key(source, target))
                .collect(),
        )
    }

    pub(super) fn check_while_flow(
        &mut self,
        source: SourceUnitId,
        condition: ExpressionId,
        body: StatementId,
        return_type: UnitTypeId,
        return_span: Option<Span>,
    ) -> Result<(), CompilationUnitTypeError> {
        let mut writes = self.expression_flow_writes(source, condition)?;
        writes.extend(
            collect_statement_mutation_targets(self.file(source).ast(), body)
                .map_err(TypeCheckingError::from)?
                .into_iter()
                .filter_map(|target| self.stable_flow_key(source, target)),
        );
        for key in writes {
            self.invalidate_flow_key(key);
        }
        let head = self.flow_facts.clone();
        let head_versions = self.flow_versions.clone();
        let boolean = self.builtin(BuiltinType::Boolean);
        self.check_expression(source, condition, Some(boolean), None, return_type)?;
        let (true_facts, _) = self.condition_facts(source, condition)?;
        self.flow_facts = flow::extend_facts(&self.flow_facts, &true_facts);
        let result = self.check_loop_body(source, body, return_type, return_span);
        self.flow_facts = head;
        self.flow_versions = flow::merge_flow_versions(&head_versions, &self.flow_versions);
        result.map(|_| ())
    }
}
