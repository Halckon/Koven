//! 同步非逃逸 scoped callback；只将真实 V 字段地址交给 Borrow 参数。
use super::*;
#[allow(clippy::too_many_arguments)]
pub(in crate::llvm) fn with_value<'ctx>(
    context: &'ctx Context,
    llvm: &LlvmModule<'ctx>,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    types: &TypeMap<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    module: &Module,
    map_type: SsaTypeId,
    header: StructValue<'ctx>,
    key: BasicValueEnum<'ctx>,
    action_type: SsaTypeId,
    action: BasicValueEnum<'ctx>,
    name: &str,
) -> Result<IntValue<'ctx>, LlvmAdapterError> {
    let (_, key_type, value_type) = module
        .map_container(map_type)
        .ok_or_else(|| LlvmAdapterError::InvalidSsa("scoped source is not a Map".into()))?;
    let slot_type = get_slot_type(context, types, key_type, value_type)?;
    let prepared =
        crate::llvm::closure::prepare(builder, module, types, action_type, action, name)?;
    let slot = lookup::find_slot(
        context, llvm, builder, function, types, runtime, module, map_type, header, key, name,
    )?;
    let present = builder.build_is_not_null(slot, "with.present")?;
    let found = context.append_basic_block(function, "with.found");
    let missing = context.append_basic_block(function, "with.missing");
    let done = context.append_basic_block(function, "with.done");
    builder.build_conditional_branch(present, found, missing)?;
    builder.position_at_end(found);
    let value = builder.build_struct_gep(slot_type, slot, 2, "with.value.slot")?;
    if crate::llvm::closure::invoke_prepared(builder, &prepared, &[value.into()], "")?.is_some() {
        return Err(LlvmAdapterError::InvalidSsa(
            "scoped callback must return Unit".into(),
        ));
    }
    let found_end = builder
        .get_insert_block()
        .ok_or_else(|| LlvmAdapterError::InvalidSsa("scoped callback missing exit".into()))?;
    builder.build_unconditional_branch(done)?;
    builder.position_at_end(missing);
    builder.build_unconditional_branch(done)?;
    builder.position_at_end(done);
    let result = builder.build_phi(context.bool_type(), name)?;
    result.add_incoming(&[
        (&context.bool_type().const_int(1, false), found_end),
        (&context.bool_type().const_zero(), missing),
    ]);
    Ok(result.as_basic_value().into_int_value())
}
