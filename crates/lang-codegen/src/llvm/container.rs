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
    let length = u64::try_from(elements.len())
        .map_err(|_| LlvmAdapterError::Build("容器元素数量不能由目标 size_t 表示".to_owned()))?;
    let length = runtime.size_type().const_int(length, false);
    let buffer = runtime.allocate_buffer(llvm, builder, function, length, layout.stride, name)?;
    if layout.stride != 0 {
        for (index, element) in elements.iter().enumerate() {
            let index = runtime.size_type().const_int(index as u64, false);
            // SAFETY: allocation size was checked as length * stride, and every source-ordered
            // index is strictly below that same length. ZST never enters this physical store path.
            let slot = unsafe {
                builder.build_in_bounds_gep(
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
    let element = match builder
        .build_call(
            initializer,
            &[BasicMetadataValueEnum::from(current)],
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
        // allocation used the same length and target-derived stride. ZST skips address formation.
        let slot = unsafe {
            builder.build_in_bounds_gep(
                layout.element,
                buffer,
                &[current],
                &format!("{name}.slot"),
            )?
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
    name: &str,
) -> Result<IntValue<'ctx>, LlvmAdapterError> {
    Ok(builder
        .build_extract_value(owner, 1, name)?
        .into_int_value())
}

fn build_header<'ctx>(
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
