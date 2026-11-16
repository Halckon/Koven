//! Scalar, integer and comparison instruction lowering.
use super::super::*;

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(super) fn lower_instruction_scalar(
        &mut self,
        instruction: &Instruction,
        results: Vec<ValueId>,
    ) -> Result<(), LlvmAdapterError> {
        match &instruction.operation {
            Operation::Constant(constant) => {
                self.lower_constant(constant, &results)?;
            }
            Operation::Binary {
                operator,
                left,
                right,
            } => self.lower_binary(*operator, *left, *right, &results)?,
            Operation::CheckedArithmetic {
                operator,
                left,
                right,
            } => self.lower_checked(*operator, *left, *right, &results)?,
            Operation::IntegerRangeConstruct {
                first,
                bound,
                inclusive,
            } => self.lower_integer_range_construct(*first, *bound, *inclusive, &results)?,
            Operation::IntegerRangeContains { value, range } => {
                self.lower_integer_range_contains(*value, *range, &results)?
            }
            Operation::IntegerRangeEqual { left, right } => {
                self.lower_integer_range_equal(*left, *right, &results)?
            }
            Operation::CheckedIntegerConvert { operand } => {
                self.lower_checked_integer_conversion(*operand, &results)?;
            }
            Operation::IntegerBitwise {
                operator,
                left,
                right,
            } => {
                self.lower_integer_bitwise(*operator, *left, *right, &results)?;
            }
            Operation::Compare {
                operator,
                left,
                right,
            } => self.lower_comparison(*operator, *left, *right, &results)?,
            Operation::BooleanNot { operand } | Operation::IntegerNot { operand } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("boolean not", 1, results.len()));
                };
                let value = self
                    .builder
                    .build_not(self.int_value(*operand)?, &value_name(*result))?;
                self.values.insert(*result, value.into());
            }
            _ => {
                return Err(unsupported(
                    "internal dispatch invariant violated: operation belongs to another lowering family",
                ));
            }
        }
        Ok(())
    }
}
