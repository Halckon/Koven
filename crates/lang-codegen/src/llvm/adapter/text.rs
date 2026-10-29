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
            Operation::Utf8ErrorOffset { source } => {
                return self.lower_utf8_error_offset(source, results);
            }
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

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(super) fn lower_utf8_error_offset(
        &mut self,
        source: LoanId,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [result] = results else {
            return Err(invalid_result_count("utf8 error offset", 1, results.len()));
        };
        let name = value_name(*result);
        let ty = access_type(self.function, PlaceAccess::Loan(source))?;
        let header = self
            .builder
            .build_load(
                self.dependencies.type_map.basic_type(ty)?,
                self.access(PlaceAccess::Loan(source))?,
                &format!("{name}.array"),
            )?
            .into_struct_value();
        let bytes = self
            .builder
            .build_extract_value(header, 0, &format!("{name}.bytes"))?
            .into_pointer_value();
        let length = self
            .builder
            .build_extract_value(header, 1, &format!("{name}.length"))?
            .into_int_value();
        let scanner = crate::llvm::utf8::define_error_offset(
            self.context,
            self.llvm,
            self.dependencies.runtime.size_type(),
        )?;
        let offset = match self
            .builder
            .build_call(
                scanner,
                &[bytes.into(), length.into()],
                &format!("{name}.offset"),
            )?
            .try_as_basic_value()
        {
            ValueKind::Basic(value) => value.into_int_value(),
            ValueKind::Instruction(_) => {
                return Err(LlvmAdapterError::Build(
                    "UTF-8 scanner returned void".to_owned(),
                ));
            }
        };
        // Array length is bounded by Int.MAX_VALUE; all-ones success becomes Int -1.
        let value = self
            .builder
            .build_int_cast(offset, self.context.i32_type(), &name)?;
        self.values.insert(*result, value.into());
        Ok(())
    }
}
