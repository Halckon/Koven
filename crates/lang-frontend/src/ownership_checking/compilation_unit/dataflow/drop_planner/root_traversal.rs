//! Original source-root and visited-lambda traversal; opt-in observation does not change execution.
use super::*;

impl DropPlanner<'_, '_> {
    pub(super) fn run(mut self) -> Result<Self, OwnershipCheckingError> {
        for &root in self.checker.parsed.roots() {
            #[cfg(any(test, feature = "d10-catalog-experiment"))]
            let trace_start = self.origin_probe.as_ref().map(Vec::len);
            self.item(root)?;
            #[cfg(any(test, feature = "d10-catalog-experiment"))]
            if let (Some(ranges), Some(start), Some(events)) =
                (&mut self.root_trace_ranges, trace_start, &self.origin_probe)
            {
                ranges.push((
                    crate::type_checking::UnitItemId::new(self.checker.source_unit, root),
                    start..events.len(),
                ));
            }
        }
        for (lambda, node) in self.checker.parsed.ast().expressions().iter() {
            let Expression::Lambda {
                opener_span,
                parameters,
                arrow_span,
                body,
                ..
            } = node.payload()
            else {
                continue;
            };
            if !self
                .checker
                .visited_lambdas
                .contains(&self.checker.unit_expression(lambda))
            {
                continue;
            }
            let mut effective_parameters = parameters.clone();
            if arrow_span.is_none()
                && let Some(symbol) = self.checker.symbols_by_span.get(&span_key(*opener_span))
                && self.checker.typed.body_parameter_mode(*symbol).is_some()
            {
                effective_parameters.push(*opener_span);
            }
            self.plan_lambda_body(lambda, &effective_parameters, *body)?;
        }
        Ok(self)
    }
}
