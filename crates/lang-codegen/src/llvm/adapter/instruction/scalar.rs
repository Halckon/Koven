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
            Operation::FloatArithmetic {
                operator,
                left,
                right,
            } => {
                self.lower_float_arithmetic(*operator, *left, *right, &results)?;
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
    fn lower_float_arithmetic(
        &mut self,
        operator: crate::ssa::model::FloatOperator,
        left: ValueId,
        right: Option<ValueId>,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        use crate::ssa::model::FloatOperator;
        let [result] = results else {
            return Err(invalid_result_count(
                "floating arithmetic",
                1,
                results.len(),
            ));
        };
        let left = self.value(left)?.into_float_value();
        let name = value_name(*result);
        let value = if let Some(right) = right {
            let right = self.value(right)?.into_float_value();
            match operator {
                FloatOperator::Add => self.builder.build_float_add(left, right, &name)?,
                FloatOperator::Subtract => self.builder.build_float_sub(left, right, &name)?,
                FloatOperator::Multiply => self.builder.build_float_mul(left, right, &name)?,
                FloatOperator::Divide => self.builder.build_float_div(left, right, &name)?,
                FloatOperator::Remainder => self.builder.build_float_rem(left, right, &name)?,
                FloatOperator::Negate => {
                    return Err(unsupported("floating negation has extra operand"));
                }
            }
        } else {
            self.builder.build_float_neg(left, &name)?
        };
        self.values.insert(*result, value.into());
        Ok(())
    }
}
