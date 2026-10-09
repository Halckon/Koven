//! Map 容器的开地址哈希表 LLVM IR 代码生成。

use inkwell::{
    IntPredicate,
    builder::Builder,
    context::Context,
    module::Module as LlvmModule,
    targets::TargetData,
    types::StructType,
    values::{
        BasicMetadataValueEnum, BasicValueEnum, FunctionValue, IntValue, PointerValue, StructValue,
    },
};

use crate::ssa::model::{Module, SsaTypeId, SsaTypeKind};

use super::{LlvmAdapterError, runtime::RuntimeAbi, type_map::TypeMap};

mod drop;
mod hash;
mod lookup;
mod mutation;
mod rehash;
mod require;
mod with_value;

use hash::{build_key_equal, build_key_hash};
pub(super) use lookup::{contains, get};
pub(super) use mutation::{put, remove};
use rehash::rehash_into;
pub(super) use require::require_value;

#[allow(clippy::too_many_arguments)]
pub(super) fn drop_owner<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    types: &TypeMap<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    module: &Module,
    map_type: SsaTypeId,
    header: StructValue<'ctx>,
) -> Result<(), LlvmAdapterError> {
    drop::owner(
        context, builder, function, types, runtime, module, map_type, header,
    )
}

const INITIAL_CAPACITY: u64 = 16;
const SLOT_STATE_EMPTY: u64 = 0;
const SLOT_STATE_OCCUPIED: u64 = 1;
const SLOT_STATE_DELETED: u64 = 2;

/// 获取 Map 的 Slot 结构体类型: `{ state: i32, key: KeyType, value: ValueType }`。
pub(super) fn get_slot_type<'ctx>(
    context: &'ctx Context,
    types: &TypeMap<'ctx>,
    key_ty: SsaTypeId,
    val_ty: SsaTypeId,
) -> Result<StructType<'ctx>, LlvmAdapterError> {
    let key_llvm = types.basic_type(key_ty)?;
    let val_llvm = types.basic_type(val_ty)?;
    Ok(context.struct_type(&[context.i32_type().into(), key_llvm, val_llvm], false))
}

/// 计算 Slot 的字节跨度 (stride)。
fn get_slot_stride<'ctx>(llvm: &LlvmModule<'ctx>, slot_type: StructType<'ctx>) -> u64 {
    let data_layout = llvm.get_data_layout();
    let dl_str = data_layout.as_str().to_str().unwrap_or("");
    let target = TargetData::create(dl_str);
    target.get_abi_size(&slot_type)
}

/// 构造空 Map/MutableMap，初始化容量为 16 的 buckets 缓冲区并将槽位清零。
#[allow(clippy::too_many_arguments)]
pub(super) fn construct<'ctx>(
    context: &'ctx Context,
    llvm: &LlvmModule<'ctx>,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    types: &TypeMap<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    module: &Module,
    map_type: SsaTypeId,
    name: &str,
) -> Result<StructValue<'ctx>, LlvmAdapterError> {
    let (_, key_ty, val_ty) = module
        .map_container(map_type)
        .ok_or_else(|| LlvmAdapterError::InvalidSsa("construct 目标类型不是 Map".to_owned()))?;
    let slot_type = get_slot_type(context, types, key_ty, val_ty)?;
    let slot_stride = get_slot_stride(llvm, slot_type);

    let capacity = runtime.size_type().const_int(INITIAL_CAPACITY, false);
    let buffer_bytes = runtime
        .size_type()
        .const_int(INITIAL_CAPACITY * slot_stride, false);
    let buffer = runtime.allocate_buffer(llvm, builder, function, capacity, slot_stride, name)?;

    // 将缓冲区槽位状态 memset 全 0 (即 SLOT_STATE_EMPTY)
    let zero_byte = context.i8_type().const_zero();
    builder.build_memset(buffer, 1, zero_byte, buffer_bytes)?;

    let header_type = types.basic_type(map_type)?.into_struct_type();
    let zero_size = runtime.size_type().const_zero();
    build_header(
        builder,
        header_type,
        buffer,
        zero_size,
        capacity,
        zero_size,
        name,
    )
}

/// 读取 Map 的大小 (元素数量)，返回 32 位整数。
pub(super) fn size<'ctx>(
    builder: &Builder<'ctx>,
    map_header: StructValue<'ctx>,
    name: &str,
) -> Result<IntValue<'ctx>, LlvmAdapterError> {
    let size_val = builder.build_extract_value(map_header, 1, &format!("{name}.raw_size"))?;
    let size_int = size_val.into_int_value();
    let i32_type = builder.get_insert_block().unwrap().get_context().i32_type();
    Ok(builder.build_int_cast(size_int, i32_type, name)?)
}

/// 构建 Map Header 结构体 `{ buckets, size, capacity, tombstones }`。
#[allow(clippy::too_many_arguments)]
fn build_header<'ctx>(
    builder: &Builder<'ctx>,
    header_type: StructType<'ctx>,
    buckets: PointerValue<'ctx>,
    size: IntValue<'ctx>,
    capacity: IntValue<'ctx>,
    tombstones: IntValue<'ctx>,
    name: &str,
) -> Result<StructValue<'ctx>, LlvmAdapterError> {
    let header = header_type.const_zero();
    let header = builder.build_insert_value(header, buckets, 0, &format!("{name}.with_buf"))?;
    let header = builder.build_insert_value(header, size, 1, &format!("{name}.with_size"))?;
    let header = builder.build_insert_value(header, capacity, 2, &format!("{name}.with_cap"))?;
    let header =
        builder.build_insert_value(header, tombstones, 3, &format!("{name}.with_tombs"))?;
    Ok(header.into_struct_value())
}

pub(super) use with_value::with_value;
