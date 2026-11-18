//! Instruction lowering dispatch by operation family.
//!
//! Invariant: the groups below partition every `Operation` variant, and each family module matches
//! exactly its own group. The compiler enforces the partition; the family wildcard exists only to
//! report an internal dispatch violation, never a user-source path.
mod aggregate;
mod call;
mod container;
mod heap;
mod place;
mod scalar;
mod text;

use super::*;

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(super) fn lower_instruction(
        &mut self,
        instruction: &Instruction,
    ) -> Result<(), LlvmAdapterError> {
        let results = if matches!(
            instruction.operation,
            Operation::TaggedPayloadPlace { .. }
                | Operation::HeapPayloadPlace { .. }
                | Operation::SharedPayloadPlace { .. }
                | Operation::SharedPayloadLoan { .. }
                | Operation::FieldPlace { .. }
                | Operation::SharedFieldLoan { .. }
                | Operation::SharedHeapFieldLoan { .. }
                | Operation::SharedReborrow { .. }
                | Operation::CallableViewOf { .. }
                | Operation::SharedReferenceFollow { .. }
                | Operation::ContainerElementPlace { .. }
                | Operation::RangeElementPlace { .. }
                | Operation::RootPlace { .. }
                | Operation::BorrowBegin { .. }
                | Operation::RangeConstruct { .. }
                | Operation::RangeCall { .. }
                | Operation::BorrowCall { .. }
                | Operation::MapRequireValue { .. }
                | Operation::MapIterationKeyLoan { .. }
                | Operation::MapIterationValueLoan { .. }
        ) {
            Vec::new()
        } else {
            value_results(instruction)?
        };
        match &instruction.operation {
            Operation::PrintLiteral { .. }
            | Operation::PrintString { .. }
            | Operation::StringLiteral { .. }
            | Operation::StringConcat { .. }
            | Operation::StringClone { .. }
            | Operation::StringCopyBytes { .. }
            | Operation::StringCopySlice { .. }
            | Operation::StringByteLength { .. }
            | Operation::StringByteAt { .. }
            | Operation::Utf8ErrorOffset { .. }
            | Operation::StringEqual { .. } => self.lower_instruction_text(instruction, results)?,
            Operation::Constant(..)
            | Operation::Binary { .. }
            | Operation::CheckedArithmetic { .. }
            | Operation::IntegerRangeConstruct { .. }
            | Operation::IntegerRangeContains { .. }
            | Operation::IntegerRangeEqual { .. }
            | Operation::CheckedIntegerConvert { .. }
            | Operation::IntegerBitwise { .. }
            | Operation::Compare { .. }
            | Operation::BooleanNot { .. }
            | Operation::IntegerNot { .. } => {
                self.lower_instruction_scalar(instruction, results)?
            }
            Operation::DirectCall { .. }
            | Operation::FunctionAddress { .. }
            | Operation::BorrowCall { .. }
            | Operation::RangeConstruct { .. }
            | Operation::RangeCall { .. }
            | Operation::RangeLength { .. }
            | Operation::RangeElementPlace { .. }
            | Operation::RangeEnd { .. }
            | Operation::ClosureConstruct { .. }
            | Operation::CallableInvoke { .. }
            | Operation::CallableViewOf { .. }
            | Operation::CallableInvokeView { .. } => {
                self.lower_instruction_call(instruction, results)?
            }
            Operation::AggregateConstruct { .. }
            | Operation::AggregateProject { .. }
            | Operation::AggregateExplode { .. }
            | Operation::AggregateCopyExplode { .. }
            | Operation::TaggedConstruct { .. }
            | Operation::TaggedPayloadPlace { .. }
            | Operation::TaggedDiscriminant { .. } => {
                self.lower_instruction_aggregate(instruction, results)?
            }
            Operation::HeapAllocate { .. }
            | Operation::HeapPayloadPlace { .. }
            | Operation::HeapFieldRead { .. }
            | Operation::HeapFieldReplace { .. }
            | Operation::HeapFieldExchange { .. }
            | Operation::InlineFieldReplace { .. }
            | Operation::SharedAllocate { .. }
            | Operation::SharedRetain { .. }
            | Operation::SharedPayloadPlace { .. }
            | Operation::SharedReferenceFollow { .. }
            | Operation::SharedPayloadLoan { .. } => {
                self.lower_instruction_heap(instruction, results)?
            }
            Operation::ContainerConstruct { .. }
            | Operation::ContainerGenerate { .. }
            | Operation::ContainerGenerateBorrowed { .. }
            | Operation::ContainerLength { .. }
            | Operation::ContainerElementPlace { .. }
            | Operation::ContainerAppend { .. }
            | Operation::ContainerClear { .. }
            | Operation::ContainerRemoveAt { .. }
            | Operation::ContainerRemoveFirst { .. }
            | Operation::ContainerRemoveLast { .. }
            | Operation::ContainerInsertAt { .. }
            | Operation::ContainerReplace { .. }
            | Operation::MapIterationFirst { .. }
            | Operation::MapIterationHasNext { .. }
            | Operation::MapIterationNext { .. }
            | Operation::MapIterationKeyLoan { .. }
            | Operation::MapIterationValueLoan { .. }
            | Operation::MapRequireValue { .. }
            | Operation::MapWithValue { .. }
            | Operation::MapConstruct { .. }
            | Operation::MapConsume { .. }
            | Operation::MapSize { .. }
            | Operation::MapContains { .. }
            | Operation::MapGet { .. }
            | Operation::MapResultUnwrap { .. }
            | Operation::MapPut { .. }
            | Operation::MapRemove { .. } => {
                self.lower_instruction_container(instruction, results)?
            }
            Operation::FieldPlace { .. }
            | Operation::SharedFieldLoan { .. }
            | Operation::SharedHeapFieldLoan { .. }
            | Operation::SharedReborrow { .. }
            | Operation::NullableWrap { .. }
            | Operation::NullableNull { .. }
            | Operation::NullableIsNull { .. }
            | Operation::NullableLoanIsNull { .. }
            | Operation::NullableTake { .. }
            | Operation::Copy { .. }
            | Operation::Consume { .. }
            | Operation::Drop { .. }
            | Operation::RootPlace { .. }
            | Operation::RootPlaceTake { .. }
            | Operation::RootReplace { .. }
            | Operation::RootSwap { .. }
            | Operation::BorrowBegin { .. }
            | Operation::BorrowEnd { .. }
            | Operation::Read { .. }
            | Operation::Mutate { .. }
            | Operation::LoanMutate { .. }
            | Operation::LoanReplace { .. } => {
                self.lower_instruction_place(instruction, results)?
            }
        }
        Ok(())
    }
}
