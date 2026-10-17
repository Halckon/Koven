//! 名称只定位真实 SSA 对应实体；证据来自 LLVM opcode/operand/phi/CFG。
use super::{
    fixture::{Failure, failure},
    query_inputs::{Inputs, need, one},
    query_ssa::Shape,
};
use crate::ssa::model::{self as s, Definition, EntityId, Operation, Program};
use inkwell::{
    IntPredicate,
    basic_block::BasicBlock,
    context::Context,
    memory_buffer::MemoryBuffer,
    targets::TargetData,
    values::{
        AnyValue, BasicValue, BasicValueEnum, CallSiteValue, FunctionValue,
        InstructionOpcode as Op, InstructionValue, PhiValue,
    },
};

fn named<'a>(f: FunctionValue<'a>, name: &str) -> Result<InstructionValue<'a>, Failure> {
    one(
        f.get_basic_blocks()
            .into_iter()
            .flat_map(|b| b.get_instructions())
            .filter(|i| i.get_name().is_some_and(|n| n.to_str() == Ok(name))),
        "LLVM unique actual entity anchor",
    )
}
fn block<'a>(f: FunctionValue<'a>, id: s::BlockId) -> Result<BasicBlock<'a>, Failure> {
    one(
        f.get_basic_blocks()
            .into_iter()
            .filter(|b| b.get_name().to_str() == Ok(format!("bb{}", id.index()).as_str())),
        "LLVM SSA block anchor",
    )
}
fn operand(i: InstructionValue<'_>, index: u32) -> Result<BasicValueEnum<'_>, Failure> {
    i.get_operand(index)
        .and_then(|o| o.value())
        .ok_or_else(|| failure("LLVM value operand", (i, index)))
}
fn producer(i: InstructionValue<'_>, index: u32) -> Result<InstructionValue<'_>, Failure> {
    operand(i, index)?
        .as_instruction_value()
        .ok_or_else(|| failure("LLVM operand definition", (i, index)))
}
fn parent(i: InstructionValue<'_>) -> Result<BasicBlock<'_>, Failure> {
    i.get_parent()
        .ok_or_else(|| failure("LLVM instruction parent", i))
}
fn integer(v: BasicValueEnum<'_>) -> Option<u64> {
    if v.is_int_value() {
        v.into_int_value().get_zero_extended_constant()
    } else {
        None
    }
}
fn base(id: s::InstructionId) -> String {
    format!("map.iteration.i{}", id.index())
}
fn successors(b: BasicBlock<'_>) -> Result<Vec<BasicBlock<'_>>, Failure> {
    let t = b
        .get_terminator()
        .ok_or_else(|| failure("LLVM CFG terminator", b))?;
    Ok(t.get_operands()
        .filter_map(|o| o.and_then(|o| o.block()))
        .collect())
}
fn reaches<'a>(
    f: FunctionValue<'a>,
    target: BasicBlock<'a>,
    avoid: Option<BasicBlock<'a>>,
) -> Result<bool, Failure> {
    let mut todo = vec![
        f.get_first_basic_block()
            .ok_or_else(|| failure("LLVM function entry", f))?,
    ];
    let mut seen = Vec::new();
    while let Some(b) = todo.pop() {
        if Some(b) == avoid || seen.contains(&b) {
            continue;
        }
        if b == target {
            return Ok(true);
        }
        seen.push(b);
        todo.extend(successors(b)?);
    }
    Ok(false)
}
fn dominates<'a>(
    f: FunctionValue<'a>,
    a: BasicBlock<'a>,
    b: BasicBlock<'a>,
) -> Result<(), Failure> {
    need(
        reaches(f, a, None)? && reaches(f, b, None)? && !reaches(f, b, Some(a))?,
        "LLVM actual CFG dominance",
        (a, b),
    )
}
fn before(a: InstructionValue<'_>, b: InstructionValue<'_>) -> Result<(), Failure> {
    let block = parent(a)?;
    need(
        parent(b)? == block,
        "LLVM same block instruction order",
        (a, b),
    )?;
    let items = block.get_instructions().collect::<Vec<_>>();
    let pa = items
        .iter()
        .position(|i| *i == a)
        .ok_or_else(|| failure("LLVM instruction position", a))?;
    let pb = items
        .iter()
        .position(|i| *i == b)
        .ok_or_else(|| failure("LLVM instruction position", b))?;
    need(pa < pb, "LLVM ordered definitions", (a, b))
}
fn extract(i: InstructionValue<'_>, field: u32) -> Result<InstructionValue<'_>, Failure> {
    need(
        i.get_opcode() == Op::ExtractValue && i.get_indices() == [field],
        "LLVM actual header field",
        i,
    )?;
    let load = producer(i, 0)?;
    need(
        load.get_opcode() == Op::Load && load.get_type().is_struct_type(),
        "LLVM actual header load",
        load,
    )?;
    Ok(load)
}
fn branch<'a>(
    b: BasicBlock<'a>,
    condition: InstructionValue<'a>,
) -> Result<(BasicBlock<'a>, BasicBlock<'a>), Failure> {
    let t = b
        .get_terminator()
        .ok_or_else(|| failure("LLVM conditional terminator", b))?;
    need(
        t.get_opcode() == Op::Br && t.get_num_operands() == 3 && producer(t, 0)? == condition,
        "LLVM actual branch condition",
        t,
    )?;
    // LLVM conditional Branch operands are condition, false destination, true destination.
    let get = |n| {
        t.get_operand(n)
            .and_then(|o| o.block())
            .ok_or_else(|| failure("LLVM branch destination", t))
    };
    Ok((get(2)?, get(1)?))
}
fn abort_edge(b: BasicBlock<'_>) -> Result<(), Failure> {
    let t = b
        .get_terminator()
        .ok_or_else(|| failure("LLVM abort terminator", b))?;
    let call = one(
        b.get_instructions()
            .filter_map(|i| CallSiteValue::try_from(i).ok()),
        "LLVM checked-slot abort call",
    )?;
    need(
        t.get_opcode() == Op::Unreachable
            && call.count_arguments() == 0
            && call
                .get_called_fn_value()
                .is_some_and(|f| f.get_name().to_str() == Ok("abort")),
        "LLVM checked-slot abort edge",
        b,
    )
}
fn guarded<'a>(
    f: FunctionValue<'a>,
    condition: InstructionValue<'a>,
    access: InstructionValue<'a>,
) -> Result<(), Failure> {
    let (abort, success) = branch(parent(condition)?, condition)?;
    need(abort != success, "LLVM distinct guard edges", condition)?;
    abort_edge(abort)?;
    // Both the guard and its successful edge dominate the actual memory access.
    dominates(f, parent(condition)?, parent(access)?)?;
    dominates(f, success, parent(access)?)
}
fn field_gep(
    i: InstructionValue<'_>,
    slot: InstructionValue<'_>,
    field: u64,
) -> Result<(), Failure> {
    need(
        i.get_opcode() == Op::GetElementPtr
            && i.get_num_operands() == 3
            && producer(i, 0)? == slot
            && integer(operand(i, 1)?) == Some(0)
            && integer(operand(i, 2)?) == Some(field)
            && i.get_gep_source_element_type().ok() == slot.get_gep_source_element_type().ok(),
        "LLVM exact slot field GEP",
        i,
    )
}

fn emitted<'a>(
    llvm: FunctionValue<'a>,
    f: &s::Function,
    entity: EntityId,
    bits: u32,
) -> Result<BasicValueEnum<'a>, Failure> {
    let data = f
        .entity(entity)
        .ok_or_else(|| failure("LLVM source SSA entity", entity))?;
    if let Definition::BlockParameter { block: b, index } = data.definition {
        if f.entry_block() == Some(b) {
            return llvm
                .get_nth_param(index as u32)
                .ok_or_else(|| failure("LLVM entry parameter", entity));
        }
        let name = match entity {
            EntityId::Value(v) => format!("v{}", v.index()),
            EntityId::Loan(l) => format!("l{}", l.index()),
            EntityId::Place(p) => format!("p{}", p.index()),
        };
        let i = named(llvm, &name)?;
        need(
            parent(i)? == block(llvm, b)?,
            "LLVM phi belongs to actual SSA block",
            i,
        )?;
        return PhiValue::try_from(i)
            .map(|p| p.as_basic_value())
            .map_err(|e| failure("LLVM entity phi", e));
    }
    let Definition::InstructionResult { instruction, index } = data.definition else {
        return Err(failure("LLVM SSA definition", data));
    };
    let i = f
        .instruction(instruction)
        .ok_or_else(|| failure("LLVM source instruction", instruction))?;
    need(
        i.results.get(index) == Some(&entity),
        "LLVM source result identity",
        i,
    )?;
    let name = match &i.operation {
        Operation::SharedReborrow { source } => {
            return emitted(llvm, f, EntityId::Loan(*source), bits);
        }
        Operation::BorrowBegin { place, .. } => {
            return emitted(llvm, f, EntityId::Place(*place), bits);
        }
        Operation::RootPlace { .. } => match entity {
            EntityId::Place(p) => format!("p{}", p.index()),
            _ => return Err(failure("LLVM root-place result", entity)),
        },
        Operation::MapIterationFirst { .. } => {
            if bits == 64 {
                format!("{}.first", base(i.id))
            } else {
                base(i.id)
            }
        }
        Operation::MapIterationNext { .. } => {
            if bits == 64 {
                format!("{}.next", base(i.id))
            } else {
                base(i.id)
            }
        }
        Operation::MapIterationKeyLoan { .. } | Operation::MapIterationValueLoan { .. } => {
            base(i.id)
        }
        _ => return Err(failure("LLVM unsupported identity entity", i)),
    };
    let instruction = named(llvm, &name)?;
    if instruction.get_type().is_int_type() {
        inkwell::values::IntValue::try_from(instruction)
            .map(Into::into)
            .map_err(|e| failure("LLVM integer entity", e))
    } else if instruction.get_type().is_pointer_type() {
        inkwell::values::PointerValue::try_from(instruction)
            .map(Into::into)
            .map_err(|e| failure("LLVM pointer entity", e))
    } else {
        Err(failure("LLVM emitted entity type", instruction))
    }
}

fn checked_slot<'a>(
    f: FunctionValue<'a>,
    id: s::InstructionId,
    source: BasicValueEnum<'a>,
    cursor: BasicValueEnum<'a>,
    bits: u32,
) -> Result<InstructionValue<'a>, Failure> {
    let n = base(id);
    let capacity = named(f, &format!("{n}.capacity"))?;
    let header = extract(capacity, 2)?;
    need(
        operand(header, 0)? == source,
        "LLVM checked slot current source pointer",
        header,
    )?;
    let bounded = named(f, &format!("{n}.bounded"))?;
    let widened_capacity = if bits == 64 {
        capacity
    } else {
        let wide = named(f, &format!("{n}.capacity.u64"))?;
        need(
            wide.get_opcode() == Op::ZExt
                && producer(wide, 0)? == capacity
                && wide.get_type().is_int_type()
                && wide.get_type().into_int_type().get_bit_width() == 64,
            "LLVM capacity zero-extension",
            wide,
        )?;
        wide
    };
    need(
        bounded.get_icmp_predicate() == Some(IntPredicate::ULT)
            && operand(bounded, 0)? == cursor
            && producer(bounded, 1)? == widened_capacity,
        "LLVM unsigned bounds on current u64 cursor",
        bounded,
    )?;
    let present = named(f, &format!("{n}.present"))?;
    none_compare(present, cursor, bits)?;
    let valid = named(f, &format!("{n}.index.valid"))?;
    need(
        valid.get_opcode() == Op::And
            && producer(valid, 0)? == bounded
            && producer(valid, 1)? == present,
        "LLVM bounds and not-NONE conjunction",
        valid,
    )?;
    let invalid = named(f, &format!("{n}.index.invalid"))?;
    need(
        invalid.get_opcode() == Op::Xor
            && producer(invalid, 0)? == valid
            && integer(operand(invalid, 1)?) == Some(1),
        "LLVM invalid guard inversion",
        invalid,
    )?;
    let buckets = named(f, &format!("{n}.buckets"))?;
    need(
        extract(buckets, 0)? == header,
        "LLVM slots from same source header",
        buckets,
    )?;
    let slot = named(f, &format!("{n}.slot"))?;
    let index = operand(slot, 1)?;
    if bits == 64 {
        need(index == cursor, "LLVM slot current cursor", slot)?;
    } else {
        let trunc = index
            .as_instruction_value()
            .ok_or_else(|| failure("LLVM narrow slot index", slot))?;
        need(
            trunc.get_opcode() == Op::Trunc
                && operand(trunc, 0)? == cursor
                && trunc.get_type().is_int_type()
                && trunc.get_type().into_int_type().get_bit_width() == bits,
            "LLVM checked cursor truncation",
            trunc,
        )?;
        guarded(f, invalid, trunc)?;
    }
    need(
        slot.get_opcode() == Op::GetElementPtr
            && slot.get_num_operands() == 2
            && producer(slot, 0)? == buckets,
        "LLVM actual bucket GEP identity",
        slot,
    )?;
    // Inkwell has no read API for the GEP inbounds bit; syntax is used only for this flag.
    need(
        !slot
            .print_to_string()
            .to_string()
            .contains("getelementptr inbounds"),
        "LLVM physical slot GEP non-inbounds",
        slot,
    )?;
    guarded(f, invalid, slot)?;
    let layout = slot
        .get_gep_source_element_type()
        .map_err(|e| failure("LLVM slot source type", e))?;
    need(layout.is_struct_type(), "LLVM slot struct", layout)?;
    let layout = layout.into_struct_type();
    need(
        layout.count_fields() == 5
            && layout
                .get_field_type_at_index(0)
                .is_some_and(|t| t.is_int_type() && t.into_int_type().get_bit_width() == 32)
            && layout.get_field_type_at_index(1) == layout.get_field_type_at_index(2)
            && [3, 4].iter().all(|&i| {
                layout
                    .get_field_type_at_index(i)
                    .is_some_and(|t| t.is_int_type() && t.into_int_type().get_bit_width() == bits)
            }),
        "LLVM slot five-field K/V/size layout",
        layout,
    )?;
    let state_ptr = named(f, &format!("{n}.state.ptr"))?;
    field_gep(state_ptr, slot, 0)?;
    let state = named(f, &format!("{n}.state"))?;
    need(
        state.get_opcode() == Op::Load
            && producer(state, 0)? == state_ptr
            && state.get_type().is_int_type()
            && state.get_type().into_int_type().get_bit_width() == 32,
        "LLVM actual slot state load",
        state,
    )?;
    guarded(f, invalid, state)?;
    let dead = named(f, &format!("{n}.dead"))?;
    need(
        dead.get_icmp_predicate() == Some(IntPredicate::NE)
            && producer(dead, 0)? == state
            && integer(operand(dead, 1)?) == Some(1),
        "LLVM actual occupied-state test",
        dead,
    )?;
    let field = named(f, &n).ok();
    // K/V result is its field GEP; Next names the field pointer and loaded next separately.
    if let Some(field) = field.filter(|i| i.get_opcode() == Op::GetElementPtr) {
        guarded(f, dead, field)?;
    } else {
        let pointer = named(f, &format!("{n}.next.ptr"))?;
        let load = named(f, &format!("{n}.next"))?;
        guarded(f, dead, pointer)?;
        guarded(f, dead, load)?;
    }
    Ok(slot)
}
fn none_compare(
    i: InstructionValue<'_>,
    cursor: BasicValueEnum<'_>,
    bits: u32,
) -> Result<(), Failure> {
    let none = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    need(
        i.get_icmp_predicate() == Some(IntPredicate::NE)
            && operand(i, 0)? == cursor
            && integer(operand(i, 1)?) == Some(none),
        "LLVM cursor compares target-width NONE",
        i,
    )
}

pub(super) fn check(
    program: &Program,
    inputs: &Inputs,
    shape: &Shape,
    text: &str,
) -> Result<(), Failure> {
    let context = Context::create();
    let llvm = context
        .create_module_from_ir(MemoryBuffer::create_from_memory_range_copy(
            text.as_bytes(),
            "provider-identity",
        ))
        .map_err(|e| failure("LLVM identity parse", e))?;
    let module = program
        .module(shape.scan.module())
        .ok_or_else(|| failure("LLVM actual SSA module", shape.scan))?;
    let scan = module
        .function(shape.scan)
        .ok_or_else(|| failure("LLVM actual SSA scan", shape.scan))?;
    need(
        scan.origin == s::Origin::Source(inputs.scan),
        "LLVM source-qualified FunctionId origin",
        scan.id,
    )?;
    let symbol = format!("f{}.{}", scan.id.index(), scan.name);
    let f = one(
        llvm.get_functions()
            .filter(|f| f.get_name().to_str() == Ok(symbol.as_str())),
        "LLVM actual scan FunctionId/name",
    )?;
    let layout = llvm
        .get_data_layout()
        .as_str()
        .to_str()
        .map_err(|e| failure("LLVM target layout", e))?
        .to_owned();
    need(
        !layout.is_empty(),
        "LLVM nonempty actual data layout",
        &layout,
    )?;
    let data = TargetData::create(&layout);
    let bits = data.get_pointer_byte_size(None) * 8;
    need((1..=64).contains(&bits), "LLVM supported target size", bits)?;
    let [first_id, guard_id, key_id, value_id, next_id] = shape.operations;
    let first = named(f, &format!("{}.first", base(first_id)))?;
    let load = extract(first, 4)?;
    let header = load.get_type().into_struct_type();
    let map_type = scan
        .entity(shape.seed)
        .ok_or_else(|| failure("LLVM SSA source type", shape.seed))?
        .ty
        .semantic_type();
    let (_, kt, vt) = module
        .map_container(map_type)
        .ok_or_else(|| failure("LLVM SSA Map type", map_type))?;
    let expected_header = context
        .get_struct_type(&format!("koven.map.t{}", map_type.index()))
        .ok_or_else(|| failure("LLVM actual mapped header type", map_type))?;
    need(
        header == expected_header,
        "LLVM source header matches actual SSA Map storage",
        header,
    )?;
    let key_type = context
        .get_struct_type(&format!("koven.string.t{}", kt.index()))
        .ok_or_else(|| failure("LLVM mapped key String", kt))?;
    let value_type = context
        .get_struct_type(&format!("koven.string.t{}", vt.index()))
        .ok_or_else(|| failure("LLVM mapped value String", vt))?;
    for ty in [key_type, value_type] {
        need(
            ty.count_fields() == 3
                && ty
                    .get_field_type_at_index(0)
                    .is_some_and(|t| t.is_pointer_type())
                && [1, 2].iter().all(|&i| {
                    ty.get_field_type_at_index(i).is_some_and(|t| {
                        t.is_int_type() && t.into_int_type().get_bit_width() == bits
                    })
                }),
            "LLVM actual String owner storage",
            ty,
        )?;
    }

    need(
        header.count_fields() == 6
            && header
                .get_field_type_at_index(0)
                .is_some_and(|t| t.is_pointer_type())
            && (1..6).all(|i| {
                header
                    .get_field_type_at_index(i)
                    .is_some_and(|t| t.is_int_type() && t.into_int_type().get_bit_width() == bits)
            }),
        "LLVM actual six-field header",
        header,
    )?;
    need(
        parent(first)? == block(f, shape.blocks[0])?
            && operand(load, 0)? == emitted(f, scan, EntityId::Loan(shape.lease), bits)?,
        "LLVM First actual source lease",
        first,
    )?;
    before(load, first)?;
    if let EntityId::Value(_) = shape.seed {
        let pointer = operand(load, 0)?;
        let alloca = pointer
            .as_instruction_value()
            .ok_or_else(|| failure("LLVM owned root storage", load))?;
        need(
            alloca.get_opcode() == Op::Alloca
                && alloca.get_allocated_type().ok() == Some(header.into()),
            "LLVM owned source actual root alloca",
            alloca,
        )?;
        let store = one(
            parent(load)?.get_instructions().filter(|i| {
                i.get_opcode() == Op::Store
                    && i.get_operand(1).and_then(|o| o.value()) == Some(pointer)
            }),
            "LLVM actual source parameter store",
        )?;
        need(
            operand(store, 0)? == emitted(f, scan, shape.seed, bits)?,
            "LLVM root storage from actual entry parameter",
            store,
        )?;
        before(store, load)?;
    }
    let hc = emitted(f, scan, EntityId::Value(shape.header_cursor), bits)?;
    let bc = emitted(f, scan, EntityId::Value(shape.body_cursor), bits)?;
    need(
        hc.is_int_value()
            && bc.is_int_value()
            && hc.into_int_value().get_type().get_bit_width() == 64
            && bc.into_int_value().get_type().get_bit_width() == 64,
        "LLVM actual u64 cursor phi",
        (hc, bc),
    )?;
    let guard = named(f, &base(guard_id))?;
    none_compare(guard, hc, bits)?;
    let (when_true, when_false) = branch(block(f, shape.blocks[1])?, guard)?;
    need(
        when_true == block(f, shape.blocks[2])? && when_false == block(f, shape.blocks[3])?,
        "LLVM HasNext actual true/zero-round false destinations",
        guard,
    )?;
    let source = emitted(f, scan, EntityId::Loan(shape.body_source), bits)?;
    for (id, field) in [(key_id, 1), (value_id, 2), (next_id, 4)] {
        let slot = checked_slot(f, id, source, bc, bits)?;
        let slot_type = slot
            .get_gep_source_element_type()
            .map_err(|e| failure("LLVM actual K/V slot type", e))?
            .into_struct_type();
        need(
            slot_type.get_field_type_at_index(1) == Some(key_type.into())
                && slot_type.get_field_type_at_index(2) == Some(value_type.into()),
            "LLVM slot K/V match SSA mapped storage",
            slot_type,
        )?;
        // Record real target offsets without inventing architecture-independent byte numbers.
        println!(
            "SPEC0331 PROVIDER IDENTITY LLVM SLOT op={id:?} offsets={:?}",
            (0..5)
                .map(|i| data.offset_of_element(&slot_type, i))
                .collect::<Vec<_>>()
        );

        dominates(f, when_true, parent(slot)?)?;
        let pointer = named(
            f,
            &if id == next_id {
                format!("{}.next.ptr", base(id))
            } else {
                base(id)
            },
        )?;
        field_gep(pointer, slot, field)?;
        if id == next_id {
            let next = named(f, &format!("{}.next", base(id)))?;
            need(
                next.get_opcode() == Op::Load
                    && producer(next, 0)? == pointer
                    && next.get_type().is_int_type()
                    && next.get_type().into_int_type().get_bit_width() == bits,
                "LLVM Next reads current slot.next",
                next,
            )?;
        }
    }
    let read = module
        .function(shape.read)
        .ok_or_else(|| failure("LLVM read SSA function", shape.read))?;
    need(
        read.origin == s::Origin::Source(inputs.read),
        "LLVM read origin identity",
        read.id,
    )?;
    let read_symbol = format!("f{}.{}", read.id.index(), read.name);
    let callee = llvm
        .get_function(&read_symbol)
        .ok_or_else(|| failure("LLVM actual read callee", &read_symbol))?;
    let call = one(
        f.get_basic_blocks()
            .into_iter()
            .flat_map(|b| b.get_instructions())
            .filter_map(|i| {
                let c = CallSiteValue::try_from(i).ok()?;
                (c.get_called_fn_value() == Some(callee)).then_some((i, c))
            }),
        "LLVM actual read call",
    )?;
    need(
        call.1.count_arguments() == 2
            && producer(call.0, 0)? == named(f, &base(key_id))?
            && producer(call.0, 1)? == named(f, &base(value_id))?,
        "LLVM ordered K/V pointer delivery",
        call.0,
    )?;
    let next_load = named(f, &format!("{}.next", base(next_id)))?;
    if bits < 64 {
        for (id, narrow) in [(first_id, first), (next_id, next_load)] {
            let wide = named(f, &base(id))?;
            need(
                wide.get_opcode() == Op::ZExt
                    && producer(wide, 0)? == narrow
                    && wide.get_type().is_int_type()
                    && wide.get_type().into_int_type().get_bit_width() == 64,
                "LLVM cursor lossless target-size to u64",
                wide,
            )?;
        }
    }

    if parent(call.0)? == parent(next_load)? {
        before(call.0, next_load)?;
    } else {
        dominates(f, parent(call.0)?, parent(next_load)?)?;
    }
    let mut provider_accesses = Vec::new();
    for id in [key_id, value_id, next_id] {
        provider_accesses.push(named(f, &format!("{}.slot", base(id)))?);
    }
    provider_accesses.extend([
        named(f, &base(key_id))?,
        named(f, &base(value_id))?,
        next_load,
    ]);
    let mut false_todo = vec![when_false];
    let mut false_seen = Vec::new();
    while let Some(b) = false_todo.pop() {
        if false_seen.contains(&b) {
            continue;
        }
        false_seen.push(b);
        need(
            b != block(f, shape.blocks[1])? && b != when_true,
            "LLVM exhaustion never reenters provider",
            b,
        )?;
        need(
            provider_accesses.iter().all(|i| i.get_parent() != Some(b)),
            "LLVM zero-round false cannot reach this provider's slot/K/V/Next accesses",
            b,
        )?;
        false_todo.extend(successors(b)?);
    }
    // Every real incoming gets its own expected SSA edge argument, including both Header roots.
    for id in &shape.blocks[1..] {
        let target = scan
            .block(*id)
            .ok_or_else(|| failure("LLVM SSA phi target", id))?;
        let expected = shape
            .edges
            .iter()
            .filter(|(_, e)| e.target == *id)
            .collect::<Vec<_>>();
        for (column, &entity) in target.parameters.iter().enumerate() {
            let actual = emitted(f, scan, entity, bits)?
                .as_instruction_value()
                .ok_or_else(|| failure("LLVM actual phi", entity))?;
            let phi = PhiValue::try_from(actual).map_err(|e| failure("LLVM phi instruction", e))?;
            let incoming = phi.get_incomings().collect::<Vec<_>>();
            need(
                incoming.len() == expected.len(),
                "LLVM all phi incoming cardinality",
                actual,
            )?;
            for (from, edge) in &expected {
                let predecessor = if *from == shape.blocks[2] {
                    parent(next_load)?
                } else {
                    block(f, *from)?
                };
                let (value, _) = one(
                    incoming.iter().filter(|(_, b)| *b == predecessor),
                    "LLVM exact predecessor phi edge",
                )?;
                need(
                    *value == emitted(f, scan, edge.arguments[column], bits)?
                        && successors(predecessor)?.contains(&block(f, *id)?),
                    "LLVM actual phi argument and CFG edge",
                    (actual, edge),
                )?;
            }
        }
    }
    // Header loan phi is checked independently of the cursor phi, with actual edge operands above.
    let hs = emitted(f, scan, EntityId::Loan(shape.header_source), bits)?;
    need(hs.is_pointer_value(), "LLVM actual header source phi", hs)?;
    println!(
        "SPEC0331 PROVIDER IDENTITY LLVM function={symbol:?} size_bits={bits} checked_slot_ops={:?}",
        [key_id, value_id, next_id]
    );
    Ok(())
}
