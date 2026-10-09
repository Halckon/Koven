//! Map mutation LLVM implementation.
use super::*;

/// 向 MutableMap 插入键值对。若 key 已存在则更新；若达到负载因子 75% 则扩容。
#[allow(clippy::too_many_arguments)]
pub(in crate::llvm) fn put<'ctx>(
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
    value: BasicValueEnum<'ctx>,
    name: &str,
) -> Result<StructValue<'ctx>, LlvmAdapterError> {
    let (_, key_ty, val_ty) = module
        .map_container(map_type)
        .ok_or_else(|| LlvmAdapterError::InvalidSsa("put 目标类型不是 Map".to_owned()))?;
    let slot_type = get_slot_type(context, types, key_ty, val_ty)?;
    let slot_stride = get_slot_stride(llvm, slot_type);

    let old_buffer = builder
        .build_extract_value(map_header, 0, "old_buf")?
        .into_pointer_value();
    let old_size = builder
        .build_extract_value(map_header, 1, "old_size")?
        .into_int_value();
    let old_cap = builder
        .build_extract_value(map_header, 2, "old_cap")?
        .into_int_value();
    let old_tombstones = builder
        .build_extract_value(map_header, 3, "old_tombstones")?
        .into_int_value();

    // 负载因子包含墓碑；rehash 后墓碑数量归零。
    let one = runtime.size_type().const_int(1, false);
    let size_plus_one = builder.build_int_add(old_size, one, "size_p1")?;
    let four = runtime.size_type().const_int(4, false);
    let used_plus_one = builder.build_int_add(size_plus_one, old_tombstones, "used_p1")?;
    let lhs = builder.build_int_mul(used_plus_one, four, "lhs")?;
    let three = runtime.size_type().const_int(3, false);
    let rhs = builder.build_int_mul(old_cap, three, "rhs")?;
    let needs_grow = builder.build_int_compare(IntPredicate::UGE, lhs, rhs, "needs_grow")?;

    let pre_grow_bb = builder.get_insert_block().unwrap();
    let grow_bb = context.append_basic_block(function, "map.grow");
    let insert_bb = context.append_basic_block(function, "map.insert");

    builder.build_conditional_branch(needs_grow, grow_bb, insert_bb)?;

    // 扩容分支
    builder.position_at_end(grow_bb);
    let two = runtime.size_type().const_int(2, false);
    let new_cap = builder.build_int_mul(old_cap, two, "new_cap")?;
    let new_cap_bytes = builder.build_int_mul(
        new_cap,
        runtime.size_type().const_int(slot_stride, false),
        "new_cap_bytes",
    )?;
    let new_buffer =
        runtime.allocate_buffer(llvm, builder, function, new_cap, slot_stride, "grow_buf")?;
    builder.build_memset(new_buffer, 1, context.i8_type().const_zero(), new_cap_bytes)?;

    // 将旧表的所有 Occupied 条目重哈希移动到新表
    rehash_into(
        context, builder, function, runtime, module, key_ty, val_ty, slot_type, old_buffer,
        old_cap, new_buffer, new_cap,
    )?;
    if slot_stride != 0
        && let Some(free_fn) = runtime.free()
    {
        builder.build_call(free_fn, &[BasicMetadataValueEnum::from(old_buffer)], "")?;
    }
    let grow_done_bb = builder.get_insert_block().unwrap();
    builder.build_unconditional_branch(insert_bb)?;

    // 统一插入入口
    builder.position_at_end(insert_bb);
    let ptr_type = context.ptr_type(inkwell::AddressSpace::default());
    let active_buffer = builder.build_phi(ptr_type, "active_buf")?;
    active_buffer.add_incoming(&[(&old_buffer, pre_grow_bb), (&new_buffer, grow_done_bb)]);
    let active_buffer = active_buffer.as_basic_value().into_pointer_value();

    let active_cap = builder.build_phi(runtime.size_type(), "active_cap")?;
    active_cap.add_incoming(&[(&old_cap, pre_grow_bb), (&new_cap, grow_done_bb)]);
    let active_cap = active_cap.as_basic_value().into_int_value();
    let active_tombstones = builder.build_phi(runtime.size_type(), "active_tombstones")?;
    active_tombstones.add_incoming(&[
        (&old_tombstones, pre_grow_bb),
        (&runtime.size_type().const_zero(), grow_done_bb),
    ]);
    let active_tombstones = active_tombstones.as_basic_value().into_int_value();

    let mask = builder.build_int_sub(active_cap, one, "active_mask")?;
    let hash = build_key_hash(
        context, builder, function, runtime, module, key_ty, key, "put_hash",
    )?;
    let start_idx = builder.build_and(hash, mask, "put_start_idx")?;
    let insert_preheader_end = builder.get_insert_block().unwrap();

    let loop_check = context.append_basic_block(function, "put.check");
    let loop_body = context.append_basic_block(function, "put.body");
    let full_scan = context.append_basic_block(function, "put.full_scan");
    let no_slot = context.append_basic_block(function, "put.no_slot");
    let check_key = context.append_basic_block(function, "put.check_key");
    let advance = context.append_basic_block(function, "put.advance");
    let write_slot = context.append_basic_block(function, "put.write");

    builder.build_unconditional_branch(loop_check)?;

    builder.position_at_end(loop_check);
    let idx_phi = builder.build_phi(runtime.size_type(), "put.idx")?;
    idx_phi.add_incoming(&[(&start_idx, insert_preheader_end)]);
    let first_free_phi = builder.build_phi(runtime.size_type(), "put.first_free")?;
    first_free_phi.add_incoming(&[(
        &runtime.size_type().const_int(u64::MAX, false),
        insert_preheader_end,
    )]);
    let steps_phi = builder.build_phi(runtime.size_type(), "put.steps")?;
    steps_phi.add_incoming(&[(&runtime.size_type().const_zero(), insert_preheader_end)]);

    let current_idx = idx_phi.as_basic_value().into_int_value();
    let current_first_free = first_free_phi.as_basic_value().into_int_value();
    let current_steps = steps_phi.as_basic_value().into_int_value();
    let scanned_all = builder.build_int_compare(
        IntPredicate::UGE,
        current_steps,
        active_cap,
        "put.scanned_all",
    )?;
    builder.build_conditional_branch(scanned_all, full_scan, loop_body)?;

    builder.position_at_end(full_scan);
    let found_tombstone = builder.build_int_compare(
        IntPredicate::NE,
        current_first_free,
        runtime.size_type().const_int(u64::MAX, false),
        "put.found_tombstone",
    )?;
    builder.build_conditional_branch(found_tombstone, write_slot, no_slot)?;
    builder.position_at_end(no_slot);
    runtime.emit_abort(builder)?;

    builder.position_at_end(loop_body);
    // SAFETY: the verified slot layout and capacity-bounded index describe this buffer access.
    let slot_ptr =
        unsafe { builder.build_gep(slot_type, active_buffer, &[current_idx], "slot_ptr")? };
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
    let check_occupied = context.append_basic_block(function, "put.check_occ");
    builder.build_conditional_branch(is_empty, write_slot, check_occupied)?;

    builder.position_at_end(check_occupied);
    let is_occupied = builder.build_int_compare(
        IntPredicate::EQ,
        state,
        context.i32_type().const_int(SLOT_STATE_OCCUPIED, false),
        "is_occ",
    )?;
    let note_tombstone = context.append_basic_block(function, "put.note_tomb");
    builder.build_conditional_branch(is_occupied, check_key, note_tombstone)?;

    // 若是 Tombstone，记录首个 Tombstone 槽位
    builder.position_at_end(note_tombstone);
    let has_no_free = builder.build_int_compare(
        IntPredicate::EQ,
        current_first_free,
        runtime.size_type().const_int(u64::MAX, false),
        "no_free",
    )?;
    let updated_free = builder
        .build_select(has_no_free, current_idx, current_first_free, "updated_free")?
        .into_int_value();
    let note_tomb_bb = builder.get_insert_block().unwrap();
    builder.build_unconditional_branch(advance)?;

    // 检查已占用槽位的 key 是否相等
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
        context, builder, function, runtime, module, key_ty, key, slot_key, "put.cmp",
    )?;
    let check_key_end_bb = builder.get_insert_block().unwrap();
    let overwrite = context.append_basic_block(function, "put.overwrite");
    builder.build_conditional_branch(key_matches, overwrite, advance)?;

    // 键已存在：覆写 value，size 保持不变
    builder.position_at_end(overwrite);
    drop::entry(
        context, builder, types, runtime, module, slot_type, slot_ptr, key_ty, val_ty,
    )?;
    builder.build_store(key_ptr, key)?;
    // SAFETY: the verified slot layout and capacity-bounded index describe this buffer access.
    let val_ptr = unsafe {
        builder.build_gep(
            slot_type,
            slot_ptr,
            &[
                context.i32_type().const_zero(),
                context.i32_type().const_int(2, false),
            ],
            "slot_val_ptr",
        )?
    };
    builder.build_store(val_ptr, value)?;
    let overwrite_bb = builder.get_insert_block().unwrap();
    let finish_bb = context.append_basic_block(function, "put.finish");
    builder.build_unconditional_branch(finish_bb)?;

    // 探查前进
    builder.position_at_end(advance);
    let free_for_advance = builder.build_phi(runtime.size_type(), "free_adv")?;
    free_for_advance.add_incoming(&[
        (&updated_free, note_tomb_bb),
        (&current_first_free, check_key_end_bb),
    ]);
    let next_idx = builder.build_int_add(current_idx, one, "next_idx")?;
    let next_steps = builder.build_int_add(current_steps, one, "put.next_steps")?;
    let wrapped_idx = builder.build_and(next_idx, mask, "wrapped_idx")?;
    let advance_bb = builder.get_insert_block().unwrap();
    builder.build_unconditional_branch(loop_check)?;
    idx_phi.add_incoming(&[(&wrapped_idx, advance_bb)]);
    first_free_phi.add_incoming(&[(
        &free_for_advance.as_basic_value().into_int_value(),
        advance_bb,
    )]);
    steps_phi.add_incoming(&[(&next_steps, advance_bb)]);

    // 写入新槽位（优先使用首个 Tombstone，若无则使用当前 Empty 槽位）
    builder.position_at_end(write_slot);
    let has_tomb = builder.build_int_compare(
        IntPredicate::NE,
        current_first_free,
        runtime.size_type().const_int(u64::MAX, false),
        "has_tomb",
    )?;
    let target_idx = builder
        .build_select(has_tomb, current_first_free, current_idx, "target_idx")?
        .into_int_value();
    // SAFETY: the verified slot layout and capacity-bounded index describe this buffer access.
    let target_slot_ptr =
        unsafe { builder.build_gep(slot_type, active_buffer, &[target_idx], "target_slot_ptr")? };
    // SAFETY: the verified slot layout and capacity-bounded index describe this buffer access.
    let target_state_ptr = unsafe {
        builder.build_gep(
            slot_type,
            target_slot_ptr,
            &[
                context.i32_type().const_zero(),
                context.i32_type().const_zero(),
            ],
            "target_state_ptr",
        )?
    };
    // SAFETY: the verified slot layout and capacity-bounded index describe this buffer access.
    let target_key_ptr = unsafe {
        builder.build_gep(
            slot_type,
            target_slot_ptr,
            &[
                context.i32_type().const_zero(),
                context.i32_type().const_int(1, false),
            ],
            "target_key_ptr",
        )?
    };
    // SAFETY: the verified slot layout and capacity-bounded index describe this buffer access.
    let target_val_ptr = unsafe {
        builder.build_gep(
            slot_type,
            target_slot_ptr,
            &[
                context.i32_type().const_zero(),
                context.i32_type().const_int(2, false),
            ],
            "target_val_ptr",
        )?
    };

    builder.build_store(
        target_state_ptr,
        context.i32_type().const_int(SLOT_STATE_OCCUPIED, false),
    )?;
    builder.build_store(target_key_ptr, key)?;
    builder.build_store(target_val_ptr, value)?;
    let new_size = builder.build_int_add(old_size, one, "new_size")?;
    let reused_tombstone = builder
        .build_select(
            has_tomb,
            one,
            runtime.size_type().const_zero(),
            "reused_tombstone",
        )?
        .into_int_value();
    let remaining_tombstones =
        builder.build_int_sub(active_tombstones, reused_tombstone, "remaining_tombstones")?;
    let write_done_bb = builder.get_insert_block().unwrap();
    builder.build_unconditional_branch(finish_bb)?;

    // 结束：产出更新后的 Header
    builder.position_at_end(finish_bb);
    let final_size = builder.build_phi(runtime.size_type(), "final_size")?;
    final_size.add_incoming(&[(&old_size, overwrite_bb), (&new_size, write_done_bb)]);
    let final_size = final_size.as_basic_value().into_int_value();
    let final_tombstones = builder.build_phi(runtime.size_type(), "final_tombstones")?;
    final_tombstones.add_incoming(&[
        (&active_tombstones, overwrite_bb),
        (&remaining_tombstones, write_done_bb),
    ]);

    let header_type = types.basic_type(map_type)?.into_struct_type();
    build_header(
        builder,
        header_type,
        active_buffer,
        final_size,
        active_cap,
        final_tombstones.as_basic_value().into_int_value(),
        name,
    )
}

/// 从 MutableMap 移除指定 key。若命中则将状态置为 Deleted 并 size -= 1。
#[allow(clippy::too_many_arguments)]
pub(in crate::llvm) fn remove<'ctx>(
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
) -> Result<(StructValue<'ctx>, IntValue<'ctx>, BasicValueEnum<'ctx>), LlvmAdapterError> {
    let (_, key_ty, val_ty) = module
        .map_container(map_type)
        .ok_or_else(|| LlvmAdapterError::InvalidSsa("remove 目标类型不是 Map".to_owned()))?;
    let slot_type = get_slot_type(context, types, key_ty, val_ty)?;

    let buffer = builder
        .build_extract_value(map_header, 0, "rem_buf")?
        .into_pointer_value();
    let size = builder
        .build_extract_value(map_header, 1, "rem_size")?
        .into_int_value();
    let capacity = builder
        .build_extract_value(map_header, 2, "rem_cap")?
        .into_int_value();
    let tombstones = builder
        .build_extract_value(map_header, 3, "rem.tombstones")?
        .into_int_value();

    let preheader = builder.get_insert_block().unwrap();
    let finish_bb = context.append_basic_block(function, "rem.finish");
    let is_cap_zero = builder.build_int_compare(
        IntPredicate::EQ,
        capacity,
        runtime.size_type().const_zero(),
        "is_zero_cap",
    )?;

    let has_cap = context.append_basic_block(function, "rem.has_cap");
    let loop_check = context.append_basic_block(function, "rem.check");
    let loop_body = context.append_basic_block(function, "rem.body");
    let check_key = context.append_basic_block(function, "rem.check_key");
    let advance = context.append_basic_block(function, "rem.advance");
    let do_remove = context.append_basic_block(function, "rem.execute");

    builder.build_conditional_branch(is_cap_zero, finish_bb, has_cap)?;

    builder.position_at_end(has_cap);
    let one = runtime.size_type().const_int(1, false);
    let mask = builder.build_int_sub(capacity, one, "mask")?;
    let hash = build_key_hash(
        context, builder, function, runtime, module, key_ty, key, "rem_hash",
    )?;
    let start_idx = builder.build_and(hash, mask, "rem_start_idx")?;
    let has_cap_end_bb = builder.get_insert_block().unwrap();
    builder.build_unconditional_branch(loop_check)?;

    builder.position_at_end(loop_check);
    let idx_phi = builder.build_phi(runtime.size_type(), "rem.idx")?;
    idx_phi.add_incoming(&[(&start_idx, has_cap_end_bb)]);
    let step_phi = builder.build_phi(runtime.size_type(), "rem.step")?;
    step_phi.add_incoming(&[(&runtime.size_type().const_zero(), has_cap_end_bb)]);

    let current_idx = idx_phi.as_basic_value().into_int_value();
    let current_step = step_phi.as_basic_value().into_int_value();

    let step_overflow =
        builder.build_int_compare(IntPredicate::UGE, current_step, capacity, "step_overflow")?;
    builder.build_conditional_branch(step_overflow, finish_bb, loop_body)?;

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
    let check_occupied = context.append_basic_block(function, "rem.check_occ");
    builder.build_conditional_branch(is_empty, finish_bb, check_occupied)?;

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
        context, builder, function, runtime, module, key_ty, key, slot_key, "rem.cmp",
    )?;
    builder.build_conditional_branch(key_matches, do_remove, advance)?;

    // 移除匹配项：标记状态为 Deleted，size - 1
    builder.position_at_end(do_remove);
    let value_ptr = builder.build_struct_gep(slot_type, slot_ptr, 2, "rem.value.ptr")?;
    let removed = builder.build_load(types.basic_type(val_ty)?, value_ptr, "rem.value")?;
    if module.type_ownership(key_ty) == Some(crate::ssa::model::Ownership::MoveOnly) {
        runtime.emit_drop(builder, key_ty, slot_key)?;
    }
    builder.build_store(
        state_ptr,
        context.i32_type().const_int(SLOT_STATE_DELETED, false),
    )?;
    let new_size = builder.build_int_sub(size, one, "rem.new_size")?;
    let new_tombstones = builder.build_int_add(tombstones, one, "rem.new_tombstones")?;
    let do_remove_bb = builder.get_insert_block().unwrap();
    builder.build_unconditional_branch(finish_bb)?;

    // 探查前进
    builder.position_at_end(advance);
    let next_idx = builder.build_int_add(current_idx, one, "next_idx")?;
    let wrapped_idx = builder.build_and(next_idx, mask, "wrapped_idx")?;
    let next_step = builder.build_int_add(current_step, one, "next_step")?;
    let advance_bb = builder.get_insert_block().unwrap();
    builder.build_unconditional_branch(loop_check)?;
    idx_phi.add_incoming(&[(&wrapped_idx, advance_bb)]);
    step_phi.add_incoming(&[(&next_step, advance_bb)]);

    // 结束：产出更新后的 Header
    builder.position_at_end(finish_bb);
    let final_size = builder.build_phi(runtime.size_type(), "rem.final_size")?;
    final_size.add_incoming(&[
        (&size, preheader),
        (&size, loop_check),
        (&size, loop_body),
        (&new_size, do_remove_bb),
    ]);
    let final_size = final_size.as_basic_value().into_int_value();
    let final_tombstones = builder.build_phi(runtime.size_type(), "rem.final_tombstones")?;
    final_tombstones.add_incoming(&[
        (&tombstones, preheader),
        (&tombstones, loop_check),
        (&tombstones, loop_body),
        (&new_tombstones, do_remove_bb),
    ]);

    let absent = types.basic_type(val_ty)?.const_zero();
    let result = builder.build_phi(types.basic_type(val_ty)?, "rem.result")?;
    result.add_incoming(&[
        (&absent, preheader),
        (&absent, loop_check),
        (&absent, loop_body),
        (&removed, do_remove_bb),
    ]);
    let present = builder.build_phi(context.bool_type(), "rem.present")?;
    present.add_incoming(&[
        (&context.bool_type().const_zero(), preheader),
        (&context.bool_type().const_zero(), loop_check),
        (&context.bool_type().const_zero(), loop_body),
        (&context.bool_type().const_int(1, false), do_remove_bb),
    ]);
    let header_type = types.basic_type(map_type)?.into_struct_type();
    Ok((
        build_header(
            builder,
            header_type,
            buffer,
            final_size,
            capacity,
            final_tombstones.as_basic_value().into_int_value(),
            name,
        )?,
        present.as_basic_value().into_int_value(),
        result.as_basic_value(),
    ))
}
