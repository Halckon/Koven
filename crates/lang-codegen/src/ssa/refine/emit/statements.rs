//! Mechanical statement emission, including aliases and optional production receipts.
use super::*;
use crate::ssa::refine::loan_receipt::Emitted;

impl Emitter<'_> {
    pub(super) fn statements(
        &mut self,
        body: &RefinedBody,
        block: BlockRef,
        target: BlockId,
        receipt: &mut Emitted,
        receiver_receipt: &mut crate::ssa::refine::receiver_receipt::Emitted,
        named_receipt: &mut crate::ssa::refine::named_receipt::Emitted,
    ) -> Result<(), LoweringError> {
        for statement in &body.blocks[block.0].statements {
            if let (ROp::Alias { source }, [result]) = (&statement.op, statement.results.as_slice())
            {
                if statement.loan_tag.is_some() {
                    return Err(error(LoweringErrorKind::InvalidModel, statement.span));
                }
                let entity = self.get(*source, statement.span)?;
                self.current.insert(*result, entity);
                self.defined.insert(*result, entity);
                continue;
            }
            let operation = self.operation(&statement.op, statement.span)?;
            let types = statement
                .results
                .iter()
                .map(|slot| body.slots[slot.0])
                .collect();
            let (instruction, results) = self
                .function
                .append_instruction(target, operation, types, Origin::Source(statement.span))
                .map_err(|_| error(LoweringErrorKind::InvalidModel, statement.span))?;
            for (slot, entity) in statement.results.iter().zip(results) {
                self.current.insert(*slot, entity);
                self.defined.insert(*slot, entity);
            }
            match statement.loan_tag {
                Some(crate::ssa::refine::body::LoanTag::Consumer(tag)) => receipt.statement(
                    &body.loan_plans,
                    tag,
                    self.function,
                    instruction,
                    &self.current,
                    statement.span,
                )?,
                Some(crate::ssa::refine::body::LoanTag::Receiver(tag)) => receiver_receipt
                    .statement(
                        &body.receiver_plans,
                        tag,
                        self.function,
                        instruction,
                        &self.current,
                        statement.span,
                    )?,
                Some(crate::ssa::refine::body::LoanTag::Named(tag)) => named_receipt.statement(
                    &body.named_plans,
                    tag,
                    self.function,
                    instruction,
                    &self.current,
                    statement.span,
                )?,
                None => {}
            }
        }
        Ok(())
    }
}
