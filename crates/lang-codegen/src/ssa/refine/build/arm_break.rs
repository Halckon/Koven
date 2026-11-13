//! Observe only the selected actual break, before and after its real cleanup.
use super::branch_no_carry::BranchLease;
use super::*;
pub(super) struct ArmBreak {
    lease: BranchLease,
    expression: UnitExpressionId,
    from: BlockRef,
    target: BlockRef,
}
impl Builder<'_, '_> {
    pub(super) fn arm_break_entry(
        &self,
        expression: ExpressionId,
        is_break: bool,
        frame: LoopFrame,
        span: Span,
    ) -> Result<Option<ArmBreak>, LoweringError> {
        let id = self.unit(expression);
        let Some(p) = self.body.arm_loop_plans.loops.iter().find(|p| {
            p.fact.arms()[0].loop_statement() == frame.statement && p.tail_break.is_some()
        }) else {
            return Ok(None);
        };
        let b = p
            .tail_break
            .as_ref()
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?;
        let from = self.current(span)?;
        if !is_break
            || b.expression != id
            || b.observed.is_some()
            || p.join != Some(from)
            || p.blocks.map(|b| b[4]) != Some(frame.exit)
            || frame.seq_for.is_some()
            || frame.map_for.is_some()
            || frame.loans != self.pending_call_loans.len()
            || frame.views != self.active_views.len()
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        let lease = self
            .arm_loop_lease(frame.statement.statement(), span)?
            .ok_or_else(|| error(LoweringErrorKind::InvalidModel, span))?;
        Ok(Some(ArmBreak {
            lease,
            expression: id,
            from,
            target: frame.exit,
        }))
    }
    pub(super) fn arm_break_emitted(
        &mut self,
        token: Option<ArmBreak>,
        target: BlockRef,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(t) = token else { return Ok(()) };
        self.arm_boundary(Some(&t.lease), span)?;
        if target != t.target
            || !matches!(self.body.blocks[t.from.0].terminator.as_ref().map(|t|&t.0),Some(RTerm::Goto(to)) if *to==target)
        {
            return Err(error(LoweringErrorKind::InvalidModel, span));
        }
        self.body
            .arm_loop_plans
            .record_break(t.expression, t.from, target, span)
    }
}
