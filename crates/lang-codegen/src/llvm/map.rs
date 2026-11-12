//! Map 容器的开地址哈希表 LLVM IR 代码生成。

use inkwell::{
    IntPredicate,
    builder::Builder,
    context::Context,
    module::Module as LlvmModule,
    targets::TargetData,
    types::{IntType, StructType},
    values::{
        BasicMetadataValueEnum, BasicValueEnum, FunctionValue, IntValue, PointerValue, StructValue,
    },
};

use crate::ssa::model::{Module, SsaTypeId, SsaTypeKind};

use super::{LlvmAdapterError, runtime::RuntimeAbi, type_map::TypeMap};

mod drop {
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
    let slot_type = get_slot_type(context, types, runtime.size_type(), key, value)?;
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
}
mod hash {
//! Map hash LLVM implementation.
use super::*;

/// 计算键的哈希值（返回 size_t IntValue）。
#[allow(clippy::too_many_arguments)]
pub(super) fn build_key_hash<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    module: &Module,
    key_ty: SsaTypeId,
    key: BasicValueEnum<'ctx>,
    name: &str,
) -> Result<IntValue<'ctx>, LlvmAdapterError> {
    match module.type_kind(key_ty) {
        Some(SsaTypeKind::Integer { bits: _, .. }) => {
            let int_val = key.into_int_value();
            let size_int = builder.build_int_cast(
                int_val,
                runtime.size_type(),
                &format!("{name}.cast_int"),
            )?;
            let multiplier = runtime.size_type().const_int(2654435761, false);
            let mul = builder.build_int_mul(size_int, multiplier, &format!("{name}.mul"))?;
            let shift = runtime.size_type().const_int(16, false);
            let shr = builder.build_right_shift(mul, shift, false, &format!("{name}.shr"))?;
            Ok(builder.build_xor(mul, shr, &format!("{name}.hash"))?)
        }
        Some(SsaTypeKind::Boolean) => {
            let bool_val = key.into_int_value();
            Ok(builder.build_int_z_extend(
                bool_val,
                runtime.size_type(),
                &format!("{name}.bool_hash"),
            )?)
        }
        Some(SsaTypeKind::Char) => {
            let char_val = key.into_int_value();
            let size_int = builder.build_int_cast(
                char_val,
                runtime.size_type(),
                &format!("{name}.char_cast"),
            )?;
            let multiplier = runtime.size_type().const_int(2654435761, false);
            Ok(builder.build_int_mul(size_int, multiplier, &format!("{name}.char_hash"))?)
        }
        Some(SsaTypeKind::StringOwner) => {
            // String: FNV-1a 哈希
            let str_struct = key.into_struct_value();
            let bytes_ptr = builder
                .build_extract_value(str_struct, 0, "str_ptr")?
                .into_pointer_value();
            let length = builder
                .build_extract_value(str_struct, 1, "str_len")?
                .into_int_value();

            let preheader = builder.get_insert_block().unwrap();
            let loop_check = context.append_basic_block(function, &format!("{name}.fnv_check"));
            let loop_body = context.append_basic_block(function, &format!("{name}.fnv_body"));
            let done = context.append_basic_block(function, &format!("{name}.fnv_done"));

            // FNV offset basis: 2166136261 (32位) / 14695981039346656037 (64位)
            let fnv_basis = if runtime.size_type().get_bit_width() >= 64 {
                14695981039346656037_u64
            } else {
                2166136261_u64
            };
            let fnv_prime = if runtime.size_type().get_bit_width() >= 64 {
                1099511628211_u64
            } else {
                16777619_u64
            };

            let basis_val = runtime.size_type().const_int(fnv_basis, false);
            let prime = runtime.size_type().const_int(fnv_prime, false);
            builder.build_unconditional_branch(loop_check)?;

            builder.position_at_end(loop_check);
            let idx_phi = builder.build_phi(runtime.size_type(), "fnv.idx")?;
            idx_phi.add_incoming(&[(&runtime.size_type().const_zero(), preheader)]);
            let hash_phi = builder.build_phi(runtime.size_type(), "fnv.hash")?;
            hash_phi.add_incoming(&[(&basis_val, preheader)]);

            let current_idx = idx_phi.as_basic_value().into_int_value();
            let current_hash = hash_phi.as_basic_value().into_int_value();

            let finished = builder.build_int_compare(
                IntPredicate::UGE,
                current_idx,
                length,
                "fnv.finished",
            )?;
            builder.build_conditional_branch(finished, done, loop_body)?;

            builder.position_at_end(loop_body);
            // SAFETY: the UTF-8 byte index is checked against length and uses an i8 layout.
            let byte_ptr = unsafe {
                builder.build_gep(context.i8_type(), bytes_ptr, &[current_idx], "byte_ptr")?
            };
            let byte_val = builder
                .build_load(context.i8_type(), byte_ptr, "byte")?
                .into_int_value();
            let byte_extended =
                builder.build_int_z_extend(byte_val, runtime.size_type(), "byte_ext")?;
            let xored = builder.build_xor(current_hash, byte_extended, "xored")?;
            let next_hash = builder.build_int_mul(xored, prime, "next_hash")?;
            let next_idx = builder.build_int_add(
                current_idx,
                runtime.size_type().const_int(1, false),
                "next_idx",
            )?;
            let loop_body_bb = builder.get_insert_block().unwrap();
            builder.build_unconditional_branch(loop_check)?;
            idx_phi.add_incoming(&[(&next_idx, loop_body_bb)]);
            hash_phi.add_incoming(&[(&next_hash, loop_body_bb)]);

            builder.position_at_end(done);
            Ok(current_hash)
        }
        _ => Err(LlvmAdapterError::Unsupported(format!(
            "不支持的 Map 键哈希类型: {key_ty:?}"
        ))),
    }
}

/// 比较两个键是否相等（返回 i1 Boolean IntValue）。
#[allow(clippy::too_many_arguments)]
pub(super) fn build_key_equal<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    module: &Module,
    key_ty: SsaTypeId,
    left: BasicValueEnum<'ctx>,
    right: BasicValueEnum<'ctx>,
    name: &str,
) -> Result<IntValue<'ctx>, LlvmAdapterError> {
    match module.type_kind(key_ty) {
        Some(SsaTypeKind::Integer { .. } | SsaTypeKind::Boolean | SsaTypeKind::Char) => {
            let left_int = left.into_int_value();
            let right_int = right.into_int_value();
            Ok(builder.build_int_compare(
                IntPredicate::EQ,
                left_int,
                right_int,
                &format!("{name}.eq"),
            )?)
        }
        Some(SsaTypeKind::StringOwner) => {
            let left_struct = left.into_struct_value();
            let right_struct = right.into_struct_value();
            let left_view = super::super::string::view(builder, left_struct, &format!("{name}.l"))?;
            let right_view =
                super::super::string::view(builder, right_struct, &format!("{name}.r"))?;
            super::super::string::equal(
                context,
                builder,
                function,
                runtime,
                left_view,
                right_view,
                &format!("{name}.streq"),
            )
        }
        _ => Err(LlvmAdapterError::Unsupported(format!(
            "不支持的 Map 键类型比较: {key_ty:?}"
        ))),
    }
}
}
pub(super) mod iteration;
mod lookup;
mod mutation;
mod order {
//! 插入链只维护 bucket 索引，不创建或清理 key/value owner。

use super::{
    Builder, Context, FunctionValue, IntPredicate, IntType, IntValue, LlvmAdapterError,
    PointerValue, StructType,
};

const PREV_FIELD: u32 = 3;
const NEXT_FIELD: u32 = 4;

#[derive(Clone, Copy)]
pub(super) struct OrderEnds<'ctx> {
    pub(super) first: IntValue<'ctx>,
    pub(super) last: IntValue<'ctx>,
}

impl<'ctx> OrderEnds<'ctx> {
    pub(super) fn empty(size_type: IntType<'ctx>) -> Self {
        let none = size_type.const_all_ones();
        Self {
            first: none,
            last: none,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct SlotTable<'ctx> {
    pub(super) slot_type: StructType<'ctx>,
    pub(super) buckets: PointerValue<'ctx>,
}

impl<'ctx> SlotTable<'ctx> {
    fn slot(
        self,
        builder: &Builder<'ctx>,
        index: IntValue<'ctx>,
        name: &str,
    ) -> Result<PointerValue<'ctx>, LlvmAdapterError> {
        // SAFETY: callers pass a bounded probe index or a non-NONE occupied chain neighbor.
        Ok(unsafe { builder.build_gep(self.slot_type, self.buckets, &[index], name)? })
    }
}

pub(super) fn append<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    table: SlotTable<'ctx>,
    ends: OrderEnds<'ctx>,
    index: IntValue<'ctx>,
    name: &str,
) -> Result<OrderEnds<'ctx>, LlvmAdapterError> {
    let none = index.get_type().const_all_ones();
    let slot = table.slot(builder, index, &format!("{name}.slot"))?;
    let prev = builder.build_struct_gep(table.slot_type, slot, PREV_FIELD, "order.prev")?;
    let next = builder.build_struct_gep(table.slot_type, slot, NEXT_FIELD, "order.next")?;
    builder.build_store(prev, ends.last)?;
    builder.build_store(next, none)?;

    let tail = context.append_basic_block(function, &format!("{name}.tail"));
    let done = context.append_basic_block(function, &format!("{name}.done"));
    let has_tail =
        builder.build_int_compare(IntPredicate::NE, ends.last, none, "order.has_tail")?;
    builder.build_conditional_branch(has_tail, tail, done)?;
    builder.position_at_end(tail);
    let previous = table.slot(builder, ends.last, "order.previous_tail")?;
    let next =
        builder.build_struct_gep(table.slot_type, previous, NEXT_FIELD, "order.tail.next")?;
    builder.build_store(next, index)?;
    builder.build_unconditional_branch(done)?;

    builder.position_at_end(done);
    let empty = builder.build_int_compare(IntPredicate::EQ, ends.first, none, "order.was_empty")?;
    let first = builder
        .build_select(empty, index, ends.first, "order.first")?
        .into_int_value();
    Ok(OrderEnds { first, last: index })
}

pub(super) fn unlink<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    table: SlotTable<'ctx>,
    ends: OrderEnds<'ctx>,
    index: IntValue<'ctx>,
    name: &str,
) -> Result<OrderEnds<'ctx>, LlvmAdapterError> {
    let size_type = index.get_type();
    let none = size_type.const_all_ones();
    let slot = table.slot(builder, index, &format!("{name}.slot"))?;
    let prev_ptr = builder.build_struct_gep(table.slot_type, slot, PREV_FIELD, "order.prev.ptr")?;
    let next_ptr = builder.build_struct_gep(table.slot_type, slot, NEXT_FIELD, "order.next.ptr")?;
    let prev = builder
        .build_load(size_type, prev_ptr, "order.prev")?
        .into_int_value();
    let next = builder
        .build_load(size_type, next_ptr, "order.next")?
        .into_int_value();

    let previous = context.append_basic_block(function, &format!("{name}.previous"));
    let previous_done = context.append_basic_block(function, &format!("{name}.previous_done"));
    let has_previous =
        builder.build_int_compare(IntPredicate::NE, prev, none, "order.has_previous")?;
    builder.build_conditional_branch(has_previous, previous, previous_done)?;
    builder.position_at_end(previous);
    let previous_slot = table.slot(builder, prev, "order.previous.slot")?;
    let previous_next = builder.build_struct_gep(
        table.slot_type,
        previous_slot,
        NEXT_FIELD,
        "order.previous.next",
    )?;
    builder.build_store(previous_next, next)?;
    builder.build_unconditional_branch(previous_done)?;
    builder.position_at_end(previous_done);
    let first = builder
        .build_select(has_previous, ends.first, next, "order.first")?
        .into_int_value();

    let following = context.append_basic_block(function, &format!("{name}.following"));
    let done = context.append_basic_block(function, &format!("{name}.done"));
    let has_next = builder.build_int_compare(IntPredicate::NE, next, none, "order.has_next")?;
    builder.build_conditional_branch(has_next, following, done)?;
    builder.position_at_end(following);
    let next_slot = table.slot(builder, next, "order.following.slot")?;
    let next_prev = builder.build_struct_gep(
        table.slot_type,
        next_slot,
        PREV_FIELD,
        "order.following.prev",
    )?;
    builder.build_store(next_prev, prev)?;
    builder.build_unconditional_branch(done)?;
    builder.position_at_end(done);
    let last = builder
        .build_select(has_next, ends.last, prev, "order.last")?
        .into_int_value();
    builder.build_store(prev_ptr, none)?;
    builder.build_store(next_ptr, none)?;
    Ok(OrderEnds { first, last })
}
}
mod rehash {
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
}
mod require {
//! 只读确定槽位位置；Missing 走既定 Abort，不读取或交付 V owner。
use super::*;
#[allow(clippy::too_many_arguments)]
pub(in crate::llvm) fn require_value<'ctx>(
    context: &'ctx Context,
    llvm: &LlvmModule<'ctx>,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    types: &TypeMap<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    module: &Module,
    map_type: SsaTypeId,
    map_header: StructValue<'ctx>,
    key: BasicValueEnum<'ctx>,
    name: &str,
) -> Result<PointerValue<'ctx>, LlvmAdapterError> {
    let (_, key_type, value_type) = module
        .map_container(map_type)
        .ok_or_else(|| LlvmAdapterError::InvalidSsa("required target is not a Map".into()))?;
    let slot_type = get_slot_type(context, types, runtime.size_type(), key_type, value_type)?;
    let slot = lookup::find_slot(
        context, llvm, builder, function, types, runtime, module, map_type, map_header, key, name,
    )?;
    let present = builder.build_is_not_null(slot, "required.present")?;
    let found = context.append_basic_block(function, "required.found");
    let missing = context.append_basic_block(function, "required.missing");
    builder.build_conditional_branch(present, found, missing)?;
    builder.position_at_end(missing);
    runtime.emit_abort(
        builder,
        crate::llvm::runtime::AbortMessage::Text("missing key"),
    )?;
    builder.position_at_end(found);
    Ok(builder.build_struct_gep(slot_type, slot, 2, name)?)
}
}
mod with_value {
//! 同步非逃逸 scoped callback；只将真实 V 字段地址交给 Borrow 参数。
use super::*;
#[allow(clippy::too_many_arguments)]
pub(in crate::llvm) fn with_value<'ctx>(
    context: &'ctx Context,
    llvm: &LlvmModule<'ctx>,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    types: &TypeMap<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    module: &Module,
    map_type: SsaTypeId,
    header: StructValue<'ctx>,
    key: BasicValueEnum<'ctx>,
    action_type: SsaTypeId,
    action: BasicValueEnum<'ctx>,
    name: &str,
) -> Result<IntValue<'ctx>, LlvmAdapterError> {
    let (_, key_type, value_type) = module
        .map_container(map_type)
        .ok_or_else(|| LlvmAdapterError::InvalidSsa("scoped source is not a Map".into()))?;
    let slot_type = get_slot_type(context, types, runtime.size_type(), key_type, value_type)?;
    let prepared =
        crate::llvm::closure::prepare(builder, module, types, action_type, action, name)?;
    let slot = lookup::find_slot(
        context, llvm, builder, function, types, runtime, module, map_type, header, key, name,
    )?;
    let present = builder.build_is_not_null(slot, "with.present")?;
    let found = context.append_basic_block(function, "with.found");
    let missing = context.append_basic_block(function, "with.missing");
    let done = context.append_basic_block(function, "with.done");
    builder.build_conditional_branch(present, found, missing)?;
    builder.position_at_end(found);
    let value = builder.build_struct_gep(slot_type, slot, 2, "with.value.slot")?;
    if crate::llvm::closure::invoke_prepared(builder, &prepared, &[value.into()], "")?.is_some() {
        return Err(LlvmAdapterError::InvalidSsa(
            "scoped callback must return Unit".into(),
        ));
    }
    let found_end = builder
        .get_insert_block()
        .ok_or_else(|| LlvmAdapterError::InvalidSsa("scoped callback missing exit".into()))?;
    builder.build_unconditional_branch(done)?;
    builder.position_at_end(missing);
    builder.build_unconditional_branch(done)?;
    builder.position_at_end(done);
    let result = builder.build_phi(context.bool_type(), name)?;
    result.add_incoming(&[
        (&context.bool_type().const_int(1, false), found_end),
        (&context.bool_type().const_zero(), missing),
    ]);
    Ok(result.as_basic_value().into_int_value())
}
}

use hash::{build_key_equal, build_key_hash};
pub(super) use lookup::{contains, get};
pub(super) use mutation::{put, remove};
use order::{OrderEnds, SlotTable};
use rehash::{RehashInput, rehash_into};
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

struct MapHeader<'ctx> {
    buckets: PointerValue<'ctx>,
    size: IntValue<'ctx>,
    capacity: IntValue<'ctx>,
    tombstones: IntValue<'ctx>,
    order: OrderEnds<'ctx>,
}

/// Slot 的 key/value 保持字段 1/2，目标宽度的 prev/next 索引追加在其后。
pub(super) fn get_slot_type<'ctx>(
    context: &'ctx Context,
    types: &TypeMap<'ctx>,
    size_type: IntType<'ctx>,
    key_ty: SsaTypeId,
    val_ty: SsaTypeId,
) -> Result<StructType<'ctx>, LlvmAdapterError> {
    let key_llvm = types.basic_type(key_ty)?;
    let val_llvm = types.basic_type(val_ty)?;
    Ok(context.struct_type(
        &[
            context.i32_type().into(),
            key_llvm,
            val_llvm,
            size_type.into(),
            size_type.into(),
        ],
        false,
    ))
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
    let slot_type = get_slot_type(context, types, runtime.size_type(), key_ty, val_ty)?;
    let slot_stride = get_slot_stride(llvm, slot_type);

    let capacity = runtime.size_type().const_int(INITIAL_CAPACITY, false);
    let buffer = runtime.allocate_buffer(llvm, builder, function, capacity, slot_stride, name)?;
    // allocate_buffer has checked stride and the target-size product before this success path.
    let buffer_bytes = builder.build_int_mul(
        capacity,
        runtime.size_type().const_int(slot_stride, false),
        &format!("{name}.buffer_bytes"),
    )?;

    // 将缓冲区槽位状态 memset 全 0 (即 SLOT_STATE_EMPTY)
    let zero_byte = context.i8_type().const_zero();
    builder.build_memset(buffer, 1, zero_byte, buffer_bytes)?;

    let header_type = types.basic_type(map_type)?.into_struct_type();
    let zero_size = runtime.size_type().const_zero();
    build_header(
        builder,
        header_type,
        MapHeader {
            buckets: buffer,
            size: zero_size,
            capacity,
            tombstones: zero_size,
            order: OrderEnds::empty(runtime.size_type()),
        },
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

/// Map 与 MutableMap 共用 `{ buckets, size, capacity, tombstones, first, last }` 布局。
fn build_header<'ctx>(
    builder: &Builder<'ctx>,
    header_type: StructType<'ctx>,
    fields: MapHeader<'ctx>,
    name: &str,
) -> Result<StructValue<'ctx>, LlvmAdapterError> {
    let header = header_type.const_zero();
    let header =
        builder.build_insert_value(header, fields.buckets, 0, &format!("{name}.with_buf"))?;
    let header =
        builder.build_insert_value(header, fields.size, 1, &format!("{name}.with_size"))?;
    let header =
        builder.build_insert_value(header, fields.capacity, 2, &format!("{name}.with_cap"))?;
    let header =
        builder.build_insert_value(header, fields.tombstones, 3, &format!("{name}.with_tombs"))?;
    let header =
        builder.build_insert_value(header, fields.order.first, 4, &format!("{name}.with_first"))?;
    let header =
        builder.build_insert_value(header, fields.order.last, 5, &format!("{name}.with_last"))?;
    Ok(header.into_struct_value())
}

pub(super) use with_value::with_value;
