//! Rehash 沿插入链搬迁 owner，在新 bucket 中重建索引链。
use super::*;

pub(super) struct RehashInput<'ctx> {
    pub(super) slot_type: StructType<'ctx>,
    pub(super) key_type: SsaTypeId,
    pub(super) old_buffer: PointerValue<'ctx>,
    pub(super) old_first: IntValue<'ctx>,
    pub(super) old_size: IntValue<'ctx>,
    pub(super) new_buffer: PointerValue<'ctx>,
    pub(super) new_cap: IntValue<'ctx>,
}

pub(super) fn rehash_into<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    module: &Module,
    input: RehashInput<'ctx>,
) -> Result<OrderEnds<'ctx>, LlvmAdapterError> {
    let preheader = builder.get_insert_block().unwrap();
    let loop_check = context.append_basic_block(function, "rehash.check");
    let loop_body = context.append_basic_block(function, "rehash.body");
    let done = context.append_basic_block(function, "rehash.done");
    let size_type = runtime.size_type();
    let one = size_type.const_int(1, false);
    let none = size_type.const_all_ones();
    let new_mask = builder.build_int_sub(input.new_cap, one, "new_mask")?;
    builder.build_unconditional_branch(loop_check)?;

    builder.position_at_end(loop_check);
    let index = builder.build_phi(size_type, "rehash.index")?;
    index.add_incoming(&[(&input.old_first, preheader)]);
    let moved = builder.build_phi(size_type, "rehash.moved")?;
    moved.add_incoming(&[(&size_type.const_zero(), preheader)]);
    let first = builder.build_phi(size_type, "rehash.first")?;
    first.add_incoming(&[(&none, preheader)]);
    let last = builder.build_phi(size_type, "rehash.last")?;
    last.add_incoming(&[(&none, preheader)]);
    let current_index = index.as_basic_value().into_int_value();
    let current_moved = moved.as_basic_value().into_int_value();
    let current_order = OrderEnds {
        first: first.as_basic_value().into_int_value(),
        last: last.as_basic_value().into_int_value(),
    };
    // This count only bounds work. Append/unlink preserve the separate invariant that
    // the old chain contains exactly old_size distinct, in-range occupied slots.
    let finished = builder.build_int_compare(
        IntPredicate::UGE,
        current_moved,
        input.old_size,
        "rehash.finished",
    )?;
    builder.build_conditional_branch(finished, done, loop_body)?;

    builder.position_at_end(loop_body);
    let slot_type = input.slot_type;
    // SAFETY: the preserved chain invariant supplies a live in-range slot for each iteration.
    let old_slot = unsafe {
        builder.build_gep(
            slot_type,
            input.old_buffer,
            &[current_index],
            "rehash.old_slot",
        )?
    };
    let next_ptr = builder.build_struct_gep(slot_type, old_slot, 4, "rehash.old_next.ptr")?;
    let old_next = builder
        .build_load(size_type, next_ptr, "rehash.old_next")?
        .into_int_value();
    let key_ptr = builder.build_struct_gep(slot_type, old_slot, 1, "rehash.old_key.ptr")?;
    let value_ptr = builder.build_struct_gep(slot_type, old_slot, 2, "rehash.old_value.ptr")?;
    let key = builder.build_load(
        slot_type.get_field_type_at_index(1).unwrap(),
        key_ptr,
        "rehash.key",
    )?;
    let value = builder.build_load(
        slot_type.get_field_type_at_index(2).unwrap(),
        value_ptr,
        "rehash.value",
    )?;
    let hash = build_key_hash(
        context,
        builder,
        function,
        runtime,
        module,
        input.key_type,
        key,
        "rehash.hash",
    )?;
    let start = builder.build_and(hash, new_mask, "rehash.start")?;
    // String hashing may have introduced blocks; use its actual exit for the incoming edge.
    let hashed = builder.get_insert_block().unwrap();
    let probe = context.append_basic_block(function, "rehash.probe");
    let probe_write = context.append_basic_block(function, "rehash.write");
    let probe_advance = context.append_basic_block(function, "rehash.probe_advance");
    builder.build_unconditional_branch(probe)?;

    builder.position_at_end(probe);
    let probe_index = builder.build_phi(size_type, "rehash.probe_index")?;
    probe_index.add_incoming(&[(&start, hashed)]);
    let target_index = probe_index.as_basic_value().into_int_value();
    // SAFETY: masking bounds the index; the checked allocation contains new_cap slots.
    let new_slot = unsafe {
        builder.build_gep(
            slot_type,
            input.new_buffer,
            &[target_index],
            "rehash.new_slot",
        )?
    };
    let state_ptr = builder.build_struct_gep(slot_type, new_slot, 0, "rehash.state.ptr")?;
    let state = builder
        .build_load(context.i32_type(), state_ptr, "rehash.state")?
        .into_int_value();
    let empty = builder.build_int_compare(
        IntPredicate::EQ,
        state,
        context.i32_type().const_int(SLOT_STATE_EMPTY, false),
        "rehash.empty",
    )?;
    // old_size <= old_cap < new_cap, so a new table without tombstones always has an empty slot.
    builder.build_conditional_branch(empty, probe_write, probe_advance)?;

    builder.position_at_end(probe_advance);
    let next_probe = builder.build_int_add(target_index, one, "rehash.next_probe")?;
    let wrapped = builder.build_and(next_probe, new_mask, "rehash.wrapped")?;
    let advanced = builder.get_insert_block().unwrap();
    builder.build_unconditional_branch(probe)?;
    probe_index.add_incoming(&[(&wrapped, advanced)]);

    builder.position_at_end(probe_write);
    let key_ptr = builder.build_struct_gep(slot_type, new_slot, 1, "rehash.new_key.ptr")?;
    let value_ptr = builder.build_struct_gep(slot_type, new_slot, 2, "rehash.new_value.ptr")?;
    builder.build_store(key_ptr, key)?;
    builder.build_store(value_ptr, value)?;
    builder.build_store(
        state_ptr,
        context.i32_type().const_int(SLOT_STATE_OCCUPIED, false),
    )?;
    let appended = order::append(
        context,
        builder,
        function,
        SlotTable {
            slot_type,
            buckets: input.new_buffer,
        },
        current_order,
        target_index,
        "rehash.order",
    )?;
    let next_moved = builder.build_int_add(current_moved, one, "rehash.next_moved")?;
    let appended_block = builder.get_insert_block().unwrap();
    builder.build_unconditional_branch(loop_check)?;
    index.add_incoming(&[(&old_next, appended_block)]);
    moved.add_incoming(&[(&next_moved, appended_block)]);
    first.add_incoming(&[(&appended.first, appended_block)]);
    last.add_incoming(&[(&appended.last, appended_block)]);

    builder.position_at_end(done);
    Ok(current_order)
}
