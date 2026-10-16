//! Checked conversions compare the original mathematical integer before truncation.
use super::{
    FunctionLowerer, IntPredicate, LlvmAdapterError, invalid_result_count, scalar, value_name,
    value_type,
};
use crate::ssa::model::ValueId;

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(super) fn lower_checked_integer_conversion(
        &mut self,
        operand: ValueId,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [result, failed] = results else {
            return Err(invalid_result_count(
                "checked integer conversion",
                2,
                results.len(),
            ));
        };
        let source = self.int_value(operand)?;
        let source_signed = scalar::integer_signed(self.module, self.function, operand)?;
        let target_signed = scalar::integer_signed(self.module, self.function, *result)?;
        let target = self
            .dependencies
            .type_map
            .int_type(value_type(self.function, *result)?)?;
        let bits = target.get_bit_width();
        let wide = self.context.i128_type();
        let name = value_name(*result);
        let mathematical = if source_signed {
            self.builder
                .build_int_s_extend(source, wide, &format!("{name}.mathematical"))?
        } else {
            self.builder
                .build_int_z_extend(source, wide, &format!("{name}.mathematical"))?
        };
        // Every source integer fits i128. Sign-extend a signed i64 minimum, but never
        // sign-extend the unsigned maximum (notably ULong::MAX).
        let minimum = if target_signed {
            -(1_i128 << (bits - 1))
        } else {
            0
        };
        let maximum = (1_u128 << (bits - u32::from(target_signed))) - 1;
        let below = self.builder.build_int_compare(
            IntPredicate::SLT,
            mathematical,
            wide.const_int(minimum as u64, true),
            &format!("{name}.below"),
        )?;
        let above = self.builder.build_int_compare(
            IntPredicate::SGT,
            mathematical,
            wide.const_int(maximum as u64, false),
            &format!("{name}.above"),
        )?;
        let failure = self.builder.build_or(below, above, &value_name(*failed))?;
        // A plain trunc has no poison/side effect. The checked SSA CFG exposes this
        // temporary only on success; failure branches to Abort without delivery.
        let converted = self
            .builder
            .build_int_truncate(mathematical, target, &name)?;
        self.values.insert(*result, converted.into());
        self.values.insert(*failed, failure.into());
        Ok(())
    }
}
