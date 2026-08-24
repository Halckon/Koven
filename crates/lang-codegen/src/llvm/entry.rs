//! Explicit Koven entry validation and C ABI process wrapper.

use inkwell::{context::Context, module::Module as LlvmModule, values::FunctionValue};

use crate::ssa::model::{FunctionId, Module};

use super::LlvmAdapterError;

pub(super) fn validate(module: &Module, entry: FunctionId) -> Result<(), LlvmAdapterError> {
    if entry.module() != module.id {
        return Err(LlvmAdapterError::InvalidEntry(
            "native entry belongs to another SSA module".to_owned(),
        ));
    }
    let function = module.function(entry).ok_or_else(|| {
        LlvmAdapterError::InvalidEntry("native entry function does not exist".to_owned())
    })?;
    let entry_block = function.blocks.first().ok_or_else(|| {
        LlvmAdapterError::InvalidEntry("native entry function has no entry block".to_owned())
    })?;
    if !entry_block.parameters.is_empty() {
        return Err(LlvmAdapterError::InvalidEntry(
            "native entry must not have parameters".to_owned(),
        ));
    }
    if !function.return_types.is_empty() {
        return Err(LlvmAdapterError::InvalidEntry(
            "native entry must return Koven Unit".to_owned(),
        ));
    }
    Ok(())
}

pub(super) fn define_wrapper<'ctx>(
    context: &'ctx Context,
    module: &LlvmModule<'ctx>,
    target: FunctionValue<'ctx>,
) -> Result<(), LlvmAdapterError> {
    if module.get_function("main").is_some() {
        return Err(LlvmAdapterError::InvalidEntry(
            "LLVM module already declares native main".to_owned(),
        ));
    }
    let wrapper = module.add_function("main", context.i32_type().fn_type(&[], false), None);
    let block = context.append_basic_block(wrapper, "entry");
    let builder = context.create_builder();
    builder.position_at_end(block);
    builder.build_call(target, &[], "")?;
    builder.build_return(Some(&context.i32_type().const_zero()))?;
    Ok(())
}
