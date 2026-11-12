//! Mechanical statement emission, including aliases and optional production receipts.
use super::*;
/// Each protocol retains its own plans, role identities and final receipt.
pub(super) struct LoanReceipts {
    loop_receipt: crate::ssa::refine::loop_receipt::Emitted,
    pub(super) nested: crate::ssa::refine::nested_receipt::Emitted,
    pub(super) bounded: crate::ssa::refine::bounded_receipt::Emitted,
    pub(super) single: crate::ssa::refine::single_receipt::Emitted,
    pub(super) multi: crate::ssa::refine::multi_receipt::Emitted,
    pub(super) branch: crate::ssa::refine::branch_receipt::Emitted,
    pub(super) forward: crate::ssa::refine::forward_receipt::Emitted,
    consumer: crate::ssa::refine::loan_receipt::Emitted,
    receiver: crate::ssa::refine::receiver_receipt::Emitted,
    named: crate::ssa::refine::named_receipt::Emitted,
    temporary: crate::ssa::refine::temporary_receipt::Emitted,
    binding: crate::ssa::refine::binding_receipt::Emitted,
    aux_binding: crate::ssa::refine::aux_binding_receipt::Emitted,
    distinct_aux: crate::ssa::refine::distinct_aux_receipt::Emitted,
    later_aux: crate::ssa::refine::later_aux_receipt::Emitted,
    auxiliary_sequence: crate::ssa::refine::auxiliary_sequence_receipt::Emitted,
    interleaved_aux: crate::ssa::refine::interleaved_aux_receipt::Emitted,
    multi_binding: crate::ssa::refine::multi_binding_receipt::Emitted,
    child: crate::ssa::refine::child_receipt::Emitted,
}
impl LoanReceipts {
    pub(super) fn new(
        body: &RefinedBody,
        function: &Function,
        span: Span,
    ) -> Result<Self, LoweringError> {
        Ok(Self {
            loop_receipt: crate::ssa::refine::loop_receipt::Emitted::new(
                &body.loop_plans,
                function,
                span,
            )?,
            nested: crate::ssa::refine::nested_receipt::Emitted::new(
                &body.nested_plans,
                function,
                span,
            )?,
            bounded: crate::ssa::refine::bounded_receipt::Emitted::new(
                &body.bounded_plans,
                function,
                span,
            )?,
            single: crate::ssa::refine::single_receipt::Emitted::new(
                &body.single_plans,
                function,
                span,
            )?,
            multi: crate::ssa::refine::multi_receipt::Emitted::new(
                &body.multi_plans,
                function,
                span,
            )?,
            branch: crate::ssa::refine::branch_receipt::Emitted::new(
                &body.branch_plans,
                function,
                span,
            )?,
            forward: crate::ssa::refine::forward_receipt::Emitted::new(
                &body.forward_plans,
                function,
                span,
            )?,
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
            aux_binding: crate::ssa::refine::aux_binding_receipt::Emitted::new(
                &body.aux_binding_plans,
                function,
                span,
            )?,
            distinct_aux: crate::ssa::refine::distinct_aux_receipt::Emitted::new(
                &body.distinct_aux_plans,
                function,
                span,
            )?,
            later_aux: crate::ssa::refine::later_aux_receipt::Emitted::new(
                &body.later_aux_plans,
                function,
                span,
            )?,
            auxiliary_sequence: crate::ssa::refine::auxiliary_sequence_receipt::Emitted::new(
                &body.auxiliary_sequence_plans,
                function,
                span,
            )?,
            interleaved_aux: crate::ssa::refine::interleaved_aux_receipt::Emitted::new(
                &body.interleaved_aux_plans,
                function,
                span,
            )?,
            multi_binding: crate::ssa::refine::multi_binding_receipt::Emitted::new(
                &body.multi_binding_plans,
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
    /// Forward the shared SSA layout through each independent protocol, fail-fast in order.
    pub(super) fn layout(
        &mut self,
        body: &RefinedBody,
        function: &Function,
        blocks: &BTreeMap<BlockRef, BlockId>,
        parameters: &BTreeMap<BlockRef, Vec<Slot>>,
        entities: &BTreeMap<BlockRef, Vec<EntityId>>,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.loop_receipt.layout(
            &body.loop_plans,
            function,
            blocks,
            parameters,
            entities,
            span,
        )?;
        self.branch.layout(
            &body.branch_plans,
            function,
            blocks,
            parameters,
            entities,
            span,
        )?;
        self.nested.layout(
            &body.nested_plans,
            function,
            blocks,
            parameters,
            entities,
            span,
        )?;
        self.bounded.layout(
            &body.bounded_plans,
            function,
            blocks,
            parameters,
            entities,
            span,
        )?;
        self.single.layout(
            &body.single_plans,
            function,
            blocks,
            parameters,
            entities,
            span,
        )?;
        self.multi.layout(
            &body.multi_plans,
            function,
            blocks,
            parameters,
            entities,
            span,
        )?;
        Ok(())
    }
    pub(super) fn terminator(
        &mut self,
        body: &RefinedBody,
        function: &Function,
        target: BlockId,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.loop_receipt
            .terminator(&body.loop_plans, function, target, current, span)?;
        self.nested
            .terminator(&body.nested_plans, function, target, current, span)?;
        self.bounded
            .terminator(&body.bounded_plans, function, target, current, span)?;
        self.single
            .terminator(&body.single_plans, function, target, current, span)?;
        self.multi
            .terminator(&body.multi_plans, function, target, current, span)?;
        self.branch
            .terminator(&body.branch_plans, function, target, current, span)?;
        self.forward
            .terminator(&body.forward_plans, function, target, current, span)
    }
    pub(super) fn finish(
        self,
        body: &RefinedBody,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.loop_receipt.finish(&body.loop_plans, function, span)?;
        self.forward.finish(&body.forward_plans, function, span)?;
        self.consumer.finish(&body.loan_plans, function, span)?;
        self.receiver.finish(&body.receiver_plans, function, span)?;
        self.named.finish(&body.named_plans, function, span)?;
        self.temporary
            .finish(&body.temporary_plans, function, span)?;
        self.binding.finish(&body.binding_plans, function, span)?;
        self.aux_binding
            .finish(&body.aux_binding_plans, function, span)?;
        self.distinct_aux
            .finish(&body.distinct_aux_plans, function, span)?;
        self.later_aux
            .finish(&body.later_aux_plans, function, span)?;
        self.auxiliary_sequence
            .finish(&body.auxiliary_sequence_plans, function, span)?;
        self.interleaved_aux
            .finish(&body.interleaved_aux_plans, function, span)?;
        self.multi_binding
            .finish(&body.multi_binding_plans, function, span)?;
        self.child.finish(&body.child_plans, function, span)?;
        self.nested.finish(&body.nested_plans, function, span)?;
        self.bounded.finish(&body.bounded_plans, function, span)?;
        self.single.finish(&body.single_plans, function, span)?;
        self.multi.finish(&body.multi_plans, function, span)?;
        self.branch.finish(&body.branch_plans, function, span)
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
                if statement.loan_tag.is_some() || statement.loop_tag.is_some() {
                    return Err(error(LoweringErrorKind::InvalidModel, statement.span));
                }
                let entity = self.get(*source, statement.span)?;
                self.current.insert(*result, entity);
                self.defined.insert(*result, entity);
                receipts.loop_receipt.alias(
                    &body.loop_plans,
                    *source,
                    *result,
                    target,
                    &self.current,
                    statement.span,
                )?;
                receipts
                    .binding
                    .alias(&body.binding_plans, &self.current, statement.span)?;
                receipts.aux_binding.alias(
                    &body.aux_binding_plans,
                    &self.current,
                    statement.span,
                )?;
                receipts.distinct_aux.alias(
                    &body.distinct_aux_plans,
                    &self.current,
                    statement.span,
                )?;
                receipts
                    .later_aux
                    .alias(&body.later_aux_plans, &self.current, statement.span)?;
                receipts.auxiliary_sequence.alias(
                    &body.auxiliary_sequence_plans,
                    &self.current,
                    statement.span,
                )?;
                receipts.interleaved_aux.alias(
                    &body.interleaved_aux_plans,
                    &self.current,
                    statement.span,
                )?;
                receipts.multi_binding.alias(
                    &body.multi_binding_plans,
                    &self.current,
                    statement.span,
                )?;
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
            if let Some(tag) = statement.loop_tag {
                receipts.loop_receipt.statement(
                    &body.loop_plans,
                    tag,
                    self.function,
                    instruction,
                    &self.current,
                    statement.span,
                )?;
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
                Some(crate::ssa::refine::body::LoanTag::AuxBinding(tag)) => {
                    receipts.aux_binding.statement(
                        &body.aux_binding_plans,
                        tag,
                        self.function,
                        instruction,
                        &self.current,
                        statement.span,
                    )?
                }
                Some(crate::ssa::refine::body::LoanTag::DistinctAux(tag)) => {
                    receipts.distinct_aux.statement(
                        &body.distinct_aux_plans,
                        tag,
                        self.function,
                        instruction,
                        &self.current,
                        statement.span,
                    )?
                }
                Some(crate::ssa::refine::body::LoanTag::LaterAux(tag)) => {
                    receipts.later_aux.statement(
                        &body.later_aux_plans,
                        tag,
                        self.function,
                        instruction,
                        &self.current,
                        statement.span,
                    )?
                }
                Some(crate::ssa::refine::body::LoanTag::AuxiliarySequence(tag)) => {
                    receipts.auxiliary_sequence.statement(
                        &body.auxiliary_sequence_plans,
                        tag,
                        self.function,
                        instruction,
                        &self.current,
                        statement.span,
                    )?
                }
                Some(crate::ssa::refine::body::LoanTag::InterleavedAux(tag)) => {
                    receipts.interleaved_aux.statement(
                        &body.interleaved_aux_plans,
                        tag,
                        self.function,
                        instruction,
                        &self.current,
                        statement.span,
                    )?
                }
                Some(crate::ssa::refine::body::LoanTag::MultiBinding(tag)) => {
                    receipts.multi_binding.statement(
                        &body.multi_binding_plans,
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
                Some(crate::ssa::refine::body::LoanTag::Forward(tag)) => {
                    receipts.forward.statement(
                        &body.forward_plans,
                        tag,
                        self.function,
                        instruction,
                        &self.current,
                        statement.span,
                    )?
                }
                Some(crate::ssa::refine::body::LoanTag::Branch(tag)) => receipts.branch.statement(
                    &body.branch_plans,
                    tag,
                    self.function,
                    instruction,
                    &self.current,
                    statement.span,
                )?,
                Some(crate::ssa::refine::body::LoanTag::Nested(tag)) => receipts.nested.statement(
                    &body.nested_plans,
                    tag,
                    self.function,
                    instruction,
                    &self.current,
                    statement.span,
                )?,
                Some(crate::ssa::refine::body::LoanTag::Bounded(tag)) => {
                    receipts.bounded.statement(
                        &body.bounded_plans,
                        tag,
                        self.function,
                        instruction,
                        &self.current,
                        statement.span,
                    )?
                }
                Some(crate::ssa::refine::body::LoanTag::Single(tag)) => receipts.single.statement(
                    &body.single_plans,
                    tag,
                    self.function,
                    instruction,
                    &self.current,
                    statement.span,
                )?,
                Some(crate::ssa::refine::body::LoanTag::Multi(tag)) => receipts.multi.statement(
                    &body.multi_plans,
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
