//! Map rehash LLVM implementation.
use super::*;

/// 将旧哈希表的所有有效 (Occupied) 键值对搬移重新哈希到新哈希表中。
#[allow(clippy::too_many_arguments)]
pub(super) fn rehash_into<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    module: &Module,
    key_ty: SsaTypeId,
    _val_ty: SsaTypeId,
    slot_type: StructType<'ctx>,
    old_buffer: PointerValue<'ctx>,
    old_cap: IntValue<'ctx>,
    new_buffer: PointerValue<'ctx>,
    new_cap: IntValue<'ctx>,
) -> Result<(), LlvmAdapterError> {
    let preheader = builder.get_insert_block().unwrap();
    let loop_check = context.append_basic_block(function, "rehash.check");
    let loop_body = context.append_basic_block(function, "rehash.body");
    let advance = context.append_basic_block(function, "rehash.advance");
    let done = context.append_basic_block(function, "rehash.done");

    let one = runtime.size_type().const_int(1, false);
    let new_mask = builder.build_int_sub(new_cap, one, "new_mask")?;
    builder.build_unconditional_branch(loop_check)?;

    builder.position_at_end(loop_check);
    let idx_phi = builder.build_phi(runtime.size_type(), "rehash.idx")?;
    idx_phi.add_incoming(&[(&runtime.size_type().const_zero(), preheader)]);
    let current_idx = idx_phi.as_basic_value().into_int_value();
    let finished =
        builder.build_int_compare(IntPredicate::UGE, current_idx, old_cap, "rehash.finished")?;
    builder.build_conditional_branch(finished, done, loop_body)?;

    builder.position_at_end(loop_body);
    // SAFETY: the verified slot layout and capacity-bounded index describe this buffer access.
    let old_slot = unsafe { builder.build_gep(slot_type, old_buffer, &[current_idx], "old_slot")? };
    // SAFETY: the verified slot layout and capacity-bounded index describe this buffer access.
    let state_ptr = unsafe {
        builder.build_gep(
            slot_type,
            old_slot,
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
    let is_occ = builder.build_int_compare(
        IntPredicate::EQ,
        state,
        context.i32_type().const_int(SLOT_STATE_OCCUPIED, false),
        "is_occ",
    )?;

    let insert_entry = context.append_basic_block(function, "rehash.insert");
    builder.build_conditional_branch(is_occ, insert_entry, advance)?;

    builder.position_at_end(insert_entry);
    // SAFETY: the verified slot layout and capacity-bounded index describe this buffer access.
    let key_ptr = unsafe {
        builder.build_gep(
            slot_type,
            old_slot,
            &[
                context.i32_type().const_zero(),
                context.i32_type().const_int(1, false),
            ],
            "old_key_ptr",
        )?
    };
    // SAFETY: the verified slot layout and capacity-bounded index describe this buffer access.
    let val_ptr = unsafe {
        builder.build_gep(
            slot_type,
            old_slot,
            &[
                context.i32_type().const_zero(),
                context.i32_type().const_int(2, false),
            ],
            "old_val_ptr",
        )?
    };
    let key = builder.build_load(
        slot_type.get_field_type_at_index(1).unwrap(),
        key_ptr,
        "rehash_key",
    )?;
    let val = builder.build_load(
        slot_type.get_field_type_at_index(2).unwrap(),
        val_ptr,
        "rehash_val",
    )?;

    let hash = build_key_hash(
        context,
        builder,
        function,
        runtime,
        module,
        key_ty,
        key,
        "rehash_hash",
    )?;
    let start_new_idx = builder.build_and(hash, new_mask, "start_new_idx")?;

    // 探查新表寻找空槽
    let probe_check = context.append_basic_block(function, "rehash.probe");
    let probe_body = context.append_basic_block(function, "rehash.probe_body");
    let probe_write = context.append_basic_block(function, "rehash.probe_write");
    let probe_advance = context.append_basic_block(function, "rehash.probe_adv");
    let insert_entry_bb = builder.get_insert_block().unwrap();
    builder.build_unconditional_branch(probe_check)?;

    builder.position_at_end(probe_check);
    let probe_idx = builder.build_phi(runtime.size_type(), "probe_idx")?;
    probe_idx.add_incoming(&[(&start_new_idx, insert_entry_bb)]);
    let cur_probe_idx = probe_idx.as_basic_value().into_int_value();
    builder.build_unconditional_branch(probe_body)?;

    builder.position_at_end(probe_body);
    // SAFETY: the verified slot layout and capacity-bounded index describe this buffer access.
    let new_slot =
        unsafe { builder.build_gep(slot_type, new_buffer, &[cur_probe_idx], "new_slot")? };
    // SAFETY: the verified slot layout and capacity-bounded index describe this buffer access.
    let new_state_ptr = unsafe {
        builder.build_gep(
            slot_type,
            new_slot,
            &[
                context.i32_type().const_zero(),
                context.i32_type().const_zero(),
            ],
            "new_state_ptr",
        )?
    };
    let new_state = builder
        .build_load(context.i32_type(), new_state_ptr, "new_state")?
        .into_int_value();
    let is_empty = builder.build_int_compare(
        IntPredicate::EQ,
        new_state,
        context.i32_type().const_int(SLOT_STATE_EMPTY, false),
        "new_is_empty",
    )?;
    builder.build_conditional_branch(is_empty, probe_write, probe_advance)?;

    builder.position_at_end(probe_advance);
    let next_probe = builder.build_int_add(cur_probe_idx, one, "next_probe")?;
    let wrapped_probe = builder.build_and(next_probe, new_mask, "wrapped_probe")?;
    let probe_adv_bb = builder.get_insert_block().unwrap();
    builder.build_unconditional_branch(probe_check)?;
    probe_idx.add_incoming(&[(&wrapped_probe, probe_adv_bb)]);

    builder.position_at_end(probe_write);
    builder.build_store(
        new_state_ptr,
        context.i32_type().const_int(SLOT_STATE_OCCUPIED, false),
    )?;
    // SAFETY: the verified slot layout and capacity-bounded index describe this buffer access.
    let new_key_ptr = unsafe {
        builder.build_gep(
            slot_type,
            new_slot,
            &[
                context.i32_type().const_zero(),
                context.i32_type().const_int(1, false),
            ],
            "new_key_ptr",
        )?
    };
    // SAFETY: the verified slot layout and capacity-bounded index describe this buffer access.
    let new_val_ptr = unsafe {
        builder.build_gep(
            slot_type,
            new_slot,
            &[
                context.i32_type().const_zero(),
                context.i32_type().const_int(2, false),
            ],
            "new_val_ptr",
        )?
    };
    builder.build_store(new_key_ptr, key)?;
    builder.build_store(new_val_ptr, val)?;
    builder.build_unconditional_branch(advance)?;

    builder.position_at_end(advance);
    let next_idx = builder.build_int_add(current_idx, one, "next_idx")?;
    let advance_bb = builder.get_insert_block().unwrap();
    builder.build_unconditional_branch(loop_check)?;
    idx_phi.add_incoming(&[(&next_idx, advance_bb)]);

    builder.position_at_end(done);
    Ok(())
}
