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
            if let Some(tag) = statement.loan_tag {
                receipt.statement(
                    &body.loan_plans,
                    tag,
                    self.function,
                    instruction,
                    &self.current,
                    statement.span,
                )?;
            }
        }
        Ok(())
    }
}
