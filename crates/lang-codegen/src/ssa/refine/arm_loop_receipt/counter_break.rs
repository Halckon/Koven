//! Independently owned conditional exit, captured from actual lowering sites.
use super::*;
#[derive(Clone, Debug)]
pub(in crate::ssa::refine) struct CounterBreak {
    pub(in crate::ssa::refine) fact: UnitLoopCounterBreakBranchNoCarryFact,
    pub(super) role: usize,
    pub(super) condition_span: Span,
    pub(in crate::ssa::refine) break_span: Span,
    pub(super) used: Option<(BlockRef, Slot)>,
    pub(in crate::ssa::refine) split: Option<[BlockRef; 3]>,
    pub(super) continued: Option<BlockRef>,
    pub(in crate::ssa::refine) observed: Option<(BlockRef, BlockRef)>,
}
impl CounterBreak {
    pub(super) fn split(
        &mut self,
        id: UnitExpressionId,
        blocks: [BlockRef; 3],
        join: Option<BlockRef>,
        s: Span,
    ) -> Result<(), LoweringError> {
        if id != self.fact.break_control() {
            return Ok(());
        }
        if self.split.is_some()
            || join != Some(blocks[0])
            || blocks
                .iter()
                .enumerate()
                .any(|(i, b)| blocks[..i].contains(b))
        {
            return Err(invalid(s));
        }
        self.split = Some(blocks);
        Ok(())
    }
    pub(super) fn join(
        &mut self,
        id: UnitExpressionId,
        b: BlockRef,
        preds: &[BlockRef],
        s: Span,
    ) -> Result<(), LoweringError> {
        if id != self.fact.break_control() {
            return Ok(());
        }
        let blocks = self.split.ok_or_else(|| invalid(s))?;
        if self.continued.is_some() || preds != [blocks[2]] || self.observed.is_none() {
            return Err(invalid(s));
        }
        self.continued = Some(b);
        Ok(())
    }
    pub(super) fn condition(
        &mut self,
        id: UnitExpressionId,
        b: BlockRef,
        value: Slot,
        join: Option<BlockRef>,
        s: Span,
    ) -> Result<(), LoweringError> {
        if id != self.fact.break_condition() {
            return Ok(());
        }
        if self.used.is_some() || join != Some(b) {
            return Err(invalid(s));
        }
        self.used = Some((b, value));
        Ok(())
    }
    pub(super) fn fold(&mut self, block: BlockRef, successor: BlockRef) {
        let remap = |b: &mut BlockRef| {
            if *b == successor {
                *b = block
            }
        };
        if let Some(bs) = &mut self.split {
            for b in bs {
                remap(b)
            }
        }
        if let Some(b) = &mut self.continued {
            remap(b)
        }
        if let Some((b, _)) = &mut self.used {
            remap(b)
        }
        if let Some((a, b)) = &mut self.observed {
            remap(a);
            remap(b)
        }
    }
}
impl Plans {
    pub(in crate::ssa::refine) fn required_counter_breaks(
        inputs: &RefineInputs<'_>,
    ) -> Vec<UnitLoopCounterBreakBranchNoCarryFact> {
        crate::ssa::borrow_result_support::branch_no_carry::counter_breaks(inputs.owned)
            .filter(|b| {
                Some(b.counter().pair().arms()[0].callable().item()) == inputs.function_item
                    && b.counter().pair().branch().source_unit() == inputs.source_unit
            })
            .cloned()
            .collect()
    }
    pub(super) fn counter_break_plan(
        inputs: &RefineInputs<'_>,
        pair: &UnitLoopBranchNoCarryFact,
    ) -> Result<Option<CounterBreak>, LoweringError> {
        let mut matches =
            crate::ssa::borrow_result_support::branch_no_carry::counter_breaks(inputs.owned)
                .filter(|b| b.counter().pair().same_identity(pair));
        let Some(fact) = matches.next() else {
            return Ok(None);
        };
        let fail = || invalid(inputs.span);
        if matches.next().is_some()
            || !crate::ssa::borrow_result_support::branch_no_carry::owns_counter_break(
                inputs.owned,
                fact,
            )
            || fact.target_loop() != pair.arms()[0].loop_statement()
            || fact.cleanup_point() != UnitDropPoint::ControlTransfer(fact.break_expression())
            || fact.exit_cleanup_point() != UnitDropPoint::LoopExit(fact.target_loop())
        {
            return Err(fail());
        }
        let node = |id: UnitExpressionId| {
            inputs
                .parsed
                .ast()
                .expressions()
                .get(id.expression())
                .map_err(|_| fail())
        };
        let control = node(fact.break_control())?;
        if !matches!(control.payload(),Expression::If{condition,then_branch,else_branch:None,..} if *condition==fact.break_condition().expression() && *then_branch==fact.break_body().statement())
            || !matches!(
                node(fact.break_expression())?.payload(),
                Expression::Break { .. }
            )
        {
            return Err(fail());
        }
        let condition = node(fact.break_condition())?;
        if !matches!(condition.payload(), Expression::Name)
            || inputs
                .references
                .get(&(condition.span().start(), condition.span().end()))
                != Some(&fact.break_flag())
        {
            return Err(fail());
        }
        let role = inputs
            .parameters
            .iter()
            .position(|p| *p == fact.break_flag())
            .ok_or_else(fail)?;
        if !matches!(
            inputs.modes.get(role),
            Some(ParameterMode::Borrow | ParameterMode::Value)
        ) {
            return Err(fail());
        }
        Ok(Some(CounterBreak {
            fact: fact.clone(),
            role,
            condition_span: condition.span(),
            break_span: node(fact.break_expression())?.span(),
            used: None,
            split: None,
            continued: None,
            observed: None,
        }))
    }
}
impl Emitted {
    pub(super) fn check_counter_break_required(
        &self,
        body: &RefinedBody,
        s: Span,
    ) -> Result<(), LoweringError> {
        let actual: Vec<_> = body
            .arm_loop_plans
            .loops
            .iter()
            .filter_map(|p| p.counter_break.as_ref())
            .collect();
        if actual.len() != body.arm_counter_break_required.len()
            || actual
                .iter()
                .zip(&body.arm_counter_break_required)
                .any(|(a, b)| !a.fact.same_identity(b))
            || body.arm_loop_plans.loops.iter().any(|p| {
                p.counter_break.is_some()
                    && (p.counter.is_none()
                        || p.counter_fact.is_some()
                        || p.tail_break.is_some()
                        || p.prefix
                            .as_ref()
                            .is_some_and(|prefix| !prefix.is_counter_composition()))
            })
        {
            return Err(invalid(s));
        }
        for b in actual {
            if b.used.is_none()
                || b.split.is_none()
                || b.continued.is_none()
                || b.observed.is_none()
            {
                return Err(invalid(s));
            }
        }
        Ok(())
    }
}
