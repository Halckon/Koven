//! Byte reads preserve the String owner and check indices before constructing an address.
use super::*;

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(super) fn lower_text_read(
        &mut self,
        instruction: &Instruction,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [result] = results else {
            return Err(invalid_result_count("text read", 1, results.len()));
        };
        let name = value_name(*result);
        let value = match instruction.operation {
            Operation::StringByteLength { source } => {
                let view = self.string_view(EntityId::Loan(source))?;
                // Every live String was checked against Int.MAX_VALUE at creation.
                self.builder
                    .build_int_cast(view.length(), self.context.i32_type(), &name)?
            }
            Operation::StringByteAt { source, index } => {
                let view = self.string_view(EntityId::Loan(source))?;
                let index = self.int_value(index)?;
                let length = self.builder.build_int_cast(
                    view.length(),
                    self.context.i32_type(),
                    &format!("{name}.length"),
                )?;
                // Unsigned comparison also rejects negative signed Int indices.
                let invalid = self.builder.build_int_compare(
                    IntPredicate::UGE,
                    index,
                    length,
                    &format!("{name}.invalid"),
                )?;
                self.dependencies.runtime.abort_if(
                    &self.builder,
                    self.llvm_function,
                    invalid,
                    &name,
                    crate::llvm::runtime::AbortMessage::Text("index out of bounds"),
                )?;
                let offset = self.builder.build_int_z_extend_or_bit_cast(
                    index,
                    self.dependencies.runtime.size_type(),
                    &format!("{name}.offset"),
                )?;
                // SAFETY: the checked success edge proves 0 <= index < byte length.
                let pointer = unsafe {
                    self.builder.build_in_bounds_gep(
                        self.context.i8_type(),
                        view.bytes(),
                        &[offset],
                        &format!("{name}.address"),
                    )?
                };
                self.builder
                    .build_load(self.context.i8_type(), pointer, &name)?
                    .into_int_value()
            }
            _ => {
                return Err(LlvmAdapterError::InvalidSsa(
                    "expected text read".to_owned(),
                ));
            }
        };
        self.values.insert(*result, value.into());
        Ok(())
    }
}
