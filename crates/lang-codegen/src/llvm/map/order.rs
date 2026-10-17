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
