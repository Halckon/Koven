//! 顺序容器固定 header 与连续缓冲区构造 lowering。

mod generate;

use inkwell::{
    builder::Builder,
    context::Context,
    module::Module as LlvmModule,
    values::{BasicValueEnum, FunctionValue, IntValue, StructValue},
};

use crate::ssa::model::SsaTypeId;
use crate::ssa::model::{Module, Ownership};

use super::{LlvmAdapterError, runtime::RuntimeAbi, type_map::TypeMap};

#[allow(clippy::too_many_arguments)]
pub(super) fn construct<'ctx>(
    llvm: &LlvmModule<'ctx>,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    types: &TypeMap<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    container: SsaTypeId,
    elements: &[BasicValueEnum<'ctx>],
    name: &str,
) -> Result<StructValue<'ctx>, LlvmAdapterError> {
    let layout = types.container_layout(container)?;
    let length = checked_list_length(elements.len(), runtime.size_type().get_bit_width())?;
    let length = runtime.size_type().const_int(length, false);
    let buffer = runtime.allocate_buffer(llvm, builder, function, length, layout.stride, name)?;
    if layout.stride != 0 {
        for (index, element) in elements.iter().enumerate() {
            let index = runtime.size_type().const_int(index as u64, false);
            // SAFETY: allocation size was checked as length * stride, and every source-ordered
            // index is strictly below that same length. Physical bytes already satisfy the
            // pointer-index bound. ZST skips this physical store path.
            let slot = unsafe {
                builder.build_gep(
                    layout.element,
                    buffer,
                    &[index],
                    &format!(
                        "{name}.slot{}",
                        index.get_zero_extended_constant().unwrap_or(0)
                    ),
                )?
            };
            builder.build_store(slot, *element)?;
        }
    }
    build_header(builder, layout.header, buffer, length, name)
}

pub(super) fn checked_list_length(count: usize, size_bits: u32) -> Result<u64, LlvmAdapterError> {
    let length = u64::try_from(count)
        .map_err(|_| LlvmAdapterError::Build("容器元素数量不能由目标 size_t 表示".to_owned()))?;
    if length > max_logical_length(size_bits) {
        return Err(LlvmAdapterError::Build(
            "容器元素数量不能由 Koven Int 或目标 size_t 表示".to_owned(),
        ));
    }
    Ok(length)
}

/// 同一 logical length 上限供静态构造及运行时 Int→size_t 边界使用。
pub(super) const fn max_logical_length(size_bits: u32) -> u64 {
    if size_bits >= 31 {
        i32::MAX as u64
    } else {
        (1_u64 << size_bits) - 1
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn generate<'ctx>(
    context: &'ctx Context,
    llvm: &LlvmModule<'ctx>,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    initializer: FunctionValue<'ctx>,
    types: &TypeMap<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    container: SsaTypeId,
    length: IntValue<'ctx>,
    name: &str,
) -> Result<StructValue<'ctx>, LlvmAdapterError> {
    generate::lower(
        context,
        llvm,
        builder,
        function,
        generate::Initializer::Static(initializer),
        types,
        runtime,
        container,
        length,
        name,
    )
}

pub(super) use generate::{Initializer, lower as generate_borrowed};

pub(super) fn length<'ctx>(
    builder: &Builder<'ctx>,
    owner: StructValue<'ctx>,
    int_type: inkwell::types::IntType<'ctx>,
    name: &str,
) -> Result<IntValue<'ctx>, LlvmAdapterError> {
    let header = builder
        .build_extract_value(owner, 1, &format!("{name}.header"))?
        .into_int_value();
    match header
        .get_type()
        .get_bit_width()
        .cmp(&int_type.get_bit_width())
    {
        std::cmp::Ordering::Less => Ok(builder.build_int_z_extend(header, int_type, name)?),
        std::cmp::Ordering::Equal => Ok(header),
        std::cmp::Ordering::Greater => Ok(builder.build_int_truncate(header, int_type, name)?),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn element_place<'ctx>(
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    types: &TypeMap<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    container: SsaTypeId,
    owner: StructValue<'ctx>,
    index: IntValue<'ctx>,
    name: &str,
) -> Result<inkwell::values::PointerValue<'ctx>, LlvmAdapterError> {
    let layout = types.container_layout(container)?;
    let buffer = builder
        .build_extract_value(owner, 0, &format!("{name}.buffer"))?
        .into_pointer_value();
    let logical_length = length(
        builder,
        owner,
        index.get_type(),
        &format!("{name}.logical_length"),
    )?;
    let negative = builder.build_int_compare(
        inkwell::IntPredicate::SLT,
        index,
        index.get_type().const_zero(),
        &format!("{name}.negative"),
    )?;
    let beyond = builder.build_int_compare(
        inkwell::IntPredicate::SGE,
        index,
        logical_length,
        &format!("{name}.beyond"),
    )?;
    let invalid = builder.build_or(negative, beyond, &format!("{name}.invalid"))?;
    runtime.abort_if(builder, function, invalid, name)?;
    let size_type = runtime.size_type();
    let index = match index
        .get_type()
        .get_bit_width()
        .cmp(&size_type.get_bit_width())
    {
        std::cmp::Ordering::Less => {
            builder.build_int_z_extend(index, size_type, &format!("{name}.index.size"))?
        }
        std::cmp::Ordering::Equal => index,
        std::cmp::Ordering::Greater => {
            builder.build_int_truncate(index, size_type, &format!("{name}.index.size"))?
        }
    };
    if layout.stride == 0 {
        return Ok(buffer);
    }
    // SAFETY: signed Int bounds checks dominate conversion and this GEP. The header was built
    // with the same logical length and target-derived element stride. Buffer allocation
    // already rejected physical sizes beyond the signed pointer-index range.
    Ok(unsafe { builder.build_gep(layout.element, buffer, &[index], &format!("{name}.slot"))? })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn replace<'ctx>(
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    module: &Module,
    types: &TypeMap<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    container: SsaTypeId,
    owner: StructValue<'ctx>,
    index: IntValue<'ctx>,
    value: BasicValueEnum<'ctx>,
    name: &str,
) -> Result<(), LlvmAdapterError> {
    let layout = types.container_layout(container)?;
    let (_, element) = module
        .sequential_container(container)
        .ok_or_else(|| LlvmAdapterError::InvalidSsa("replace owner 不是顺序容器".to_owned()))?;
    let slot = element_place(
        builder, function, types, runtime, container, owner, index, name,
    )?;
    let old = if layout.stride == 0 {
        layout.element.const_zero()
    } else {
        let old = builder.build_load(layout.element, slot, &format!("{name}.old"))?;
        builder.build_store(slot, value)?;
        old
    };
    if module.type_ownership(element) == Some(Ownership::MoveOnly) {
        runtime.emit_drop(builder, element, old)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn append<'ctx>(
    llvm: &LlvmModule<'ctx>,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    types: &TypeMap<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    container: SsaTypeId,
    owner: StructValue<'ctx>,
    element: BasicValueEnum<'ctx>,
    name: &str,
) -> Result<StructValue<'ctx>, LlvmAdapterError> {
    let layout = types.container_layout(container)?;
    let buffer = builder
        .build_extract_value(owner, 0, &format!("{name}.old_buffer"))?
        .into_pointer_value();
    let old_length = builder
        .build_extract_value(owner, 1, &format!("{name}.old_length"))?
        .into_int_value();
    let old_capacity = builder
        .build_extract_value(owner, 2, &format!("{name}.old_capacity"))?
        .into_int_value();

    let size_type = runtime.size_type();
    let need_grow = builder.build_int_compare(
        inkwell::IntPredicate::EQ,
        old_length,
        old_capacity,
        &format!("{name}.need_grow"),
    )?;

    let context = llvm.get_context();
    let grow_block = context.append_basic_block(function, &format!("{name}.grow"));
    let no_grow_block = context.append_basic_block(function, &format!("{name}.no_grow"));
    let write_block = context.append_basic_block(function, &format!("{name}.write"));

    builder.build_conditional_branch(need_grow, grow_block, no_grow_block)?;

    builder.position_at_end(grow_block);
    let zero = size_type.const_zero();
    let is_zero_cap = builder.build_int_compare(
        inkwell::IntPredicate::EQ,
        old_capacity,
        zero,
        &format!("{name}.is_zero_cap"),
    )?;
    let four = size_type.const_int(4, false);
    let double_cap = builder.build_int_mul(
        old_capacity,
        size_type.const_int(2, false),
        &format!("{name}.double_cap"),
    )?;
    let new_capacity = builder
        .build_select(
            is_zero_cap,
            four,
            double_cap,
            &format!("{name}.new_capacity"),
        )?
        .into_int_value();

    let new_buffer = runtime.allocate_buffer(
        llvm,
        builder,
        function,
        new_capacity,
        layout.stride,
        &format!("{name}.new_buffer"),
    )?;

    if layout.stride != 0 {
        let has_elements = builder.build_int_compare(
            inkwell::IntPredicate::UGT,
            old_length,
            zero,
            &format!("{name}.has_elements"),
        )?;
        let copy_block = context.append_basic_block(function, &format!("{name}.copy"));
        let free_block = context.append_basic_block(function, &format!("{name}.free_check"));
        builder.build_conditional_branch(has_elements, copy_block, free_block)?;

        builder.position_at_end(copy_block);
        let copy_bytes = builder.build_int_mul(
            old_length,
            size_type.const_int(layout.stride, false),
            &format!("{name}.copy_bytes"),
        )?;
        builder.build_memcpy(new_buffer, 1, buffer, 1, copy_bytes)?;
        builder.build_unconditional_branch(free_block)?;

        builder.position_at_end(free_block);
        let has_old_alloc = builder.build_int_compare(
            inkwell::IntPredicate::UGT,
            old_capacity,
            zero,
            &format!("{name}.has_old_alloc"),
        )?;
        let do_free_block = context.append_basic_block(function, &format!("{name}.do_free"));
        let after_grow_block = context.append_basic_block(function, &format!("{name}.after_grow"));
        builder.build_conditional_branch(has_old_alloc, do_free_block, after_grow_block)?;

        builder.position_at_end(do_free_block);
        if let Some(free_fn) = runtime.free() {
            builder.build_call(
                free_fn,
                &[inkwell::values::BasicMetadataValueEnum::from(buffer)],
                "",
            )?;
        }
        builder.build_unconditional_branch(after_grow_block)?;

        builder.position_at_end(after_grow_block);
        builder.build_unconditional_branch(write_block)?;
    } else {
        builder.build_unconditional_branch(write_block)?;
    }
    let grow_final_block = builder.get_insert_block().unwrap();

    builder.position_at_end(no_grow_block);
    builder.build_unconditional_branch(write_block)?;

    builder.position_at_end(write_block);
    let active_buffer_phi = builder.build_phi(
        context.ptr_type(inkwell::AddressSpace::default()),
        &format!("{name}.active_buffer"),
    )?;
    active_buffer_phi.add_incoming(&[(&new_buffer, grow_final_block), (&buffer, no_grow_block)]);
    let active_buffer = active_buffer_phi.as_basic_value().into_pointer_value();

    let active_capacity_phi = builder.build_phi(size_type, &format!("{name}.active_capacity"))?;
    active_capacity_phi.add_incoming(&[
        (&new_capacity, grow_final_block),
        (&old_capacity, no_grow_block),
    ]);
    let active_capacity = active_capacity_phi.as_basic_value().into_int_value();

    if layout.stride != 0 {
        let slot = unsafe {
            builder.build_gep(
                layout.element,
                active_buffer,
                &[old_length],
                &format!("{name}.append_slot"),
            )?
        };
        builder.build_store(slot, element)?;
    }

    let one = size_type.const_int(1, false);
    let new_length = builder.build_int_add(old_length, one, &format!("{name}.new_length"))?;

    let header_type = layout.header;
    let with_buffer = builder
        .build_insert_value(
            header_type.const_zero(),
            active_buffer,
            0,
            &format!("{name}.with_buffer"),
        )?
        .into_struct_value();
    let with_length = builder
        .build_insert_value(with_buffer, new_length, 1, &format!("{name}.with_length"))?
        .into_struct_value();
    let with_capacity = builder
        .build_insert_value(
            with_length,
            active_capacity,
            2,
            &format!("{name}.with_capacity"),
        )?
        .into_struct_value();

    Ok(with_capacity)
}

pub(super) fn build_header<'ctx>(
    builder: &Builder<'ctx>,
    header: inkwell::types::StructType<'ctx>,
    buffer: inkwell::values::PointerValue<'ctx>,
    length: IntValue<'ctx>,
    name: &str,
) -> Result<StructValue<'ctx>, LlvmAdapterError> {
    let with_buffer = builder
        .build_insert_value(
            header.const_zero(),
            buffer,
            0,
            &format!("{name}.with_buffer"),
        )?
        .into_struct_value();
    let with_length = builder
        .build_insert_value(with_buffer, length, 1, &format!("{name}.with_length"))?
        .into_struct_value();
    if header.count_fields() == 3 {
        Ok(builder
            .build_insert_value(with_length, length, 2, &format!("{name}.with_capacity"))?
            .into_struct_value())
    } else {
        Ok(with_length)
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn clear<'ctx>(
    llvm: &LlvmModule<'ctx>,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    module: &Module,
    types: &TypeMap<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    container: SsaTypeId,
    owner: StructValue<'ctx>,
    name: &str,
) -> Result<StructValue<'ctx>, LlvmAdapterError> {
    let layout = types.container_layout(container)?;
    let (_, element) = module
        .sequential_container(container)
        .ok_or_else(|| LlvmAdapterError::InvalidSsa("clear owner 不是顺序容器".to_owned()))?;
    let buffer = builder
        .build_extract_value(owner, 0, &format!("{name}.buffer"))?
        .into_pointer_value();
    let length = builder
        .build_extract_value(owner, 1, &format!("{name}.length"))?
        .into_int_value();
    let capacity = builder
        .build_extract_value(owner, 2, &format!("{name}.capacity"))?
        .into_int_value();

    if module.type_ownership(element) == Some(Ownership::MoveOnly) {
        let size_type = runtime.size_type();
        let preheader = builder
            .get_insert_block()
            .ok_or_else(|| LlvmAdapterError::Build("container clear 缺少 preheader".to_owned()))?;
        let context = llvm.get_context();
        let loop_header = context.append_basic_block(function, &format!("{name}.drop.loop"));
        let loop_body = context.append_basic_block(function, &format!("{name}.drop.body"));
        let released = context.append_basic_block(function, &format!("{name}.drop.released"));
        builder.build_unconditional_branch(loop_header)?;

        builder.position_at_end(loop_header);
        let remaining_phi = builder.build_phi(size_type, &format!("{name}.remaining"))?;
        remaining_phi.add_incoming(&[(&length, preheader)]);
        let remaining = remaining_phi.as_basic_value().into_int_value();
        let is_empty = builder.build_int_compare(
            inkwell::IntPredicate::EQ,
            remaining,
            size_type.const_zero(),
            &format!("{name}.drop.empty"),
        )?;
        builder.build_conditional_branch(is_empty, released, loop_body)?;

        builder.position_at_end(loop_body);
        let one = size_type.const_int(1, false);
        let index = builder.build_int_sub(remaining, one, &format!("{name}.drop.index"))?;
        let value = if layout.stride == 0 {
            layout.element.const_zero()
        } else {
            let slot = unsafe {
                builder.build_gep(
                    layout.element,
                    buffer,
                    &[index],
                    &format!("{name}.drop.slot"),
                )?
            };
            builder.build_load(layout.element, slot, &format!("{name}.drop.element"))?
        };
        runtime.emit_drop(builder, element, value)?;
        let backedge = builder
            .get_insert_block()
            .ok_or_else(|| LlvmAdapterError::Build("container clear 缺少 backedge".to_owned()))?;
        builder.build_unconditional_branch(loop_header)?;
        remaining_phi.add_incoming(&[(&index, backedge)]);
        builder.position_at_end(released);
    }

    let size_type = runtime.size_type();
    let zero = size_type.const_zero();
    let header_type = layout.header;
    let with_buffer = builder
        .build_insert_value(
            header_type.const_zero(),
            buffer,
            0,
            &format!("{name}.with_buffer"),
        )?
        .into_struct_value();
    let with_length = builder
        .build_insert_value(with_buffer, zero, 1, &format!("{name}.with_length"))?
        .into_struct_value();
    let with_capacity = builder
        .build_insert_value(with_length, capacity, 2, &format!("{name}.with_capacity"))?
        .into_struct_value();

    Ok(with_capacity)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn remove_at<'ctx>(
    llvm: &LlvmModule<'ctx>,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    module: &Module,
    types: &TypeMap<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    container: SsaTypeId,
    owner: StructValue<'ctx>,
    index: IntValue<'ctx>,
    name: &str,
) -> Result<(BasicValueEnum<'ctx>, StructValue<'ctx>), LlvmAdapterError> {
    let layout = types.container_layout(container)?;
    let (_, _element) = module
        .sequential_container(container)
        .ok_or_else(|| LlvmAdapterError::InvalidSsa("removeAt owner 不是顺序容器".to_owned()))?;
    let buffer = builder
        .build_extract_value(owner, 0, &format!("{name}.buffer"))?
        .into_pointer_value();
    let length = builder
        .build_extract_value(owner, 1, &format!("{name}.length"))?
        .into_int_value();
    let capacity = builder
        .build_extract_value(owner, 2, &format!("{name}.capacity"))?
        .into_int_value();

    let logical_length = self::length(
        builder,
        owner,
        index.get_type(),
        &format!("{name}.logical_length"),
    )?;
    let negative = builder.build_int_compare(
        inkwell::IntPredicate::SLT,
        index,
        index.get_type().const_zero(),
        &format!("{name}.negative"),
    )?;
    let beyond = builder.build_int_compare(
        inkwell::IntPredicate::SGE,
        index,
        logical_length,
        &format!("{name}.beyond"),
    )?;
    let invalid = builder.build_or(negative, beyond, &format!("{name}.invalid"))?;
    runtime.abort_if(builder, function, invalid, name)?;

    let size_type = runtime.size_type();
    let size_index = match index
        .get_type()
        .get_bit_width()
        .cmp(&size_type.get_bit_width())
    {
        std::cmp::Ordering::Less => {
            builder.build_int_z_extend(index, size_type, &format!("{name}.index.size"))?
        }
        std::cmp::Ordering::Equal => index,
        std::cmp::Ordering::Greater => {
            builder.build_int_truncate(index, size_type, &format!("{name}.index.size"))?
        }
    };

    let removed_element: BasicValueEnum<'ctx> = if layout.stride == 0 {
        layout.element.const_zero()
    } else {
        let slot = unsafe {
            builder.build_gep(
                layout.element,
                buffer,
                &[size_index],
                &format!("{name}.remove_slot"),
            )?
        };
        builder.build_load(layout.element, slot, &format!("{name}.removed_element"))?
    };

    let one = size_type.const_int(1, false);
    if layout.stride > 0 {
        let next_index = builder.build_int_add(size_index, one, &format!("{name}.next_index"))?;
        let has_elements_to_shift = builder.build_int_compare(
            inkwell::IntPredicate::ULT,
            next_index,
            length,
            &format!("{name}.has_shift"),
        )?;
        let context = llvm.get_context();
        let shift_block = context.append_basic_block(function, &format!("{name}.shift"));
        let after_shift_block =
            context.append_basic_block(function, &format!("{name}.after_shift"));
        builder.build_conditional_branch(has_elements_to_shift, shift_block, after_shift_block)?;

        builder.position_at_end(shift_block);
        let shift_count =
            builder.build_int_sub(length, next_index, &format!("{name}.shift_count"))?;
        let shift_bytes = builder.build_int_mul(
            shift_count,
            size_type.const_int(layout.stride, false),
            &format!("{name}.shift_bytes"),
        )?;
        let src_ptr = unsafe {
            builder.build_gep(
                layout.element,
                buffer,
                &[next_index],
                &format!("{name}.src_slot"),
            )?
        };
        let dst_ptr = unsafe {
            builder.build_gep(
                layout.element,
                buffer,
                &[size_index],
                &format!("{name}.dst_slot"),
            )?
        };
        builder.build_memmove(dst_ptr, 1, src_ptr, 1, shift_bytes)?;
        builder.build_unconditional_branch(after_shift_block)?;

        builder.position_at_end(after_shift_block);
    }

    let new_length = builder.build_int_sub(length, one, &format!("{name}.new_length"))?;
    let header_type = layout.header;
    let with_buffer = builder
        .build_insert_value(
            header_type.const_zero(),
            buffer,
            0,
            &format!("{name}.with_buffer"),
        )?
        .into_struct_value();
    let with_length = builder
        .build_insert_value(with_buffer, new_length, 1, &format!("{name}.with_length"))?
        .into_struct_value();
    let with_capacity = builder
        .build_insert_value(with_length, capacity, 2, &format!("{name}.with_capacity"))?
        .into_struct_value();

    Ok((removed_element, with_capacity))
}
