//! Occupied 条目拥有 key/value；覆写、丢弃 remove 结果和容器收尾各释放一次。
use super::*;
use crate::ssa::model::Ownership;

#[allow(clippy::too_many_arguments)]
pub(super) fn entry<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    types: &TypeMap<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    module: &Module,
    slot_type: StructType<'ctx>,
    slot: PointerValue<'ctx>,
    key: SsaTypeId,
    value: SsaTypeId,
) -> Result<(), LlvmAdapterError> {
    for (index, ty) in [(1, key), (2, value)] {
        if module.type_ownership(ty) != Some(Ownership::MoveOnly) {
            continue;
        }
        let pointer = builder.build_struct_gep(slot_type, slot, index, "map.drop.field")?;
        let field = builder.build_load(types.basic_type(ty)?, pointer, "map.drop.value")?;
        runtime.emit_drop(builder, ty, field)?;
    }
    let _ = context;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn owner<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    types: &TypeMap<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    module: &Module,
    map_type: SsaTypeId,
    header: StructValue<'ctx>,
) -> Result<(), LlvmAdapterError> {
    let (_, key, value) = module
        .map_container(map_type)
        .ok_or_else(|| LlvmAdapterError::InvalidSsa("map drop requires map type".to_owned()))?;
    let slot_type = get_slot_type(context, types, key, value)?;
    let buffer = builder
        .build_extract_value(header, 0, "map.drop.buffer")?
        .into_pointer_value();
    let capacity = builder
        .build_extract_value(header, 2, "map.drop.capacity")?
        .into_int_value();
    let preheader = builder
        .get_insert_block()
        .ok_or_else(|| LlvmAdapterError::Build("map drop requires insertion block".to_owned()))?;
    let check = context.append_basic_block(function, "map.drop.check");
    let slot_check = context.append_basic_block(function, "map.drop.slot");
    let occupied = context.append_basic_block(function, "map.drop.occupied");
    let advance = context.append_basic_block(function, "map.drop.advance");
    let done = context.append_basic_block(function, "map.drop.done");
    builder.build_unconditional_branch(check)?;
    builder.position_at_end(check);
    let index = builder.build_phi(runtime.size_type(), "map.drop.index")?;
    index.add_incoming(&[(&runtime.size_type().const_zero(), preheader)]);
    let current = index.as_basic_value().into_int_value();
    let finished =
        builder.build_int_compare(IntPredicate::UGE, current, capacity, "map.drop.finished")?;
    builder.build_conditional_branch(finished, done, slot_check)?;
    builder.position_at_end(slot_check);
    // SAFETY: current < capacity; buffer contains capacity slots of the verified layout.
    let slot = unsafe { builder.build_gep(slot_type, buffer, &[current], "map.drop.slot.ptr")? };
    let state_ptr = builder.build_struct_gep(slot_type, slot, 0, "map.drop.state.ptr")?;
    let state = builder
        .build_load(context.i32_type(), state_ptr, "map.drop.state")?
        .into_int_value();
    let active = builder.build_int_compare(
        IntPredicate::EQ,
        state,
        context.i32_type().const_int(SLOT_STATE_OCCUPIED, false),
        "map.drop.active",
    )?;
    builder.build_conditional_branch(active, occupied, advance)?;
    builder.position_at_end(occupied);
    entry(
        context, builder, types, runtime, module, slot_type, slot, key, value,
    )?;
    builder.build_unconditional_branch(advance)?;
    builder.position_at_end(advance);
    let next = builder.build_int_add(
        current,
        runtime.size_type().const_int(1, false),
        "map.drop.next",
    )?;
    builder.build_unconditional_branch(check)?;
    index.add_incoming(&[(&next, advance)]);
    builder.position_at_end(done);
    let free = runtime
        .free()
        .ok_or_else(|| LlvmAdapterError::Build("map drop requires free".to_owned()))?;
    builder.build_call(free, &[buffer.into()], "")?;
    Ok(())
}
