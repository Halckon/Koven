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
        let named = self
            .body
            .named_plans
            .statement(&op, &results, block, span)?;
        let temporary = self
            .body
            .temporary_plans
            .statement(&op, &results, block, span)?;
        let binding = self
            .body
            .binding_plans
            .statement(&op, &results, block, span)?;
        let child = self
            .body
            .child_plans
            .statement(&op, &results, block, span)?;
        let loan_tag = match (consumer, receiver, named, temporary, binding, child) {
            (Some(tag), None, None, None, None, None) => Some(LoanTag::Consumer(tag)),
            (None, Some(tag), None, None, None, None) => Some(LoanTag::Receiver(tag)),
            (None, None, Some(tag), None, None, None) => Some(LoanTag::Named(tag)),
            (None, None, None, Some(tag), None, None) => Some(LoanTag::Temporary(tag)),
            (None, None, None, None, Some(tag), None) => Some(LoanTag::Binding(tag)),
            (None, None, None, None, None, Some(tag)) => Some(LoanTag::Child(tag)),
            (None, None, None, None, None, None) => None,
            _ => return Err(error(LoweringErrorKind::InvalidModel, span)),
        };
        let forward = self
            .body
            .forward_plans
            .statement(&op, &results, block, span)?;
        let loan_tag = match (loan_tag, forward) {
            (None, Some(tag)) => Some(LoanTag::Forward(tag)),
            (tag, None) => tag,
            _ => return Err(error(LoweringErrorKind::InvalidModel, span)),
        };
        let branch = self
            .body
            .branch_plans
            .statement(&op, &results, block, span)?;
        let loan_tag = match (loan_tag, branch) {
            (None, Some(tag)) => Some(LoanTag::Branch(tag)),
            (tag, None) => tag,
            _ => return Err(error(LoweringErrorKind::InvalidModel, span)),
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

impl Builder<'_, '_> {
    pub(super) fn finish_loan_receipts(&self) -> Result<(), LoweringError> {
        self.body.forward_plans.finish(self.inputs.span)?;
        self.body.branch_plans.finish(self.inputs.span)?;
        self.body
            .loan_plans
            .finish(&self.consumed_drops, self.inputs.span)?;
        self.body
            .receiver_plans
            .finish(&self.consumed_drops, self.inputs.span)?;
        self.body
            .named_plans
            .finish(&self.consumed_drops, self.inputs.span)?;
        self.body
            .temporary_plans
            .finish(&self.consumed_drops, self.inputs.span)?;
        self.body
            .binding_plans
            .finish(&self.consumed_drops, self.inputs.span)?;
        self.body
            .child_plans
            .finish(&self.consumed_drops, self.inputs.span)
    }
    pub(super) fn authorize_loan_drops(
        &mut self,
        index: usize,
        target: UnitDropTarget,
        slot: Slot,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.body.loan_plans.drop_fact(index, target, slot, span)?;
        self.body
            .receiver_plans
            .drop_fact(index, target, slot, span)?;
        self.body.named_plans.drop_fact(index, target, slot, span)?;
        self.body
            .temporary_plans
            .drop_fact(index, target, slot, span)?;
        self.body
            .binding_plans
            .drop_fact(index, target, slot, span)?;
        self.body.child_plans.drop_fact(index, target, slot, span)
    }
}
