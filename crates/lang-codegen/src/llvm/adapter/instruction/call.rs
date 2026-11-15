//! Call, closure, range and callable instruction lowering.
use super::super::*;

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(super) fn lower_instruction_call(
        &mut self,
        instruction: &Instruction,
        results: Vec<ValueId>,
    ) -> Result<(), LlvmAdapterError> {
        match &instruction.operation {
            Operation::DirectCall {
                callee,
                receiver,
                arguments,
            } => {
                let operands = receiver
                    .iter()
                    .copied()
                    .chain(arguments.iter().copied())
                    .collect::<Vec<_>>();
                self.lower_call(*callee, &operands, &results)?;
            }
            Operation::FunctionAddress { target } => {
                self.lower_function_address(*target, &results)?;
            }
            Operation::BorrowCall {
                callee, arguments, ..
            } => {
                self.lower_borrow_call(instruction, *callee, arguments)?;
            }
            Operation::RangeConstruct { .. }
            | Operation::RangeCall { .. }
            | Operation::RangeLength { .. }
            | Operation::RangeElementPlace { .. }
            | Operation::RangeEnd { .. } => self.lower_range(instruction)?,
            Operation::ClosureConstruct {
                closure: closure_type,
                thunk,
                captures,
            } => {
                self.lower_closure_construct(*closure_type, *thunk, captures, &results)?;
            }
            Operation::CallableInvoke { .. }
            | Operation::CallableViewOf { .. }
            | Operation::CallableInvokeView { .. } => self.lower_callable(instruction, &results)?,
            _ => {
                return Err(unsupported(
                    "operation must be dispatched to its lowering family",
                ));
            }
        }
        Ok(())
    }
}
