//! Ordered Map cursors retain all target size bits; every slot access is checked.
use super::*;

pub(in crate::llvm) enum Component {
    Key,
    Value,
}

/// Read-only lowering context for one actual Map header and its checked slots.
pub(in crate::llvm) struct SlotSource<'ctx, 'a> {
    pub(in crate::llvm) context: &'ctx Context,
    pub(in crate::llvm) builder: &'a Builder<'ctx>,
    pub(in crate::llvm) function: FunctionValue<'ctx>,
    pub(in crate::llvm) types: &'a TypeMap<'ctx>,
    pub(in crate::llvm) runtime: &'a RuntimeAbi<'ctx>,
    pub(in crate::llvm) module: &'a Module,
    pub(in crate::llvm) map_type: SsaTypeId,
    pub(in crate::llvm) header: StructValue<'ctx>,
}

fn check_size(runtime: &RuntimeAbi<'_>) -> Result<(), LlvmAdapterError> {
    if runtime.size_type().get_bit_width() > 64 {
        return Err(LlvmAdapterError::Unsupported(
            "Map iteration requires a target size width no greater than 64 bits".into(),
        ));
    }
    Ok(())
}

fn widen<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    value: IntValue<'ctx>,
    name: &str,
) -> Result<IntValue<'ctx>, LlvmAdapterError> {
    match value.get_type().get_bit_width() {
        64 => Ok(value),
        1..=63 => Ok(builder.build_int_z_extend(value, context.i64_type(), name)?),
        _ => Err(LlvmAdapterError::Unsupported(
            "Map iteration cannot represent target size without loss".into(),
        )),
    }
}

pub(in crate::llvm) fn first<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    header: StructValue<'ctx>,
    name: &str,
) -> Result<IntValue<'ctx>, LlvmAdapterError> {
    check_size(runtime)?;
    let first = builder
        .build_extract_value(header, 4, &format!("{name}.first"))?
        .into_int_value();
    widen(context, builder, first, name)
}

pub(in crate::llvm) fn has_next<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    cursor: IntValue<'ctx>,
    name: &str,
) -> Result<IntValue<'ctx>, LlvmAdapterError> {
    check_size(runtime)?;
    let none = widen(
        context,
        builder,
        runtime.size_type().const_all_ones(),
        "map.iteration.none",
    )?;
    Ok(builder.build_int_compare(IntPredicate::NE, cursor, none, name)?)
}

fn checked_slot<'ctx>(
    source: &SlotSource<'ctx, '_>,
    cursor: IntValue<'ctx>,
    name: &str,
) -> Result<(StructType<'ctx>, PointerValue<'ctx>), LlvmAdapterError> {
    let SlotSource {
        context,
        builder,
        function,
        types,
        runtime,
        module,
        map_type,
        header,
    } = *source;
    check_size(runtime)?;
    let (_, key, value) = module.map_container(map_type).ok_or_else(|| {
        LlvmAdapterError::InvalidSsa("Map iteration source has no Map storage".into())
    })?;
    let slot_type = get_slot_type(context, types, runtime.size_type(), key, value)?;
    let capacity = builder
        .build_extract_value(header, 2, &format!("{name}.capacity"))?
        .into_int_value();
    let capacity = widen(context, builder, capacity, &format!("{name}.capacity.u64"))?;
    let bounded = builder.build_int_compare(
        IntPredicate::ULT,
        cursor,
        capacity,
        &format!("{name}.bounded"),
    )?;
    let present = has_next(
        context,
        builder,
        runtime,
        cursor,
        &format!("{name}.present"),
    )?;
    let valid = builder.build_and(bounded, present, &format!("{name}.index.valid"))?;
    let invalid = builder.build_not(valid, &format!("{name}.index.invalid"))?;
    runtime.abort_if(
        builder,
        function,
        invalid,
        &format!("{name}.index"),
        crate::llvm::runtime::AbortMessage::Text("internal error: invalid map iteration cursor"),
    )?;

    // The u64 comparison above proves this conversion lossless on narrow targets.
    let index = if runtime.size_type().get_bit_width() < 64 {
        builder.build_int_truncate(cursor, runtime.size_type(), &format!("{name}.index"))?
    } else {
        cursor
    };
    let buckets = builder
        .build_extract_value(header, 0, &format!("{name}.buckets"))?
        .into_pointer_value();
    // SAFETY: only the successful non-NONE, cursor < capacity edge reaches this
    // non-inbounds GEP; the verified Map header owns capacity initialized slots.
    let slot = unsafe { builder.build_gep(slot_type, buckets, &[index], &format!("{name}.slot"))? };
    let state_pointer =
        builder.build_struct_gep(slot_type, slot, 0, &format!("{name}.state.ptr"))?;
    let state = builder
        .build_load(context.i32_type(), state_pointer, &format!("{name}.state"))?
        .into_int_value();
    let dead = builder.build_int_compare(
        IntPredicate::NE,
        state,
        context.i32_type().const_int(SLOT_STATE_OCCUPIED, false),
        &format!("{name}.dead"),
    )?;
    runtime.abort_if(
        builder,
        function,
        dead,
        &format!("{name}.occupied"),
        crate::llvm::runtime::AbortMessage::Text("internal error: invalid map iteration cursor"),
    )?;
    Ok((slot_type, slot))
}

pub(in crate::llvm) fn next<'ctx>(
    source: &SlotSource<'ctx, '_>,
    cursor: IntValue<'ctx>,
    name: &str,
) -> Result<IntValue<'ctx>, LlvmAdapterError> {
    let (slot_type, slot) = checked_slot(source, cursor, name)?;
    let SlotSource {
        context,
        builder,
        runtime,
        ..
    } = *source;
    let pointer = builder.build_struct_gep(slot_type, slot, 4, &format!("{name}.next.ptr"))?;
    let next = builder
        .build_load(runtime.size_type(), pointer, &format!("{name}.next"))?
        .into_int_value();
    widen(context, builder, next, name)
}

pub(in crate::llvm) fn component<'ctx>(
    source: &SlotSource<'ctx, '_>,
    cursor: IntValue<'ctx>,
    component: Component,
    name: &str,
) -> Result<PointerValue<'ctx>, LlvmAdapterError> {
    let (slot_type, slot) = checked_slot(source, cursor, name)?;
    let field = match component {
        Component::Key => 1,
        Component::Value => 2,
    };
    Ok(source
        .builder
        .build_struct_gep(slot_type, slot, field, name)?)
}
