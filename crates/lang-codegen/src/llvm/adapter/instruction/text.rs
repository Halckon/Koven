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
