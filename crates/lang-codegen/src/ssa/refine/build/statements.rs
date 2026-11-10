//! The existing slot-statement producers, with optional loan verification tags.
use super::*;
use crate::ssa::refine::body::RStmt;

impl Builder<'_, '_> {
    pub(super) fn push(
        &mut self,
        op: ROp,
        results: Vec<Slot>,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.sync_dead();
        let block = self.current(span)?;
        let loan_tag = self.body.loan_plans.statement(&op, &results, block, span)?;
        self.body.blocks[block.0].statements.push(RStmt {
            op,
            results,
            span,
            loan_tag,
        });
        Ok(())
    }

    /// Appends `op` producing one fresh slot of `ty`.
    pub(super) fn value(
        &mut self,
        op: ROp,
        ty: EntityType,
        span: Span,
    ) -> Result<Slot, LoweringError> {
        let slot = self.body.slot(ty);
        self.push(op, vec![slot], span)?;
        Ok(slot)
    }
}
