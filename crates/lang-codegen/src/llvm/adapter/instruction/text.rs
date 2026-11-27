//! Text, String and print instruction lowering.
use super::super::*;

impl<'ctx, 'llvm, 'ssa, 'functions, 'sources>
    FunctionLowerer<'ctx, 'llvm, 'ssa, 'functions, 'sources>
{
    fn string_equality_view(
        &self,
        operand: EntityId,
    ) -> Result<(string::StringView<'ctx>, bool), LlvmAdapterError> {
        let ty = self
            .function
            .entity(operand)
            .ok_or_else(|| LlvmAdapterError::InvalidSsa("missing String equality operand".into()))?
            .ty
            .semantic_type();
        if self.module.map_result_value(ty).is_some() {
            return Ok((
                string::map_result_view(&self.builder, self.string_storage(operand)?)?,
                true,
            ));
        }
        Ok((
            self.string_view(operand)?,
            self.module.inline_nullable(ty).is_some(),
        ))
    }

    fn lower_scalar_string(
        &mut self,
        scalar: ValueId,
        result: ValueId,
    ) -> Result<(), LlvmAdapterError> {
        use crate::{llvm::decimal, ssa::model::SsaTypeKind};

        let name = value_name(result);
        let string_type = self
            .dependencies
            .type_map
            .basic_type(value_type(self.function, result)?)?
            .into_struct_type();
        let runtime = self.dependencies.runtime;
        let value = self.int_value(scalar)?;
        let string = match self.module.type_kind(value_type(self.function, scalar)?) {
            Some(SsaTypeKind::Boolean) => {
                let ty = value_type(self.function, result)?;
                let literal = |bytes: &[u8], name: &str| {
                    string::literal(
                        self.llvm,
                        &self.builder,
                        self.dependencies.type_map,
                        runtime,
                        ty,
                        bytes,
                        name,
                    )
                };
                let yes = literal(b"true".as_slice(), &format!("{name}.true"))?;
                let no = literal(b"false".as_slice(), &format!("{name}.false"))?;
                self.builder
                    .build_select(value, yes, no, &name)?
                    .into_struct_value()
            }
            Some(SsaTypeKind::Char) => string::from_char(
                &self.builder,
                self.llvm_function,
                runtime,
                string_type,
                value,
                &name,
            )?,
            Some(SsaTypeKind::Integer { signed, .. }) => {
                let scratch = self.entry_alloca(
                    self.context
                        .i8_type()
                        .array_type(decimal::BUFFER_BYTES)
                        .into(),
                    &format!("{name}.decimal"),
                )?;
                let (text, length) = decimal::format_into(
                    &self.builder,
                    self.llvm_function,
                    scratch,
                    value,
                    *signed,
                )?;
                let length = self.builder.build_int_cast(
                    length,
                    runtime.size_type(),
                    &format!("{name}.length"),
                )?;
                let owned = runtime.allocate_string_bytes(
                    &self.builder,
                    self.llvm_function,
                    length,
                    &name,
                )?;
                self.builder.build_memcpy(owned, 1, text, 1, length)?;
                string::build_owner(&self.builder, string_type, owned, length, length, &name)?
            }
            _ => {
                return Err(LlvmAdapterError::InvalidSsa(
                    "invalid scalar for String conversion".to_owned(),
                ));
            }
        };
        self.values.insert(result, string.into());
        Ok(())
    }

    pub(super) fn lower_instruction_text(
        &mut self,
        instruction: &Instruction,
        results: Vec<ValueId>,
    ) -> Result<(), LlvmAdapterError> {
        match &instruction.operation {
            Operation::PrintLiteral { bytes } => {
                if !results.is_empty() {
                    return Err(invalid_result_count("print literal", 0, results.len()));
                }
                self.dependencies.runtime.emit_print_literal(
                    self.llvm,
                    &self.builder,
                    self.llvm_function,
                    bytes,
                    &format!(
                        "print.f{}.i{}",
                        self.function.id.index(),
                        instruction.id.index()
                    ),
                )?;
            }
            Operation::StringLiteral { string, bytes } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("string literal", 1, results.len()));
                };
                let value = string::literal(
                    self.llvm,
                    &self.builder,
                    self.dependencies.type_map,
                    self.dependencies.runtime,
                    *string,
                    bytes,
                    &value_name(*result),
                )?;
                self.values.insert(*result, value.into());
            }
            Operation::StringAppend { owner, fragment } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("string append", 1, results.len()));
                };
                let value = string::append(
                    self.context,
                    &self.builder,
                    self.llvm_function,
                    self.dependencies.runtime,
                    value_type(self.function, *owner)?,
                    self.value(*owner)?.into_struct_value(),
                    self.string_view(*fragment)?,
                    &value_name(*result),
                )?;
                self.values.insert(*result, value.into());
            }
            Operation::StringConcat { left, right } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("string concat", 1, results.len()));
                };
                let value = string::concat(
                    self.context,
                    &self.builder,
                    self.llvm_function,
                    self.dependencies.runtime,
                    self.dependencies
                        .type_map
                        .basic_type(value_type(self.function, *result)?)?
                        .into_struct_type(),
                    self.string_view(*left)?,
                    self.string_view(*right)?,
                    &value_name(*result),
                )?;
                self.values.insert(*result, value.into());
            }
            Operation::StringFromScalar { scalar } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("string from scalar", 1, results.len()));
                };
                self.lower_scalar_string(*scalar, *result)?;
            }
            Operation::StringClone { .. }
            | Operation::StringCopyBytes { .. }
            | Operation::StringCopySlice { .. } => self.lower_text_copy(instruction, &results)?,
            Operation::StringByteLength { .. }
            | Operation::StringByteAt { .. }
            | Operation::Utf8ErrorOffset { .. } => self.lower_text_read(instruction, &results)?,
            Operation::StringEqual { left, right } => {
                let [result] = results.as_slice() else {
                    return Err(invalid_result_count("string equal", 1, results.len()));
                };
                let (left, left_nullable) = self.string_equality_view(*left)?;
                let (right, right_nullable) = self.string_equality_view(*right)?;
                let value = string::equal(
                    self.context,
                    &self.builder,
                    self.llvm_function,
                    self.dependencies.runtime,
                    [left, right],
                    left_nullable || right_nullable,
                    &value_name(*result),
                )?;
                self.values.insert(*result, value.into());
            }
            Operation::PrintString { value } => {
                if !results.is_empty() {
                    return Err(invalid_result_count("print string", 0, results.len()));
                }
                string::print(
                    self.llvm,
                    &self.builder,
                    self.llvm_function,
                    self.dependencies.runtime,
                    self.string_view(EntityId::Loan(*value))?,
                    &format!(
                        "print.f{}.i{}",
                        self.function.id.index(),
                        instruction.id.index()
                    ),
                )?;
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
