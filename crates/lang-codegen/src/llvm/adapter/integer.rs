//! Exact-width integer bit operations, with defined LLVM shifts for every source count.
use super::{FunctionLowerer, LlvmAdapterError, invalid_result_count, scalar, value_name};
use crate::ssa::model::{IntegerBitwiseOperator, ValueId};

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(super) fn lower_integer_bitwise(
        &mut self,
        operator: IntegerBitwiseOperator,
        left: ValueId,
        right: ValueId,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [result] = results else {
            return Err(invalid_result_count("integer bitwise", 1, results.len()));
        };
        let lhs = self.int_value(left)?;
        let mut rhs = self.int_value(right)?;
        let name = value_name(*result);
        if matches!(
            operator,
            IntegerBitwiseOperator::Shl
                | IntegerBitwiseOperator::Shr
                | IntegerBitwiseOperator::Ushr
        ) {
            let mask = lhs
                .get_type()
                .const_int(u64::from(lhs.get_type().get_bit_width() - 1), false);
            rhs = self
                .builder
                .build_and(rhs, mask, &format!("{name}.count"))?;
        }
        let value = match operator {
            IntegerBitwiseOperator::And => self.builder.build_and(lhs, rhs, &name)?,
            IntegerBitwiseOperator::Or => self.builder.build_or(lhs, rhs, &name)?,
            IntegerBitwiseOperator::Xor => self.builder.build_xor(lhs, rhs, &name)?,
            // No nsw/nuw/exact flags: high bits are discarded, never checked arithmetic.
            IntegerBitwiseOperator::Shl => self.builder.build_left_shift(lhs, rhs, &name)?,
            IntegerBitwiseOperator::Shr => self.builder.build_right_shift(
                lhs,
                rhs,
                scalar::integer_signed(self.module, self.function, left)?,
                &name,
            )?,
            IntegerBitwiseOperator::Ushr => {
                self.builder.build_right_shift(lhs, rhs, false, &name)?
            }
        };
        self.values.insert(*result, value.into());
        Ok(())
    }
}
