//! Map lookup LLVM implementation.
use super::*;

/// 检查 Map 是否包含指定 key。
#[allow(clippy::too_many_arguments)]
pub(super) fn find_slot<'ctx>(
    context: &'ctx Context,
    _llvm: &LlvmModule<'ctx>,
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
    let (_, key_ty, val_ty) = module
        .map_container(map_type)
        .ok_or_else(|| LlvmAdapterError::InvalidSsa("contains 目标类型不是 Map".to_owned()))?;
    let slot_type = get_slot_type(context, types, key_ty, val_ty)?;

    let buffer = builder
        .build_extract_value(map_header, 0, &format!("{name}.buffer"))?
        .into_pointer_value();
    let capacity = builder
        .build_extract_value(map_header, 2, &format!("{name}.cap"))?
        .into_int_value();

    let has_cap = context.append_basic_block(function, &format!("{name}.has_cap"));
    let loop_check = context.append_basic_block(function, &format!("{name}.check"));
    let loop_body = context.append_basic_block(function, &format!("{name}.probe"));
    let check_key = context.append_basic_block(function, &format!("{name}.check_key"));
    let advance = context.append_basic_block(function, &format!("{name}.advance"));
    let found = context.append_basic_block(function, &format!("{name}.found"));
    let not_found = context.append_basic_block(function, &format!("{name}.not_found"));
    let ready = context.append_basic_block(function, &format!("{name}.ready"));

    let is_cap_zero = builder.build_int_compare(
        IntPredicate::EQ,
        capacity,
        runtime.size_type().const_zero(),
        "is_zero_cap",
    )?;
    builder.build_conditional_branch(is_cap_zero, not_found, has_cap)?;

    // 计算初始 hash 与索引
    builder.position_at_end(has_cap);
    let one = runtime.size_type().const_int(1, false);
    let mask = builder.build_int_sub(capacity, one, "mask")?;
    let hash = build_key_hash(
        context, builder, function, runtime, module, key_ty, key, name,
    )?;
    let start_idx = builder.build_and(hash, mask, "start_idx")?;
    let has_cap_end_bb = builder.get_insert_block().unwrap();
    builder.build_unconditional_branch(loop_check)?;

    // 探查循环
    builder.position_at_end(loop_check);
    let idx_phi = builder.build_phi(runtime.size_type(), &format!("{name}.idx"))?;
    idx_phi.add_incoming(&[(&start_idx, has_cap_end_bb)]);
    let step_phi = builder.build_phi(runtime.size_type(), &format!("{name}.step"))?;
    step_phi.add_incoming(&[(&runtime.size_type().const_zero(), has_cap_end_bb)]);

    let current_idx = idx_phi.as_basic_value().into_int_value();
    let current_step = step_phi.as_basic_value().into_int_value();

    let step_overflow =
        builder.build_int_compare(IntPredicate::UGE, current_step, capacity, "step_overflow")?;
    builder.build_conditional_branch(step_overflow, not_found, loop_body)?;

    builder.position_at_end(loop_body);
    // SAFETY: the verified slot layout and capacity-bounded index describe this buffer access.
    let slot_ptr = unsafe { builder.build_gep(slot_type, buffer, &[current_idx], "slot_ptr")? };
    // SAFETY: the verified slot layout and capacity-bounded index describe this buffer access.
    let state_ptr = unsafe {
        builder.build_gep(
            slot_type,
            slot_ptr,
            &[
                context.i32_type().const_zero(),
                context.i32_type().const_zero(),
            ],
            "state_ptr",
        )?
    };
    let state = builder
        .build_load(context.i32_type(), state_ptr, "state")?
        .into_int_value();

    let is_empty = builder.build_int_compare(
        IntPredicate::EQ,
        state,
        context.i32_type().const_int(SLOT_STATE_EMPTY, false),
        "is_empty",
    )?;
    let check_occupied = context.append_basic_block(function, "check_occ");
    builder.build_conditional_branch(is_empty, not_found, check_occupied)?;

    builder.position_at_end(check_occupied);
    let is_occupied = builder.build_int_compare(
        IntPredicate::EQ,
        state,
        context.i32_type().const_int(SLOT_STATE_OCCUPIED, false),
        "is_occ",
    )?;
    builder.build_conditional_branch(is_occupied, check_key, advance)?;

    builder.position_at_end(check_key);
    // SAFETY: the verified slot layout and capacity-bounded index describe this buffer access.
    let key_ptr = unsafe {
        builder.build_gep(
            slot_type,
            slot_ptr,
            &[
                context.i32_type().const_zero(),
                context.i32_type().const_int(1, false),
            ],
            "slot_key_ptr",
        )?
    };
    let slot_key = builder.build_load(types.basic_type(key_ty)?, key_ptr, "slot_key")?;
    let key_matches = build_key_equal(
        context, builder, function, runtime, module, key_ty, key, slot_key, name,
    )?;
    builder.build_conditional_branch(key_matches, found, advance)?;

    builder.position_at_end(advance);
    let next_idx = builder.build_int_add(current_idx, one, "next_idx")?;
    let wrapped_idx = builder.build_and(next_idx, mask, "wrapped_idx")?;
    let next_step = builder.build_int_add(current_step, one, "next_step")?;
    let advance_bb = builder.get_insert_block().unwrap();
    builder.build_unconditional_branch(loop_check)?;
    idx_phi.add_incoming(&[(&wrapped_idx, advance_bb)]);
    step_phi.add_incoming(&[(&next_step, advance_bb)]);

    builder.position_at_end(found);
    builder.build_unconditional_branch(ready)?;
    let found_bb = builder.get_insert_block().unwrap();

    builder.position_at_end(not_found);
    builder.build_unconditional_branch(ready)?;
    let not_found_bb = builder.get_insert_block().unwrap();

    builder.position_at_end(ready);
    let result_phi = builder.build_phi(buffer.get_type(), name)?;
    result_phi.add_incoming(&[
        (&slot_ptr, found_bb),
        (&buffer.get_type().const_null(), not_found_bb),
    ]);
    Ok(result_phi.as_basic_value().into_pointer_value())
}

/// 共享有界 lookup，返回包含性，不读取 V。
#[allow(clippy::too_many_arguments)]
pub(in crate::llvm) fn contains<'ctx>(
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
) -> Result<IntValue<'ctx>, LlvmAdapterError> {
    let slot = find_slot(
        context, llvm, builder, function, types, runtime, module, map_type, map_header, key, name,
    )?;
    Ok(builder.build_is_not_null(slot, name)?)
}

/// Copyable 按值查询使用同一 lookup；缺失保留独立 presence。
#[allow(clippy::too_many_arguments)]
pub(in crate::llvm) fn get<'ctx>(
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
) -> Result<(IntValue<'ctx>, BasicValueEnum<'ctx>), LlvmAdapterError> {
    let (_, key_type, value_type) = module
        .map_container(map_type)
        .ok_or_else(|| LlvmAdapterError::InvalidSsa("get target is not a Map".into()))?;
    let slot_type = get_slot_type(context, types, key_type, value_type)?;
    let value_llvm_type = types.basic_type(value_type)?;
    let slot = find_slot(
        context, llvm, builder, function, types, runtime, module, map_type, map_header, key, name,
    )?;
    let present = builder.build_is_not_null(slot, &format!("{name}.present"))?;
    let found = context.append_basic_block(function, &format!("{name}.payload.found"));
    let absent = context.append_basic_block(function, &format!("{name}.payload.absent"));
    let ready = context.append_basic_block(function, &format!("{name}.payload.ready"));
    builder.build_conditional_branch(present, found, absent)?;
    builder.position_at_end(found);
    let pointer = builder.build_struct_gep(slot_type, slot, 2, "value.slot")?;
    let value = builder.build_load(value_llvm_type, pointer, name)?;
    builder.build_unconditional_branch(ready)?;
    builder.position_at_end(absent);
    builder.build_unconditional_branch(ready)?;
    builder.position_at_end(ready);
    let payload = builder.build_phi(value_llvm_type, &format!("{name}.payload"))?;
    payload.add_incoming(&[(&value, found), (&value_llvm_type.const_zero(), absent)]);
    Ok((present, payload.as_basic_value()))
}
