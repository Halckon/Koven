//! The existing slot-statement producers, with optional loan verification tags.
use super::*;
use crate::ssa::refine::body::{LoanTag, RStmt};

impl Builder<'_, '_> {
    pub(super) fn push(
        &mut self,
        op: ROp,
        results: Vec<Slot>,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.sync_dead();
        let block = self.current(span)?;
        let consumer = self.body.loan_plans.statement(&op, &results, block, span)?;
        let receiver = self
            .body
            .receiver_plans
            .statement(&op, &results, block, span)?;
        let loan_tag = match (consumer, receiver) {
            (Some(tag), None) => Some(LoanTag::Consumer(tag)),
            (None, Some(tag)) => Some(LoanTag::Receiver(tag)),
            (None, None) => None,
            (Some(_), Some(_)) => return Err(error(LoweringErrorKind::InvalidModel, span)),
        };
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
