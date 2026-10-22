//! Guide v0.46 abort output contract (SPEC-0347).
//!
//! Every abort writes `abort: <message>\n` and, when the source position is known,
//! `  at <logical-path>:<line>:<column>\n` to stderr, ignores write failures, then calls
//! `abort()`. Messages are either fixed text, the `error(message)` String bytes, or the
//! index bounds text with run-time integers.

use inkwell::{
    AddressSpace, IntPredicate,
    builder::Builder,
    context::Context,
    module::{Linkage, Module as LlvmModule},
    types::IntType,
    values::{
        BasicMetadataValueEnum, BasicValueEnum, FunctionValue, IntValue, PointerValue, ValueKind,
    },
};

use super::{RuntimeAbi, add_noreturn_attribute};
use crate::llvm::LlvmAdapterError;

const STDERR: u64 = 2;
/// `u64::MAX` has 20 decimal digits; one more byte holds the sign.
const DECIMAL_BUFFER: u32 = 21;

/// What the `abort: <message>` line prints.
#[derive(Clone, Copy)]
pub(in crate::llvm) enum AbortMessage<'ctx, 'text> {
    Text(&'text str),
    /// UTF-8 bytes written verbatim, such as the `error(message)` argument.
    Bytes {
        bytes: PointerValue<'ctx>,
        length: IntValue<'ctx>,
    },
    /// `index <index> out of bounds for size <size>`; `index` is signed, `size` unsigned.
    IndexOutOfBounds {
        index: IntValue<'ctx>,
        size: IntValue<'ctx>,
    },
    /// `when_true` if `condition` holds at run time, otherwise `when_false`.
    Select {
        condition: IntValue<'ctx>,
        when_true: &'text str,
        when_false: &'text str,
    },
}

/// Module-level helpers declared when the module can abort.
pub(super) struct AbortHelpers<'ctx> {
    /// `(message ptr, message len, location ptr, location len) -> noreturn`.
    report: FunctionValue<'ctx>,
    /// `(index i64, size i64, location ptr, location len) -> noreturn`.
    report_index: FunctionValue<'ctx>,
}

impl<'ctx> AbortHelpers<'ctx> {
    pub(super) fn define(
        context: &'ctx Context,
        llvm: &LlvmModule<'ctx>,
        size_type: IntType<'ctx>,
        write: FunctionValue<'ctx>,
        abort: FunctionValue<'ctx>,
    ) -> Result<Self, LlvmAdapterError> {
        let pointer = context.ptr_type(AddressSpace::default());
        let i64_type = context.i64_type();
        let write_bytes = define_write_bytes(context, llvm, size_type, write)?;
        let write_decimal = define_write_decimal(context, llvm, size_type, write_bytes)?;

        let report = llvm.add_function(
            "koven.abort.report",
            context.void_type().fn_type(
                &[
                    pointer.into(),
                    size_type.into(),
                    pointer.into(),
                    size_type.into(),
                ],
                false,
            ),
            Some(Linkage::Internal),
        );
        add_noreturn_attribute(context, report);
        {
            let builder = context.create_builder();
            builder.position_at_end(context.append_basic_block(report, "entry"));
            let parameter = |index| report.get_nth_param(index).expect("abort report parameter");
            write_text(&builder, size_type, write_bytes, "abort: ", "abort.prefix")?;
            call(
                &builder,
                write_bytes,
                &[parameter(0).into(), parameter(1).into()],
            )?;
            write_text(&builder, size_type, write_bytes, "\n", "abort.newline")?;
            call(
                &builder,
                write_bytes,
                &[parameter(2).into(), parameter(3).into()],
            )?;
            call(&builder, abort, &[])?;
            builder.build_unreachable()?;
        }

        let report_index = llvm.add_function(
            "koven.abort.report_index",
            context.void_type().fn_type(
                &[
                    i64_type.into(),
                    i64_type.into(),
                    pointer.into(),
                    size_type.into(),
                ],
                false,
            ),
            Some(Linkage::Internal),
        );
        add_noreturn_attribute(context, report_index);
        {
            let builder = context.create_builder();
            builder.position_at_end(context.append_basic_block(report_index, "entry"));
            let parameter = |index| {
                report_index
                    .get_nth_param(index)
                    .expect("abort index parameter")
            };
            write_text(
                &builder,
                size_type,
                write_bytes,
                "abort: index ",
                "abort.index.prefix",
            )?;
            call(&builder, write_decimal, &[parameter(0).into()])?;
            write_text(
                &builder,
                size_type,
                write_bytes,
                " out of bounds for size ",
                "abort.index.middle",
            )?;
            call(&builder, write_decimal, &[parameter(1).into()])?;
            write_text(
                &builder,
                size_type,
                write_bytes,
                "\n",
                "abort.index.newline",
            )?;
            call(
                &builder,
                write_bytes,
                &[parameter(2).into(), parameter(3).into()],
            )?;
            call(&builder, abort, &[])?;
            builder.build_unreachable()?;
        }
        Ok(Self {
            report,
            report_index,
        })
    }
}

impl<'ctx> RuntimeAbi<'ctx> {
    /// Sets the `  at path:line:column` line used by aborts emitted until the next call;
    /// `None` omits the location line.
    pub(in crate::llvm) fn set_abort_location(&self, location: Option<String>) {
        *self.abort_location.borrow_mut() = location;
    }

    /// Emits the abort report at the builder position and terminates the block.
    pub(in crate::llvm) fn emit_abort_with(
        &self,
        builder: &Builder<'ctx>,
        message: AbortMessage<'ctx, '_>,
    ) -> Result<(), LlvmAdapterError> {
        let helpers = self
            .abort_helpers
            .as_ref()
            .ok_or_else(|| LlvmAdapterError::Build("abort 未声明".to_owned()))?;
        let (location, location_length) = self.location_operands(builder)?;
        match message {
            AbortMessage::IndexOutOfBounds { index, size } => {
                let i64_type = self.context.i64_type();
                let index =
                    builder.build_int_s_extend_or_bit_cast(index, i64_type, "abort.index.value")?;
                let size =
                    builder.build_int_z_extend_or_bit_cast(size, i64_type, "abort.index.size")?;
                call(
                    builder,
                    helpers.report_index,
                    &[
                        index.into(),
                        size.into(),
                        location.into(),
                        location_length.into(),
                    ],
                )?;
            }
            message => {
                let (bytes, length) = match message {
                    AbortMessage::Text(text) => {
                        self.text_operands(builder, text, "abort.message")?
                    }
                    AbortMessage::Bytes { bytes, length } => (bytes, length),
                    AbortMessage::Select {
                        condition,
                        when_true,
                        when_false,
                    } => {
                        let (true_bytes, true_length) =
                            self.text_operands(builder, when_true, "abort.message.true")?;
                        let (false_bytes, false_length) =
                            self.text_operands(builder, when_false, "abort.message.false")?;
                        let bytes = builder
                            .build_select(condition, true_bytes, false_bytes, "abort.message")?
                            .into_pointer_value();
                        let length = builder
                            .build_select(
                                condition,
                                true_length,
                                false_length,
                                "abort.message.length",
                            )?
                            .into_int_value();
                        (bytes, length)
                    }
                    AbortMessage::IndexOutOfBounds { .. } => unreachable!("handled above"),
                };
                call(
                    builder,
                    helpers.report,
                    &[
                        bytes.into(),
                        length.into(),
                        location.into(),
                        location_length.into(),
                    ],
                )?;
            }
        }
        builder.build_unreachable()?;
        Ok(())
    }

    fn location_operands(
        &self,
        builder: &Builder<'ctx>,
    ) -> Result<(PointerValue<'ctx>, IntValue<'ctx>), LlvmAdapterError> {
        let location = self.abort_location.borrow().clone();
        // An unknown position writes zero bytes; an empty text keeps IR free of null pointers.
        self.text_operands(builder, location.as_deref().unwrap_or(""), "abort.location")
    }

    fn text_operands(
        &self,
        builder: &Builder<'ctx>,
        text: &str,
        name: &str,
    ) -> Result<(PointerValue<'ctx>, IntValue<'ctx>), LlvmAdapterError> {
        let length = u64::try_from(text.len())
            .map_err(|_| LlvmAdapterError::Build("abort text 长度超出 u64".to_owned()))?;
        let global = builder.build_global_string_ptr(text, name)?;
        global.set_linkage(Linkage::Private);
        Ok((
            global.as_pointer_value(),
            self.size_type.const_int(length, false),
        ))
    }
}

/// `koven.abort.write(ptr, len)`: writes to stderr, retrying short writes and ignoring
/// failures, as the contract requires.
fn define_write_bytes<'ctx>(
    context: &'ctx Context,
    llvm: &LlvmModule<'ctx>,
    size_type: IntType<'ctx>,
    write: FunctionValue<'ctx>,
) -> Result<FunctionValue<'ctx>, LlvmAdapterError> {
    let pointer = context.ptr_type(AddressSpace::default());
    let function = llvm.add_function(
        "koven.abort.write",
        context
            .void_type()
            .fn_type(&[pointer.into(), size_type.into()], false),
        Some(Linkage::Internal),
    );
    let bytes = function
        .get_nth_param(0)
        .expect("bytes")
        .into_pointer_value();
    let length = function.get_nth_param(1).expect("length").into_int_value();
    let builder = context.create_builder();
    let entry = context.append_basic_block(function, "entry");
    let header = context.append_basic_block(function, "loop");
    let body = context.append_basic_block(function, "write");
    let done = context.append_basic_block(function, "done");
    builder.position_at_end(entry);
    builder.build_unconditional_branch(header)?;

    builder.position_at_end(header);
    let offset = builder.build_phi(size_type, "offset")?;
    offset.add_incoming(&[(&size_type.const_zero(), entry)]);
    let current = offset.as_basic_value().into_int_value();
    let remaining = builder.build_int_sub(length, current, "remaining")?;
    let finished = builder.build_int_compare(
        IntPredicate::EQ,
        remaining,
        size_type.const_zero(),
        "finished",
    )?;
    builder.build_conditional_branch(finished, done, body)?;

    builder.position_at_end(body);
    // SAFETY: offset < length, inside the caller-provided byte range.
    let cursor =
        unsafe { builder.build_in_bounds_gep(context.i8_type(), bytes, &[current], "cursor")? };
    let written = match builder
        .build_call(
            write,
            &[
                BasicMetadataValueEnum::from(context.i32_type().const_int(STDERR, false)),
                BasicMetadataValueEnum::from(cursor),
                BasicMetadataValueEnum::from(remaining),
            ],
            "written",
        )?
        .try_as_basic_value()
    {
        ValueKind::Basic(BasicValueEnum::IntValue(value)) => value,
        _ => {
            return Err(LlvmAdapterError::Build(
                "write 未返回整数 byte count".to_owned(),
            ));
        }
    };
    // write returns -1 on failure and never more than `remaining`; stop on <= 0.
    let progressed = builder.build_int_compare(
        IntPredicate::SGT,
        written,
        size_type.const_zero(),
        "progressed",
    )?;
    let next = builder.build_int_add(current, written, "next")?;
    let body_end = builder
        .get_insert_block()
        .ok_or_else(|| LlvmAdapterError::Build("abort write 缺少 block".to_owned()))?;
    offset.add_incoming(&[(&next, body_end)]);
    builder.build_conditional_branch(progressed, header, done)?;

    builder.position_at_end(done);
    builder.build_return(None)?;
    Ok(function)
}

/// `koven.abort.write_i64(i64)`: writes the signed decimal text to stderr.
fn define_write_decimal<'ctx>(
    context: &'ctx Context,
    llvm: &LlvmModule<'ctx>,
    size_type: IntType<'ctx>,
    write_bytes: FunctionValue<'ctx>,
) -> Result<FunctionValue<'ctx>, LlvmAdapterError> {
    let i64_type = context.i64_type();
    let i8_type = context.i8_type();
    let function = llvm.add_function(
        "koven.abort.write_i64",
        context.void_type().fn_type(&[i64_type.into()], false),
        Some(Linkage::Internal),
    );
    let value = function.get_nth_param(0).expect("value").into_int_value();
    let builder = context.create_builder();
    let entry = context.append_basic_block(function, "entry");
    let digits = context.append_basic_block(function, "digit");
    let sign = context.append_basic_block(function, "sign");
    let emit = context.append_basic_block(function, "emit");

    builder.position_at_end(entry);
    let buffer_type = i8_type.array_type(DECIMAL_BUFFER);
    let buffer = builder.build_alloca(buffer_type, "buffer")?;
    let negative =
        builder.build_int_compare(IntPredicate::SLT, value, i64_type.const_zero(), "negative")?;
    // Two's-complement negation yields the unsigned magnitude, including for i64::MIN.
    let negated = builder.build_int_sub(i64_type.const_zero(), value, "negated")?;
    let magnitude = builder
        .build_select(negative, negated, value, "magnitude")?
        .into_int_value();
    builder.build_unconditional_branch(digits)?;

    builder.position_at_end(digits);
    let position = builder.build_phi(i64_type, "position")?;
    let remaining = builder.build_phi(i64_type, "remaining")?;
    position.add_incoming(&[(&i64_type.const_int(u64::from(DECIMAL_BUFFER), false), entry)]);
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
    let after_digits =
        builder.build_and(builder.build_not(more, "done")?, negative, "needs.sign")?;
    let finished = context.append_basic_block(function, "finished");
    builder.build_conditional_branch(more, digits, finished)?;

    builder.position_at_end(finished);
    builder.build_conditional_branch(after_digits, sign, emit)?;

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
        i64_type.const_int(u64::from(DECIMAL_BUFFER), false),
        start,
        "length",
    )?;
    let length = builder.build_int_cast(length, size_type, "length.size")?;
    call(&builder, write_bytes, &[text.into(), length.into()])?;
    builder.build_return(None)?;
    Ok(function)
}

fn write_text<'ctx>(
    builder: &Builder<'ctx>,
    size_type: IntType<'ctx>,
    write_bytes: FunctionValue<'ctx>,
    text: &str,
    name: &str,
) -> Result<(), LlvmAdapterError> {
    let global = builder.build_global_string_ptr(text, name)?;
    global.set_linkage(Linkage::Private);
    let length = u64::try_from(text.len())
        .map_err(|_| LlvmAdapterError::Build("abort text 长度超出 u64".to_owned()))?;
    call(
        builder,
        write_bytes,
        &[
            global.as_pointer_value().into(),
            size_type.const_int(length, false).into(),
        ],
    )
}

fn call<'ctx>(
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    arguments: &[BasicMetadataValueEnum<'ctx>],
) -> Result<(), LlvmAdapterError> {
    builder.build_call(function, arguments, "")?;
    Ok(())
}
