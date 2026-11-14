//! Independent break capability selection and actual emitted exit capture.
use super::*;
use lang_frontend::{ownership_checking::UnitLoopNoCarryExitKind, type_checking::UnitStatementId};
#[derive(Clone, Debug)]
pub(in crate::ssa::refine) struct BreakPlan {
    pub(in crate::ssa::refine) fact: UnitLoopBreakBranchNoCarryFact,
    pub(in crate::ssa::refine) expression: UnitExpressionId,
    pub(in crate::ssa::refine) span: Span,
    pub(in crate::ssa::refine) observed: Option<(BlockRef, BlockRef)>,
}
impl Plans {
    pub(in crate::ssa::refine) fn required_breaks(
        inputs: &RefineInputs<'_>,
    ) -> Vec<UnitLoopBreakBranchNoCarryFact> {
        inputs
            .owned
            .no_carry_break_branches()
            .iter()
            .filter(|b| {
                Some(b.pair().arms()[0].callable().item()) == inputs.function_item
                    && b.pair().branch().source_unit() == inputs.source_unit
            })
            .cloned()
            .collect()
    }
    pub(super) fn break_plan(
        inputs: &RefineInputs<'_>,
        pair: &UnitLoopBranchNoCarryFact,
    ) -> Result<Option<BreakPlan>, LoweringError> {
        let mut found = inputs
            .owned
            .no_carry_break_branches()
            .iter()
            .filter(|b| b.pair().same_identity(pair));
        let Some(fact) = found.next() else {
            return Ok(None);
        };
        let fail = || invalid(inputs.span);
        if found.next().is_some()
            || !inputs.owned.owns_no_carry_break_branch(fact)
            || fact.exit_kind() != UnitLoopNoCarryExitKind::Break
            || fact.exit_target() != pair.arms()[0].loop_statement()
            || fact.cleanup_point() != UnitDropPoint::ControlTransfer(fact.break_expression())
            || fact.exit_cleanup_point() != UnitDropPoint::LoopExit(fact.exit_target())
        {
            return Err(fail());
        }
        let node = inputs
            .parsed
            .ast()
            .expressions()
            .get(fact.break_expression().expression())
            .map_err(|_| fail())?;
        let statement = inputs
            .parsed
            .ast()
            .statements()
            .get(fact.break_statement().statement())
            .map_err(|_| fail())?;
        let loop_node = inputs
            .parsed
            .ast()
            .statements()
            .get(fact.exit_target().statement())
            .map_err(|_| fail())?;
        if !matches!(node.payload(), Expression::Break { .. })
            || !matches!(statement.payload(),Statement::Expression{expression} if *expression==fact.break_expression().expression())
            || !matches!(loop_node.payload(),Statement::While{body,..} if *body==fact.loop_body().statement())
        {
            return Err(fail());
        }
        Ok(Some(BreakPlan {
            fact: fact.clone(),
            expression: fact.break_expression(),
            span: node.span(),
            observed: None,
        }))
    }
    pub(in crate::ssa::refine) fn completed_break(&self, statement: UnitStatementId) -> bool {
        self.loops.iter().any(|p| {
            p.fact.arms()[0].loop_statement() == statement
                && p.tail_break.as_ref().is_some_and(|b| b.observed.is_some())
        })
    }
    pub(in crate::ssa::refine) fn record_break(
        &mut self,
        expression: UnitExpressionId,
        from: BlockRef,
        to: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        let mut plans = self.loops.iter_mut().filter(|p| {
            p.tail_break
                .as_ref()
                .is_some_and(|b| b.expression == expression)
        });
        let p = plans.next().ok_or_else(|| invalid(span))?;
        if plans.next().is_some() || p.join != Some(from) || p.blocks.map(|b| b[4]) != Some(to) {
            return Err(invalid(span));
        }
        let b = p.tail_break.as_mut().ok_or_else(|| invalid(span))?;
        if b.observed.replace((from, to)).is_some() || b.span != span {
            return Err(invalid(span));
        }
        Ok(())
    }
}
impl Emitted {
    pub(super) fn check_break_required(
        &self,
        body: &RefinedBody,
        span: Span,
    ) -> Result<(), LoweringError> {
        let actual: Vec<_> = body
            .arm_loop_plans
            .loops
            .iter()
            .filter_map(|p| p.tail_break.as_ref().map(|b| &b.fact))
            .collect();
        if actual.len() != body.arm_break_required.len()
            || actual
                .iter()
                .zip(&body.arm_break_required)
                .any(|(a, b)| !a.same_identity(b))
            || body
                .arm_loop_plans
                .loops
                .iter()
                .any(|p| p.tail_break.is_some() && p.counter.is_some())
        {
            return Err(invalid(span));
        }
        Ok(())
    }
}
