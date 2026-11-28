//! Emit decimal digits into a caller-owned scratch buffer.

use super::LlvmAdapterError;
use inkwell::{
    IntPredicate,
    builder::Builder,
    values::{FunctionValue, IntValue, PointerValue},
};

/// At most 20 magnitude digits and one sign.
pub(super) const BUFFER_BYTES: u32 = 21;

/// Returns the address and byte count of the fully initialized suffix.
pub(super) fn format_into<'ctx>(
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    buffer: PointerValue<'ctx>,
    value: IntValue<'ctx>,
    signed: bool,
) -> Result<(PointerValue<'ctx>, IntValue<'ctx>), LlvmAdapterError> {
    let context = value.get_type().get_context();
    let i64_type = context.i64_type();
    let i8_type = context.i8_type();
    let buffer_type = i8_type.array_type(BUFFER_BYTES);
    let value = builder.build_int_cast_sign_flag(value, i64_type, signed, "decimal.wide")?;
    let negative = if signed {
        builder.build_int_compare(IntPredicate::SLT, value, i64_type.const_zero(), "negative")?
    } else {
        context.bool_type().const_zero()
    };
    // Two's-complement negation yields the unsigned magnitude, including i64::MIN.
    let negated = builder.build_int_sub(i64_type.const_zero(), value, "negated")?;
    let magnitude = builder
        .build_select(negative, negated, value, "magnitude")?
        .into_int_value();
    let entry = builder.get_insert_block().ok_or_else(|| {
        LlvmAdapterError::Build("decimal formatting requires an insertion block".to_owned())
    })?;
    let digits = context.append_basic_block(function, "digit");
    let sign = context.append_basic_block(function, "sign");
    let emit = context.append_basic_block(function, "emit");
    builder.build_unconditional_branch(digits)?;

    builder.position_at_end(digits);
    let position = builder.build_phi(i64_type, "position")?;
    let remaining = builder.build_phi(i64_type, "remaining")?;
    position.add_incoming(&[(&i64_type.const_int(u64::from(BUFFER_BYTES), false), entry)]);
    remaining.add_incoming(&[(&magnitude, entry)]);
    let ten = i64_type.const_int(10, false);
    let slot = builder.build_int_sub(
        position.as_basic_value().into_int_value(),
        i64_type.const_int(1, false),
        "slot",
    )?;
    let digit = builder.build_int_unsigned_rem(
        remaining.as_basic_value().into_int_value(),
        ten,
        "digit.value",
    )?;
    let digit = builder.build_int_truncate(digit, i8_type, "digit.byte")?;
    let digit = builder.build_int_add(digit, i8_type.const_int(u64::from(b'0'), false), "digit")?;
    // SAFETY: slot is in 1..=20 while digits remain; the buffer has 21 bytes.
    let digit_pointer = unsafe {
        builder.build_in_bounds_gep(
            buffer_type,
            buffer,
            &[i64_type.const_zero(), slot],
            "digit.pointer",
        )?
    };
    builder.build_store(digit_pointer, digit)?;
    let quotient = builder.build_int_unsigned_div(
        remaining.as_basic_value().into_int_value(),
        ten,
        "quotient",
    )?;
    position.add_incoming(&[(&slot, digits)]);
    remaining.add_incoming(&[(&quotient, digits)]);
    let more =
        builder.build_int_compare(IntPredicate::NE, quotient, i64_type.const_zero(), "more")?;
    let finished = context.append_basic_block(function, "finished");
    builder.build_conditional_branch(more, digits, finished)?;

    builder.position_at_end(finished);
    builder.build_conditional_branch(negative, sign, emit)?;

    builder.position_at_end(sign);
    let sign_slot = builder.build_int_sub(slot, i64_type.const_int(1, false), "sign.slot")?;
    // SAFETY: at most 20 digits leave slot >= 1, so sign.slot >= 0.
    let sign_pointer = unsafe {
        builder.build_in_bounds_gep(
            buffer_type,
            buffer,
            &[i64_type.const_zero(), sign_slot],
            "sign.pointer",
        )?
    };
    builder.build_store(sign_pointer, i8_type.const_int(u64::from(b'-'), false))?;
    builder.build_unconditional_branch(emit)?;

    builder.position_at_end(emit);
    let start = builder.build_phi(i64_type, "start")?;
    start.add_incoming(&[(&slot, finished), (&sign_slot, sign)]);
    let start = start.as_basic_value().into_int_value();
    // SAFETY: start indexes the written suffix of the 21-byte buffer.
    let text = unsafe {
        builder.build_in_bounds_gep(buffer_type, buffer, &[i64_type.const_zero(), start], "text")?
    };
    let length = builder.build_int_sub(
        i64_type.const_int(u64::from(BUFFER_BYTES), false),
        start,
        "length",
    )?;
    Ok((text, length))
}
