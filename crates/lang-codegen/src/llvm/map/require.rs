//! 只读确定槽位位置；Missing 走既定 Abort，不读取或交付 V owner。
use super::*;
#[allow(clippy::too_many_arguments)]
pub(in crate::llvm) fn require_value<'ctx>(
    context: &'ctx Context,
    llvm: &LlvmModule<'ctx>,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    types: &TypeMap<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    module: &Module,
    map_type: SsaTypeId,
    map_header: StructValue<'ctx>,
    key: BasicValueEnum<'ctx>,
    name: &str,
) -> Result<PointerValue<'ctx>, LlvmAdapterError> {
    let (_, key_type, value_type) = module
        .map_container(map_type)
        .ok_or_else(|| LlvmAdapterError::InvalidSsa("required target is not a Map".into()))?;
    let slot_type = get_slot_type(context, types, key_type, value_type)?;
    let slot = lookup::find_slot(
        context, llvm, builder, function, types, runtime, module, map_type, map_header, key, name,
    )?;
    let present = builder.build_is_not_null(slot, "required.present")?;
    let found = context.append_basic_block(function, "required.found");
    let missing = context.append_basic_block(function, "required.missing");
    builder.build_conditional_branch(present, found, missing)?;
    builder.position_at_end(missing);
    runtime.emit_abort(builder)?;
    builder.position_at_end(found);
    Ok(builder.build_struct_gep(slot_type, slot, 2, name)?)
}
