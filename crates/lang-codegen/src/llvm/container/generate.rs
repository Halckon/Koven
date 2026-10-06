//! Checked allocation and one shared static/borrowed generation loop.

use super::{LlvmAdapterError, RuntimeAbi, TypeMap, build_header};
use crate::llvm::closure;
use crate::ssa::model::{Module, SsaTypeId};
use inkwell::{
    builder::Builder,
    context::Context,
    module::Module as LlvmModule,
    values::{
        BasicMetadataValueEnum, BasicValueEnum, FunctionValue, IntValue, PointerValue, StructValue,
        ValueKind,
    },
};

pub(in crate::llvm) enum Initializer<'ctx, 'ssa> {
    Static(FunctionValue<'ctx>),
    Borrowed {
        module: &'ssa Module,
        ty: SsaTypeId,
        callable: BasicValueEnum<'ctx>,
    },
}

enum Prepared<'ctx> {
    Static(FunctionValue<'ctx>),
    Borrowed {
        callable: closure::PreparedCallable<'ctx>,
        index: PointerValue<'ctx>,
    },
}

#[allow(clippy::too_many_arguments)]
pub(in crate::llvm) fn lower<'ctx>(
    context: &'ctx Context,
    llvm: &LlvmModule<'ctx>,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    initializer: Initializer<'ctx, '_>,
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
    let initializer = match initializer {
        Initializer::Static(function) => Prepared::Static(function),
        Initializer::Borrowed {
            module,
            ty,
            callable,
        } => {
            let callable = closure::prepare(builder, module, types, ty, callable, name)?;
            let index =
                builder.build_alloca(context.i32_type(), &format!("{name}.index.storage"))?;
            Prepared::Borrowed { callable, index }
        }
    };
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
    let element = match &initializer {
        Prepared::Static(initializer) => match builder
            .build_call(
                *initializer,
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
        },
        Prepared::Borrowed { callable, index } => {
            builder.build_store(*index, initializer_index)?;
            // The operation verifier permits a void result only for logical Unit storage.
            closure::invoke_prepared(
                builder,
                callable,
                &[(*index).into()],
                &format!("{name}.element"),
            )?
            .unwrap_or_else(|| layout.element.const_zero())
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
