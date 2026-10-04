//! Scalar values and test-only synthetic ZST materialization.
use super::*;
impl FunctionLowerer<'_, '_, '_, '_, '_> {
    pub(super) fn lower_constant(
        &mut self,
        constant: &ScalarConstant,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [result] = results else {
            return Err(invalid_result_count("constant", 1, results.len()));
        };
        let integer = match constant {
            ScalarConstant::Boolean(value) => Some((u64::from(*value), false)),
            ScalarConstant::Char(value) => Some((u64::from(*value), false)),
            ScalarConstant::Integer(value) => Some((*value as u64, *value < 0)),
            #[cfg(test)]
            ScalarConstant::SyntheticZero => None,
            ScalarConstant::Unit => None,
        };
        let value = match integer {
            Some((value, signed)) => self
                .dependencies
                .type_map
                .int_type(value_type(self.function, *result)?)?
                .const_int(value, signed)
                .into(),
            None => {
                #[cfg(test)]
                if matches!(constant, ScalarConstant::SyntheticZero) {
                    let zero = self
                        .dependencies
                        .type_map
                        .basic_type(value_type(self.function, *result)?)?
                        .into_struct_type()
                        .const_zero();
                    self.values.insert(*result, zero.into());
                    return Ok(());
                }
                self.context.struct_type(&[], false).const_zero().into()
            }
        };
        self.values.insert(*result, value);
        Ok(())
    }
}
