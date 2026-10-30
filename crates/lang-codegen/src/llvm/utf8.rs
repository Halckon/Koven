//! Shared strict UTF-8 scanner for argv preflight and standard text decoding.
use super::LlvmAdapterError;
use inkwell::{
    AddressSpace, IntPredicate,
    context::Context,
    module::{Linkage, Module as LlvmModule},
    values::{FunctionValue, IntValue},
};

/// Return target-usize MAX for valid input, otherwise the first invalid sequence start.
/// The caller provides a readable byte range; a zero length never dereferences its pointer.
pub(super) fn define_error_offset<'ctx>(
    context: &'ctx Context,
    module: &LlvmModule<'ctx>,
    size: inkwell::types::IntType<'ctx>,
) -> Result<FunctionValue<'ctx>, LlvmAdapterError> {
    if let Some(function) = module.get_function("koven.utf8.error_offset") {
        return Ok(function);
    }
    let pointer = context.ptr_type(AddressSpace::default());
    let function = module.add_function(
        "koven.utf8.error_offset",
        size.fn_type(&[pointer.into(), size.into()], false),
        Some(Linkage::Internal),
    );
    let bytes = function
        .get_nth_param(0)
        .expect("bytes")
        .into_pointer_value();
    let length = function.get_nth_param(1).expect("length").into_int_value();
    let builder = context.create_builder();
    let entry = context.append_basic_block(function, "entry");
    let header = context.append_basic_block(function, "scan");
    let body = context.append_basic_block(function, "byte");
    let advance = context.append_basic_block(function, "advance");
    let invalid = context.append_basic_block(function, "invalid");
    let done = context.append_basic_block(function, "done");
    builder.position_at_end(entry);
    builder.build_unconditional_branch(header)?;
    builder.position_at_end(header);
    let index_phi = builder.build_phi(size, "index")?;
    let start_phi = builder.build_phi(size, "sequence.start")?;
    let remaining_phi = builder.build_phi(context.i8_type(), "remaining")?;
    let minimum_phi = builder.build_phi(context.i8_type(), "minimum")?;
    let maximum_phi = builder.build_phi(context.i8_type(), "maximum")?;
    let zero_size = size.const_zero();
    let zero8 = context.i8_type().const_zero();
    let min_cont = context.i8_type().const_int(0x80, false);
    let max_cont = context.i8_type().const_int(0xbf, false);
    index_phi.add_incoming(&[(&zero_size, entry)]);
    start_phi.add_incoming(&[(&zero_size, entry)]);
    remaining_phi.add_incoming(&[(&zero8, entry)]);
    minimum_phi.add_incoming(&[(&min_cont, entry)]);
    maximum_phi.add_incoming(&[(&max_cont, entry)]);
    let index = index_phi.as_basic_value().into_int_value();
    let start = start_phi.as_basic_value().into_int_value();
    let remaining = remaining_phi.as_basic_value().into_int_value();
    let minimum = minimum_phi.as_basic_value().into_int_value();
    let maximum = maximum_phi.as_basic_value().into_int_value();
    let remains = builder.build_int_compare(IntPredicate::ULT, index, length, "remains")?;
    builder.build_conditional_branch(remains, body, done)?;
    builder.position_at_end(done);
    let complete = builder.build_int_compare(IntPredicate::EQ, remaining, zero8, "complete")?;
    let result = builder
        .build_select(complete, size.const_all_ones(), start, "result")?
        .into_int_value();
    builder.build_return(Some(&result))?;

    builder.position_at_end(body);
    // SAFETY: body is reachable only for index < length.
    let address =
        unsafe { builder.build_in_bounds_gep(context.i8_type(), bytes, &[index], "address")? };
    let byte = builder
        .build_load(context.i8_type(), address, "byte")?
        .into_int_value();
    let continuing = builder.build_int_compare(IntPredicate::NE, remaining, zero8, "continuing")?;
    let at_least_min =
        builder.build_int_compare(IntPredicate::UGE, byte, minimum, "at_least_min")?;
    let at_most_max = builder.build_int_compare(IntPredicate::ULE, byte, maximum, "at_most_max")?;
    let valid_cont = builder.build_and(at_least_min, at_most_max, "valid_cont")?;
    let ascii = builder.build_int_compare(
        IntPredicate::ULT,
        byte,
        context.i8_type().const_int(0x80, false),
        "ascii",
    )?;
    let lead2 = int_range(&builder, byte, 0xc2, 0xdf, "lead2")?;
    let lead3 = int_range(&builder, byte, 0xe0, 0xef, "lead3")?;
    let lead4 = int_range(&builder, byte, 0xf0, 0xf4, "lead4")?;
    let valid_lead = builder.build_or(
        ascii,
        builder.build_or(lead2, builder.build_or(lead3, lead4, "lead34")?, "lead234")?,
        "valid_lead",
    )?;
    let valid = builder
        .build_select(continuing, valid_cont, valid_lead, "valid")?
        .into_int_value();
    builder.build_conditional_branch(valid, advance, invalid)?;
    builder.position_at_end(invalid);
    let error_start = builder
        .build_select(continuing, start, index, "error.start")?
        .into_int_value();
    builder.build_return(Some(&error_start))?;

    builder.position_at_end(advance);
    let continued_remaining = builder.build_int_sub(
        remaining,
        context.i8_type().const_int(1, false),
        "continued_remaining",
    )?;
    let lead2_remaining = builder
        .build_select(
            lead2,
            context.i8_type().const_int(1, false),
            zero8,
            "lead2.remaining",
        )?
        .into_int_value();
    let lead3_remaining = builder
        .build_select(
            lead3,
            context.i8_type().const_int(2, false),
            lead2_remaining,
            "lead3.remaining",
        )?
        .into_int_value();
    let fresh_remaining = builder
        .build_select(
            lead4,
            context.i8_type().const_int(3, false),
            lead3_remaining,
            "lead4.remaining",
        )?
        .into_int_value();
    let next_remaining = builder
        .build_select(
            continuing,
            continued_remaining,
            fresh_remaining,
            "next.remaining",
        )?
        .into_int_value();
    let e0 = builder.build_int_compare(
        IntPredicate::EQ,
        byte,
        context.i8_type().const_int(0xe0, false),
        "e0",
    )?;
    let ed = builder.build_int_compare(
        IntPredicate::EQ,
        byte,
        context.i8_type().const_int(0xed, false),
        "ed",
    )?;
    let f0 = builder.build_int_compare(
        IntPredicate::EQ,
        byte,
        context.i8_type().const_int(0xf0, false),
        "f0",
    )?;
    let f4 = builder.build_int_compare(
        IntPredicate::EQ,
        byte,
        context.i8_type().const_int(0xf4, false),
        "f4",
    )?;
    let f0_min = builder
        .build_select(
            f0,
            context.i8_type().const_int(0x90, false),
            min_cont,
            "f0.min",
        )?
        .into_int_value();
    let fresh_min = builder
        .build_select(
            e0,
            context.i8_type().const_int(0xa0, false),
            f0_min,
            "e0.min",
        )?
        .into_int_value();
    let f4_max = builder
        .build_select(
            f4,
            context.i8_type().const_int(0x8f, false),
            max_cont,
            "f4.max",
        )?
        .into_int_value();
    let fresh_max = builder
        .build_select(
            ed,
            context.i8_type().const_int(0x9f, false),
            f4_max,
            "ed.max",
        )?
        .into_int_value();
    let next_start = builder
        .build_select(continuing, start, index, "next.start")?
        .into_int_value();
    let next_index = builder.build_int_add(index, size.const_int(1, false), "next.index")?;
    builder.build_unconditional_branch(header)?;
    index_phi.add_incoming(&[(&next_index, advance)]);
    start_phi.add_incoming(&[(&next_start, advance)]);
    remaining_phi.add_incoming(&[(&next_remaining, advance)]);
    minimum_phi.add_incoming(&[(&fresh_min, advance)]);
    maximum_phi.add_incoming(&[(&fresh_max, advance)]);
    Ok(function)
}

fn int_range<'ctx>(
    builder: &inkwell::builder::Builder<'ctx>,
    value: IntValue<'ctx>,
    minimum: u64,
    maximum: u64,
    name: &str,
) -> Result<IntValue<'ctx>, LlvmAdapterError> {
    let ty = value.get_type();
    let lower = builder.build_int_compare(
        IntPredicate::UGE,
        value,
        ty.const_int(minimum, false),
        &format!("{name}.lower"),
    )?;
    let upper = builder.build_int_compare(
        IntPredicate::ULE,
        value,
        ty.const_int(maximum, false),
        &format!("{name}.upper"),
    )?;
    Ok(builder.build_and(lower, upper, name)?)
}
