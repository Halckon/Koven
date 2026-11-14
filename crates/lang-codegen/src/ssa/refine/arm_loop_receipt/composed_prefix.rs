//! Composition identity is independently required, selected and checked after publication.
use super::*;
use lang_frontend::ownership_checking::UnitLoopPrefixCounterBreakFact;
impl Plans {
    pub(in crate::ssa::refine) fn required_composed_prefixes(
        inputs: &RefineInputs<'_>,
    ) -> Vec<UnitLoopPrefixCounterBreakFact> {
        inputs
            .owned
            .no_carry_prefix_counter_breaks()
            .iter()
            .filter(|f| {
                Some(
                    f.counter_break().counter().pair().arms()[0]
                        .callable()
                        .item(),
                ) == inputs.function_item
                    && f.prefix().source_unit() == inputs.source_unit
            })
            .cloned()
            .collect()
    }
}
impl Emitted {
    pub(super) fn check_composed_prefix_required(
        &self,
        body: &RefinedBody,
        s: Span,
    ) -> Result<(), LoweringError> {
        let actual: Vec<_> = body
            .arm_loop_plans
            .loops
            .iter()
            .filter_map(|p| {
                p.prefix.as_ref().and_then(|p| match &p.fact {
                    prefix_fact::PrefixFact::Counter(f) => Some(f),
                    _ => None,
                })
            })
            .collect();
        if actual.len() != body.arm_composed_prefix_required.len()
            || actual
                .iter()
                .zip(&body.arm_composed_prefix_required)
                .any(|(a, b)| !a.same_identity(b))
        {
            return Err(invalid(s));
        }
        Ok(())
    }
}
