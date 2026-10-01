//! UTF-8 String owner 的 LLVM 值布局与字节级 primitive lowering。

use inkwell::{
    AddressSpace, IntPredicate,
    builder::Builder,
    context::Context,
    module::{Linkage, Module as LlvmModule},
    values::{FunctionValue, IntValue, PointerValue, StructValue},
};

use crate::ssa::model::SsaTypeId;

use super::{LlvmAdapterError, runtime::RuntimeAbi, type_map::TypeMap};

#[derive(Clone, Copy)]
pub(super) struct StringView<'ctx> {
    bytes: PointerValue<'ctx>,
    length: IntValue<'ctx>,
}

pub(super) fn view<'ctx>(
    builder: &Builder<'ctx>,
    value: StructValue<'ctx>,
    name: &str,
) -> Result<StringView<'ctx>, LlvmAdapterError> {
    Ok(StringView {
        bytes: builder
            .build_extract_value(value, 0, &format!("{name}.bytes"))?
            .into_pointer_value(),
        length: builder
            .build_extract_value(value, 1, &format!("{name}.length"))?
            .into_int_value(),
    })
}

pub(super) fn literal<'ctx>(
    llvm: &LlvmModule<'ctx>,
    builder: &Builder<'ctx>,
    types: &TypeMap<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    string: SsaTypeId,
    bytes: &[u8],
    name: &str,
) -> Result<StructValue<'ctx>, LlvmAdapterError> {
    let string_type = types.basic_type(string)?.into_struct_type();
    let pointer = if bytes.is_empty() {
        llvm.get_context()
            .ptr_type(AddressSpace::default())
            .const_null()
    } else {
        let constant = llvm.get_context().const_string(bytes, false);
        let global = llvm.add_global(constant.get_type(), None, &format!("{name}.bytes"));
        global.set_linkage(Linkage::Private);
        global.set_constant(true);
        global.set_initializer(&constant);
        global.as_pointer_value()
    };
    let length = u64::try_from(bytes.len())
        .map_err(|_| LlvmAdapterError::Build("String literal 长度超出 u64".to_owned()))?;
    build_owner(
        builder,
        string_type,
        pointer,
        runtime.size_type().const_int(length, false),
        runtime.size_type().const_zero(),
        name,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn concat<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    string_type: inkwell::types::StructType<'ctx>,
    left: StringView<'ctx>,
    right: StringView<'ctx>,
    name: &str,
) -> Result<StructValue<'ctx>, LlvmAdapterError> {
    let length = builder.build_int_add(left.length, right.length, &format!("{name}.length"))?;
    let overflow = builder.build_int_compare(
        IntPredicate::ULT,
        length,
        left.length,
        &format!("{name}.overflow"),
    )?;
    runtime.abort_if(builder, function, overflow, name)?;

    let before_empty = builder
        .get_insert_block()
        .ok_or_else(|| LlvmAdapterError::Build("String concat 缺少 preheader".to_owned()))?;
    let empty = context.append_basic_block(function, &format!("{name}.empty"));
    let allocate = context.append_basic_block(function, &format!("{name}.allocate"));
    let ready = context.append_basic_block(function, &format!("{name}.ready"));
    let is_empty = builder.build_int_compare(
        IntPredicate::EQ,
        length,
        runtime.size_type().const_zero(),
        &format!("{name}.is_empty"),
    )?;
    builder.build_conditional_branch(is_empty, empty, allocate)?;

    builder.position_at_end(empty);
    let empty_value = build_owner(
        builder,
        string_type,
        context.ptr_type(AddressSpace::default()).const_null(),
        length,
        runtime.size_type().const_zero(),
        &format!("{name}.empty_value"),
    )?;
    let empty_block = builder.get_insert_block().unwrap_or(before_empty);
    builder.build_unconditional_branch(ready)?;

    builder.position_at_end(allocate);
    let allocation = runtime.allocate_string_bytes(builder, function, length, name)?;
    builder.build_memcpy(allocation, 1, left.bytes, 1, left.length)?;
    // SAFETY: allocation has checked `left.length + right.length` bytes, so offset left.length
    // starts the remaining right.length-byte suffix.
    let suffix = unsafe {
        builder.build_in_bounds_gep(
            context.i8_type(),
            allocation,
            &[left.length],
            &format!("{name}.suffix"),
        )?
    };
    builder.build_memcpy(suffix, 1, right.bytes, 1, right.length)?;
    let allocated_value = build_owner(
        builder,
        string_type,
        allocation,
        length,
        length,
        &format!("{name}.allocated_value"),
    )?;
    let allocated_block = builder
        .get_insert_block()
        .ok_or_else(|| LlvmAdapterError::Build("String concat 缺少 allocated block".to_owned()))?;
    builder.build_unconditional_branch(ready)?;

    builder.position_at_end(ready);
    let result = builder.build_phi(string_type, name)?;
    result.add_incoming(&[
        (&empty_value, empty_block),
        (&allocated_value, allocated_block),
    ]);
    Ok(result.as_basic_value().into_struct_value())
}

/// Clone always copies non-empty bytes, even when the source uses static storage.
pub(super) fn clone_owner<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    string_type: inkwell::types::StructType<'ctx>,
    source: StringView<'ctx>,
    name: &str,
) -> Result<StructValue<'ctx>, LlvmAdapterError> {
    let empty = context.append_basic_block(function, &format!("{name}.clone.empty"));
    let allocate = context.append_basic_block(function, &format!("{name}.clone.allocate"));
    let ready = context.append_basic_block(function, &format!("{name}.clone.ready"));
    let is_empty = builder.build_int_compare(
        IntPredicate::EQ,
        source.length,
        runtime.size_type().const_zero(),
        &format!("{name}.clone.is_empty"),
    )?;
    builder.build_conditional_branch(is_empty, empty, allocate)?;

    builder.position_at_end(empty);
    let empty_value = build_owner(
        builder,
        string_type,
        context.ptr_type(AddressSpace::default()).const_null(),
        runtime.size_type().const_zero(),
        runtime.size_type().const_zero(),
        &format!("{name}.clone.empty_value"),
    )?;
    builder.build_unconditional_branch(ready)?;

    builder.position_at_end(allocate);
    let allocation = runtime.allocate_string_bytes(builder, function, source.length, name)?;
    builder.build_memcpy(allocation, 1, source.bytes, 1, source.length)?;
    let allocated_value = build_owner(
        builder,
        string_type,
        allocation,
        source.length,
        source.length,
        &format!("{name}.clone.allocated_value"),
    )?;
    let allocated_block = builder
        .get_insert_block()
        .ok_or_else(|| LlvmAdapterError::Build("String clone 缺少 allocated block".to_owned()))?;
    builder.build_unconditional_branch(ready)?;

    builder.position_at_end(ready);
    let result = builder.build_phi(string_type, name)?;
    result.add_incoming(&[(&empty_value, empty), (&allocated_value, allocated_block)]);
    Ok(result.as_basic_value().into_struct_value())
}

pub(super) fn equal<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    left: StringView<'ctx>,
    right: StringView<'ctx>,
    name: &str,
) -> Result<IntValue<'ctx>, LlvmAdapterError> {
    let preheader = builder
        .get_insert_block()
        .ok_or_else(|| LlvmAdapterError::Build("String equality 缺少 preheader".to_owned()))?;
    let compare = context.append_basic_block(function, &format!("{name}.compare"));
    let body = context.append_basic_block(function, &format!("{name}.body"));
    let advance = context.append_basic_block(function, &format!("{name}.advance"));
    let equal = context.append_basic_block(function, &format!("{name}.equal"));
    let mismatch = context.append_basic_block(function, &format!("{name}.mismatch"));
    let done = context.append_basic_block(function, &format!("{name}.done"));
    let same_length = builder.build_int_compare(
        IntPredicate::EQ,
        left.length,
        right.length,
        &format!("{name}.same_length"),
    )?;
    builder.build_conditional_branch(same_length, compare, done)?;

    builder.position_at_end(compare);
    let index = builder.build_phi(runtime.size_type(), &format!("{name}.index"))?;
    index.add_incoming(&[(&runtime.size_type().const_zero(), preheader)]);
    let current = index.as_basic_value().into_int_value();
    let remains = builder.build_int_compare(
        IntPredicate::ULT,
        current,
        left.length,
        &format!("{name}.remains"),
    )?;
    builder.build_conditional_branch(remains, body, equal)?;

    builder.position_at_end(body);
    // SAFETY: the body is dominated by index < the equal left/right lengths.
    let left_byte = unsafe {
        builder.build_in_bounds_gep(context.i8_type(), left.bytes, &[current], "left.byte")?
    };
    // SAFETY: the same checked index is in bounds for the equal right length.
    let right_byte = unsafe {
        builder.build_in_bounds_gep(context.i8_type(), right.bytes, &[current], "right.byte")?
    };
    let left_byte = builder
        .build_load(context.i8_type(), left_byte, "left.value")?
        .into_int_value();
    let right_byte = builder
        .build_load(context.i8_type(), right_byte, "right.value")?
        .into_int_value();
    let same_byte = builder.build_int_compare(
        IntPredicate::EQ,
        left_byte,
        right_byte,
        &format!("{name}.same_byte"),
    )?;
    builder.build_conditional_branch(same_byte, advance, mismatch)?;

    builder.position_at_end(advance);
    let next = builder.build_int_add(
        current,
        runtime.size_type().const_int(1, false),
        &format!("{name}.next"),
    )?;
    let backedge = builder
        .get_insert_block()
        .ok_or_else(|| LlvmAdapterError::Build("String equality 缺少 backedge".to_owned()))?;
    builder.build_unconditional_branch(compare)?;
    index.add_incoming(&[(&next, backedge)]);

    builder.position_at_end(equal);
    builder.build_unconditional_branch(done)?;
    builder.position_at_end(mismatch);
    builder.build_unconditional_branch(done)?;
    builder.position_at_end(done);
    let result = builder.build_phi(context.bool_type(), name)?;
    let false_value = context.bool_type().const_zero();
    let true_value = context.bool_type().const_int(1, false);
    result.add_incoming(&[
        (&false_value, preheader),
        (&true_value, equal),
        (&false_value, mismatch),
    ]);
    Ok(result.as_basic_value().into_int_value())
}

pub(super) fn print<'ctx>(
    llvm: &LlvmModule<'ctx>,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    value: StringView<'ctx>,
    name: &str,
) -> Result<(), LlvmAdapterError> {
    runtime.emit_write_bytes(builder, function, value.bytes, value.length, name)?;
    runtime.emit_print_literal(llvm, builder, function, b"\n", &format!("{name}.newline"))
}

pub(super) fn build_owner<'ctx>(
    builder: &Builder<'ctx>,
    ty: inkwell::types::StructType<'ctx>,
    bytes: PointerValue<'ctx>,
    length: IntValue<'ctx>,
    capacity: IntValue<'ctx>,
    name: &str,
) -> Result<StructValue<'ctx>, LlvmAdapterError> {
    let value = builder
        .build_insert_value(ty.const_zero(), bytes, 0, &format!("{name}.with_bytes"))?
        .into_struct_value();
    let value = builder
        .build_insert_value(value, length, 1, &format!("{name}.with_length"))?
        .into_struct_value();
    Ok(builder
        .build_insert_value(value, capacity, 2, &format!("{name}.with_capacity"))?
        .into_struct_value())
}
