//! Checked text copies share the established clone allocation and empty-owner paths.
use super::*;

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    pub(super) fn lower_text_copy(
        &mut self,
        instruction: &Instruction,
        results: &[ValueId],
    ) -> Result<(), LlvmAdapterError> {
        let [result] = results else {
            return Err(invalid_result_count("text copy", 1, results.len()));
        };
        let name = value_name(*result);
        let view = match instruction.operation {
            Operation::StringClone { source } => self.string_view(EntityId::Loan(source))?,
            Operation::StringCopyBytes { source } => {
                let (bytes, length) = self.byte_array_parts(source, &name)?;
                let offset = self.utf8_offset(bytes, length, &name)?;
                let invalid = self.builder.build_int_compare(
                    IntPredicate::NE,
                    offset,
                    self.dependencies.runtime.size_type().const_all_ones(),
                    &format!("{name}.invalid_utf8"),
                )?;
                self.dependencies.runtime.abort_if(
                    &self.builder,
                    self.llvm_function,
                    invalid,
                    &name,
                    crate::llvm::runtime::AbortMessage::Text("invalid UTF-8"),
                )?;
                string::StringView::from_valid_utf8(bytes, length)
            }
            Operation::StringCopySlice { source, start, end } => {
                let source = self.string_view(EntityId::Loan(source))?;
                self.checked_text_slice(
                    source,
                    self.int_value(start)?,
                    self.int_value(end)?,
                    &name,
                )?
            }
            _ => {
                return Err(LlvmAdapterError::InvalidSsa(
                    "expected text copy".to_owned(),
                ));
            }
        };
        let value = string::clone_owner(
            self.context,
            &self.builder,
            self.llvm_function,
            self.dependencies.runtime,
            self.dependencies
                .type_map
                .basic_type(value_type(self.function, *result)?)?
                .into_struct_type(),
            view,
            &name,
        )?;
        self.values.insert(*result, value.into());
        Ok(())
    }

    fn checked_text_slice(
        &self,
        source: string::StringView<'ctx>,
        start: IntValue<'ctx>,
        end: IntValue<'ctx>,
        name: &str,
    ) -> Result<string::StringView<'ctx>, LlvmAdapterError> {
        let length = self.builder.build_int_cast(
            source.length(),
            self.context.i32_type(),
            &format!("{name}.source_length"),
        )?;
        // UGT rejects negative endpoints, and reversed unsigned endpoints are rejected too.
        let start_out = self.builder.build_int_compare(
            IntPredicate::UGT,
            start,
            length,
            &format!("{name}.start_out"),
        )?;
        let end_out = self.builder.build_int_compare(
            IntPredicate::UGT,
            end,
            length,
            &format!("{name}.end_out"),
        )?;
        let reversed = self.builder.build_int_compare(
            IntPredicate::UGT,
            start,
            end,
            &format!("{name}.reversed"),
        )?;
        let outside = self
            .builder
            .build_or(start_out, end_out, &format!("{name}.outside"))?;
        let invalid = self
            .builder
            .build_or(outside, reversed, &format!("{name}.invalid_range"))?;
        self.dependencies.runtime.abort_if(
            &self.builder,
            self.llvm_function,
            invalid,
            name,
            crate::llvm::runtime::AbortMessage::Text("invalid byte range"),
        )?;
        let size = self.dependencies.runtime.size_type();
        let start =
            self.builder
                .build_int_z_extend_or_bit_cast(start, size, &format!("{name}.start"))?;
        let end = self
            .builder
            .build_int_z_extend_or_bit_cast(end, size, &format!("{name}.end"))?;
        self.require_scalar_boundary(source, start, &format!("{name}.start_boundary"))?;
        self.require_scalar_boundary(source, end, &format!("{name}.end_boundary"))?;
        let length = self
            .builder
            .build_int_sub(end, start, &format!("{name}.length"))?;
        // SAFETY: checked bounds prove start <= source length, including the one-past empty slice.
        let bytes = unsafe {
            self.builder.build_in_bounds_gep(
                self.context.i8_type(),
                source.bytes(),
                &[start],
                &format!("{name}.bytes"),
            )?
        };
        Ok(string::StringView::from_valid_utf8(bytes, length))
    }

    /// The caller has checked offset <= length; the end boundary must never load a byte.
    fn require_scalar_boundary(
        &self,
        source: string::StringView<'ctx>,
        offset: IntValue<'ctx>,
        name: &str,
    ) -> Result<(), LlvmAdapterError> {
        let inspect = self
            .context
            .append_basic_block(self.llvm_function, &format!("{name}.inspect"));
        let valid = self
            .context
            .append_basic_block(self.llvm_function, &format!("{name}.valid"));
        let at_end = self.builder.build_int_compare(
            IntPredicate::EQ,
            offset,
            source.length(),
            &format!("{name}.at_end"),
        )?;
        self.builder
            .build_conditional_branch(at_end, valid, inspect)?;
        self.builder.position_at_end(inspect);
        // SAFETY: offset <= length was checked, and this edge excludes offset == length.
        let pointer = unsafe {
            self.builder.build_in_bounds_gep(
                self.context.i8_type(),
                source.bytes(),
                &[offset],
                &format!("{name}.address"),
            )?
        };
        let byte = self
            .builder
            .build_load(self.context.i8_type(), pointer, &format!("{name}.byte"))?
            .into_int_value();
        let prefix = self.builder.build_and(
            byte,
            self.context.i8_type().const_int(0xc0, false),
            &format!("{name}.prefix"),
        )?;
        let continuation = self.builder.build_int_compare(
            IntPredicate::EQ,
            prefix,
            self.context.i8_type().const_int(0x80, false),
            &format!("{name}.continuation"),
        )?;
        self.dependencies.runtime.abort_if(
            &self.builder,
            self.llvm_function,
            continuation,
            name,
            crate::llvm::runtime::AbortMessage::Text("not a scalar boundary"),
        )?;
        self.builder.build_unconditional_branch(valid)?;
        self.builder.position_at_end(valid);
        Ok(())
    }
}
