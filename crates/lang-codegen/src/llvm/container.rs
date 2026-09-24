//! 顺序容器固定 header 与连续缓冲区构造 lowering。

use inkwell::{
    builder::Builder,
    context::Context,
    module::Module as LlvmModule,
    values::{
        BasicMetadataValueEnum, BasicValueEnum, FunctionValue, IntValue, StructValue, ValueKind,
    },
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
    let layout = types.container_layout(container)?;
    let length = runtime.container_int_to_size(builder, function, length, name)?;
    let buffer = runtime.allocate_buffer(llvm, builder, function, length, layout.stride, name)?;
    let preheader = builder
        .get_insert_block()
        .ok_or_else(|| LlvmAdapterError::Build("容器生成缺少 preheader".to_owned()))?;
    let loop_header = context.append_basic_block(function, &format!("{name}.loop"));
    let loop_body = context.append_basic_block(function, &format!("{name}.body"));
    let done = context.append_basic_block(function, &format!("{name}.done"));
    builder.build_unconditional_branch(loop_header)?;

    builder.position_at_end(loop_header);
    let index = builder.build_phi(runtime.size_type(), &format!("{name}.index"))?;
    let zero = runtime.size_type().const_zero();
    index.add_incoming(&[(&zero, preheader)]);
    let current = index.as_basic_value().into_int_value();
    let remains = builder.build_int_compare(
        inkwell::IntPredicate::ULT,
        current,
        length,
        &format!("{name}.remains"),
    )?;
    builder.build_conditional_branch(remains, loop_body, done)?;

    builder.position_at_end(loop_body);
    let initializer_index = match current.get_type().get_bit_width().cmp(&32) {
        std::cmp::Ordering::Less => {
            builder.build_int_z_extend(current, context.i32_type(), &format!("{name}.index.int"))?
        }
        std::cmp::Ordering::Equal => current,
        std::cmp::Ordering::Greater => {
            builder.build_int_truncate(current, context.i32_type(), &format!("{name}.index.int"))?
        }
    };
    let element = match builder
        .build_call(
            initializer,
            &[BasicMetadataValueEnum::from(initializer_index)],
            &format!("{name}.element"),
        )?
        .try_as_basic_value()
    {
        ValueKind::Basic(value) => value,
        ValueKind::Instruction(_) => {
            return Err(LlvmAdapterError::Build(
                "容器 initializer 未返回元素值".to_owned(),
            ));
        }
    };
    if layout.stride != 0 {
        // SAFETY: the loop body is reachable only when index < the checked non-negative length;
        // allocation used the same length and target-derived stride, with physical bytes
        // checked against the pointer-index bound. ZST skips address formation.
        let slot = unsafe {
            builder.build_gep(layout.element, buffer, &[current], &format!("{name}.slot"))?
        };
        builder.build_store(slot, element)?;
    }
    let next = builder.build_int_add(
        current,
        runtime.size_type().const_int(1, false),
        &format!("{name}.next"),
    )?;
    let backedge = builder
        .get_insert_block()
        .ok_or_else(|| LlvmAdapterError::Build("容器生成缺少 backedge block".to_owned()))?;
    builder.build_unconditional_branch(loop_header)?;
    index.add_incoming(&[(&next, backedge)]);

    builder.position_at_end(done);
    build_header(builder, layout.header, buffer, length, name)
}

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
