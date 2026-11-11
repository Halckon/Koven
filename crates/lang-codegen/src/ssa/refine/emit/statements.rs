//! Mechanical statement emission, including aliases and optional production receipts.
use super::*;
/// Each protocol retains its own plans, role identities and final receipt.
pub(super) struct LoanReceipts {
    consumer: crate::ssa::refine::loan_receipt::Emitted,
    receiver: crate::ssa::refine::receiver_receipt::Emitted,
    named: crate::ssa::refine::named_receipt::Emitted,
    temporary: crate::ssa::refine::temporary_receipt::Emitted,
    binding: crate::ssa::refine::binding_receipt::Emitted,
    child: crate::ssa::refine::child_receipt::Emitted,
}
impl LoanReceipts {
    pub(super) fn new(
        body: &RefinedBody,
        function: &Function,
        span: Span,
    ) -> Result<Self, LoweringError> {
        Ok(Self {
            child: crate::ssa::refine::child_receipt::Emitted::new(
                &body.child_plans,
                function,
                span,
            )?,
            consumer: crate::ssa::refine::loan_receipt::Emitted::new(
                &body.loan_plans,
                function,
                span,
            )?,
            receiver: crate::ssa::refine::receiver_receipt::Emitted::new(
                &body.receiver_plans,
                function,
                span,
            )?,
            named: crate::ssa::refine::named_receipt::Emitted::new(
                &body.named_plans,
                function,
                span,
            )?,
            binding: crate::ssa::refine::binding_receipt::Emitted::new(
                &body.binding_plans,
                function,
                span,
            )?,
            temporary: crate::ssa::refine::temporary_receipt::Emitted::new(
                &body.temporary_plans,
                function,
                span,
            )?,
        })
    }
    pub(super) fn finish(
        self,
        body: &RefinedBody,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.consumer.finish(&body.loan_plans, function, span)?;
        self.receiver.finish(&body.receiver_plans, function, span)?;
        self.named.finish(&body.named_plans, function, span)?;
        self.temporary
            .finish(&body.temporary_plans, function, span)?;
        self.binding.finish(&body.binding_plans, function, span)?;
        self.child.finish(&body.child_plans, function, span)
    }
}

impl Emitter<'_> {
    pub(super) fn statements(
        &mut self,
        body: &RefinedBody,
        block: BlockRef,
        target: BlockId,
        receipts: &mut LoanReceipts,
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
                receipts
                    .binding
                    .alias(&body.binding_plans, &self.current, statement.span)?;
                receipts
                    .child
                    .alias(&body.child_plans, &self.current, statement.span)?;
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
                Some(crate::ssa::refine::body::LoanTag::Consumer(tag)) => {
                    receipts.consumer.statement(
                        &body.loan_plans,
                        tag,
                        self.function,
                        instruction,
                        &self.current,
                        statement.span,
                    )?
                }
                Some(crate::ssa::refine::body::LoanTag::Receiver(tag)) => {
                    receipts.receiver.statement(
                        &body.receiver_plans,
                        tag,
                        self.function,
                        instruction,
                        &self.current,
                        statement.span,
                    )?
                }
                Some(crate::ssa::refine::body::LoanTag::Named(tag)) => receipts.named.statement(
                    &body.named_plans,
                    tag,
                    self.function,
                    instruction,
                    &self.current,
                    statement.span,
                )?,
                Some(crate::ssa::refine::body::LoanTag::Temporary(tag)) => {
                    receipts.temporary.statement(
                        &body.temporary_plans,
                        tag,
                        self.function,
                        instruction,
                        &self.current,
                        statement.span,
                    )?
                }
                Some(crate::ssa::refine::body::LoanTag::Binding(tag)) => {
                    receipts.binding.statement(
                        &body.binding_plans,
                        tag,
                        self.function,
                        instruction,
                        &self.current,
                        statement.span,
                    )?
                }
                Some(crate::ssa::refine::body::LoanTag::Child(tag)) => {
                    receipts.child.statement(
                        &body.child_plans,
                        tag,
                        self.function,
                        instruction,
                        &self.current,
                        statement.span,
                    )?;
                }
                None => {}
            }
        }
        Ok(())
    }
}
