//! Explicit Koven entry validation and C ABI process wrapper.

use inkwell::{
    AddressSpace, IntPredicate,
    context::Context,
    module::{Linkage, Module as LlvmModule},
    values::{BasicMetadataValueEnum, FunctionValue, IntValue, ValueKind},
};

use crate::ssa::model::{
    EntityId, EntityType, FunctionId, LoanKind, Module, SequentialContainerKind, SsaTypeId,
    SsaTypeKind,
};

use super::{LlvmAdapterError, container, runtime::RuntimeAbi, string, type_map::TypeMap};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeEntryPlan {
    NoArguments {
        function: FunctionId,
    },
    BorrowedArguments {
        function: FunctionId,
        arguments: SsaTypeId,
        string: SsaTypeId,
    },
}

impl NativeEntryPlan {
    pub(crate) const fn function(self) -> FunctionId {
        match self {
            Self::NoArguments { function } | Self::BorrowedArguments { function, .. } => function,
        }
    }
}

impl From<FunctionId> for NativeEntryPlan {
    fn from(function: FunctionId) -> Self {
        Self::NoArguments { function }
    }
}

pub(super) fn validate(module: &Module, plan: NativeEntryPlan) -> Result<(), LlvmAdapterError> {
    let entry = plan.function();
    if entry.module() != module.id {
        return Err(LlvmAdapterError::InvalidEntry(
            "native entry belongs to another SSA module".to_owned(),
        ));
    }
    let function = module.function(entry).ok_or_else(|| {
        LlvmAdapterError::InvalidEntry("native entry function does not exist".to_owned())
    })?;
    let entry_block = function.blocks.first().ok_or_else(|| {
        LlvmAdapterError::InvalidEntry("native entry function has no entry block".to_owned())
    })?;
    if !function.return_types.is_empty() {
        return Err(LlvmAdapterError::InvalidEntry(
            "native entry must return Koven Unit".to_owned(),
        ));
    }
    match plan {
        NativeEntryPlan::NoArguments { .. } if entry_block.parameters.is_empty() => {}
        NativeEntryPlan::NoArguments { .. } => {
            return Err(LlvmAdapterError::InvalidEntry(
                "native entry must not have parameters".to_owned(),
            ));
        }
        NativeEntryPlan::BorrowedArguments {
            arguments, string, ..
        } => {
            let parameter = match entry_block.parameters.as_slice() {
                [parameter @ EntityId::Loan(_)] => *parameter,
                _ => return Err(invalid_borrowed_arguments()),
            };
            if module.type_kind(string) != Some(&SsaTypeKind::StringOwner)
                || module.sequential_container(arguments)
                    != Some((SequentialContainerKind::Array, string))
                || function.entity(parameter).map(|entity| entity.ty)
                    != Some(EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: arguments,
                    })
            {
                return Err(invalid_borrowed_arguments());
            }
        }
    }
    Ok(())
}

fn invalid_borrowed_arguments() -> LlvmAdapterError {
    LlvmAdapterError::InvalidEntry(
        "parameterized native entry must borrow exactly Array<String> and return Unit".to_owned(),
    )
}

pub(super) fn define_wrapper<'ctx>(
    context: &'ctx Context,
    module: &LlvmModule<'ctx>,
    target: FunctionValue<'ctx>,
    plan: NativeEntryPlan,
    types: &TypeMap<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
) -> Result<(), LlvmAdapterError> {
    if module.get_function("main").is_some() {
        return Err(LlvmAdapterError::InvalidEntry(
            "LLVM module already declares native main".to_owned(),
        ));
    }
    let wrapper_type = match plan {
        NativeEntryPlan::NoArguments { .. } => context.i32_type().fn_type(&[], false),
        NativeEntryPlan::BorrowedArguments { .. } => context.i32_type().fn_type(
            &[
                context.i32_type().into(),
                context.ptr_type(inkwell::AddressSpace::default()).into(),
            ],
            false,
        ),
    };
    let wrapper = module.add_function("main", wrapper_type, None);
    match plan {
        NativeEntryPlan::NoArguments { .. } => {
            let block = context.append_basic_block(wrapper, "entry");
            let builder = context.create_builder();
            builder.position_at_end(block);
            builder.build_call(target, &[], "")?;
            builder.build_return(Some(&context.i32_type().const_zero()))?;
        }
        NativeEntryPlan::BorrowedArguments {
            arguments, string, ..
        } => {
            define_borrowed_arguments_wrapper(
                context, module, wrapper, target, arguments, string, types, runtime,
            )?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn define_borrowed_arguments_wrapper<'ctx>(
    context: &'ctx Context,
    module: &LlvmModule<'ctx>,
    wrapper: FunctionValue<'ctx>,
    target: FunctionValue<'ctx>,
    arguments: SsaTypeId,
    string_type: SsaTypeId,
    types: &TypeMap<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
) -> Result<(), LlvmAdapterError> {
    let builder = context.create_builder();
    let entry = context.append_basic_block(wrapper, "entry");
    let preflight_header = context.append_basic_block(wrapper, "argv.preflight");
    let preflight_body = context.append_basic_block(wrapper, "argv.preflight.body");
    let allocate = context.append_basic_block(wrapper, "argv.allocate");
    let construct_header = context.append_basic_block(wrapper, "argv.construct");
    let construct_body = context.append_basic_block(wrapper, "argv.construct.body");
    let construct_empty = context.append_basic_block(wrapper, "argv.construct.empty");
    let construct_copy = context.append_basic_block(wrapper, "argv.construct.copy");
    let construct_store = context.append_basic_block(wrapper, "argv.construct.store");
    let call = context.append_basic_block(wrapper, "argv.call");
    let failure = context.append_basic_block(wrapper, "argv.invalid");

    let argc = wrapper
        .get_nth_param(0)
        .expect("validated wrapper argc")
        .into_int_value();
    let argv = wrapper
        .get_nth_param(1)
        .expect("validated wrapper argv")
        .into_pointer_value();
    let zero32 = context.i32_type().const_zero();
    let one32 = context.i32_type().const_int(1, false);
    let pointer = context.ptr_type(AddressSpace::default());
    let size = runtime.size_type();
    let strlen = module.get_function("strlen").unwrap_or_else(|| {
        module.add_function("strlen", size.fn_type(&[pointer.into()], false), None)
    });
    let utf8 = define_utf8_validator(context, module, size)?;

    builder.position_at_end(entry);
    let negative = builder.build_int_compare(IntPredicate::SLT, argc, zero32, "argc.negative")?;
    let has_arguments =
        builder.build_int_compare(IntPredicate::SGT, argc, one32, "argc.has_args")?;
    let argv_null = builder.build_is_null(argv, "argv.null")?;
    let missing_argv = builder.build_and(has_arguments, argv_null, "argv.missing")?;
    let invalid = builder.build_or(negative, missing_argv, "argv.invalid")?;
    builder.build_conditional_branch(invalid, failure, preflight_header)?;

    builder.position_at_end(preflight_header);
    let preflight_index = builder.build_phi(context.i32_type(), "argv.preflight.index")?;
    preflight_index.add_incoming(&[(&one32, entry)]);
    let index = preflight_index.as_basic_value().into_int_value();
    let remains =
        builder.build_int_compare(IntPredicate::SLT, index, argc, "argv.preflight.remains")?;
    builder.build_conditional_branch(remains, preflight_body, allocate)?;

    builder.position_at_end(preflight_body);
    let item = load_argv_item(context, &builder, argv, index, "argv.preflight.item")?;
    let item_null = builder.build_is_null(item, "argv.item.null")?;
    let item_valid = context.append_basic_block(wrapper, "argv.preflight.item_valid");
    builder.build_conditional_branch(item_null, failure, item_valid)?;
    builder.position_at_end(item_valid);
    let length = call_int(&builder, strlen, &[item.into()], "argv.preflight.length")?;
    let valid = call_int(
        &builder,
        utf8,
        &[item.into(), length.into()],
        "argv.preflight.utf8",
    )?;
    let advance = context.append_basic_block(wrapper, "argv.preflight.advance");
    builder.build_conditional_branch(valid, advance, failure)?;
    builder.position_at_end(advance);
    let next = builder.build_int_add(index, one32, "argv.preflight.next")?;
    builder.build_unconditional_branch(preflight_header)?;
    preflight_index.add_incoming(&[(&next, advance)]);

    builder.position_at_end(allocate);
    let count32 = builder.build_int_sub(argc, one32, "argv.count.raw")?;
    let nonnegative_count = builder
        .build_select(has_arguments, count32, zero32, "argv.count")?
        .into_int_value();
    let count = builder.build_int_z_extend(nonnegative_count, size, "argv.count.size")?;
    let layout = types.container_layout(arguments)?;
    let buffer = runtime.allocate_buffer(
        module,
        &builder,
        wrapper,
        count,
        layout.stride,
        "argv.array",
    )?;
    let allocated = builder.get_insert_block().ok_or_else(|| {
        LlvmAdapterError::Build("argv Array allocation 缺少 ready block".to_owned())
    })?;
    builder.build_unconditional_branch(construct_header)?;
    builder.position_at_end(construct_header);
    let construct_index = builder.build_phi(context.i32_type(), "argv.construct.index")?;
    construct_index.add_incoming(&[(&one32, allocated)]);
    let current = construct_index.as_basic_value().into_int_value();
    let construct_remains =
        builder.build_int_compare(IntPredicate::SLT, current, argc, "argv.construct.remains")?;
    builder.build_conditional_branch(construct_remains, construct_body, call)?;

    builder.position_at_end(construct_body);
    let source = load_argv_item(context, &builder, argv, current, "argv.source")?;
    let length = call_int(&builder, strlen, &[source.into()], "argv.length")?;
    let empty =
        builder.build_int_compare(IntPredicate::EQ, length, size.const_zero(), "argv.empty")?;
    builder.build_conditional_branch(empty, construct_empty, construct_copy)?;

    let llvm_string = types.basic_type(string_type)?.into_struct_type();
    builder.position_at_end(construct_empty);
    let empty_owner = string::build_owner(
        &builder,
        llvm_string,
        pointer.const_null(),
        length,
        size.const_zero(),
        "argv.string.empty",
    )?;
    builder.build_unconditional_branch(construct_store)?;
    let empty_block = builder.get_insert_block().expect("empty block");

    builder.position_at_end(construct_copy);
    let allocation = runtime.allocate_string_bytes(&builder, wrapper, length, "argv.string")?;
    builder.build_memcpy(allocation, 1, source, 1, length)?;
    let owned = string::build_owner(
        &builder,
        llvm_string,
        allocation,
        length,
        length,
        "argv.string",
    )?;
    builder.build_unconditional_branch(construct_store)?;
    let copy_block = builder.get_insert_block().expect("copy block");

    builder.position_at_end(construct_store);
    let owner = builder.build_phi(llvm_string, "argv.string.owner")?;
    owner.add_incoming(&[(&empty_owner, empty_block), (&owned, copy_block)]);
    let element_index = builder.build_int_sub(current, one32, "argv.element.index")?;
    // SAFETY: current ranges from 1 through argc-1, while buffer was allocated for argc-1
    // elements, so subtracting one yields an in-bounds destination in source order.
    let slot = unsafe {
        builder.build_in_bounds_gep(
            layout.element,
            buffer,
            &[builder.build_int_z_extend(element_index, size, "argv.element.size")?],
            "argv.element",
        )?
    };
    builder.build_store(slot, owner.as_basic_value())?;
    let next = builder.build_int_add(current, one32, "argv.construct.next")?;
    builder.build_unconditional_branch(construct_header)?;
    construct_index.add_incoming(&[(&next, construct_store)]);

    builder.position_at_end(call);
    let array = container::build_header(&builder, layout.header, buffer, count, "argv.owner")?;
    let owner_slot = builder.build_alloca(layout.header, "argv.owner.slot")?;
    builder.build_store(owner_slot, array)?;
    builder.build_call(target, &[BasicMetadataValueEnum::from(owner_slot)], "")?;
    runtime.emit_drop(&builder, arguments, array.into())?;
    builder.build_return(Some(&zero32))?;

    builder.position_at_end(failure);
    builder.build_return(Some(&one32))?;
    Ok(())
}

fn load_argv_item<'ctx>(
    context: &'ctx Context,
    builder: &inkwell::builder::Builder<'ctx>,
    argv: inkwell::values::PointerValue<'ctx>,
    index: IntValue<'ctx>,
    name: &str,
) -> Result<inkwell::values::PointerValue<'ctx>, LlvmAdapterError> {
    // SAFETY: the process ABI guarantees argv has argc entries; callers dominate this access with
    // 1 <= index < argc and reject a null argv whenever such an entry exists.
    let slot = unsafe {
        builder.build_in_bounds_gep(
            context.ptr_type(AddressSpace::default()),
            argv,
            &[index],
            name,
        )?
    };
    Ok(builder
        .build_load(context.ptr_type(AddressSpace::default()), slot, name)?
        .into_pointer_value())
}

fn call_int<'ctx>(
    builder: &inkwell::builder::Builder<'ctx>,
    function: FunctionValue<'ctx>,
    arguments: &[BasicMetadataValueEnum<'ctx>],
    name: &str,
) -> Result<IntValue<'ctx>, LlvmAdapterError> {
    match builder
        .build_call(function, arguments, name)?
        .try_as_basic_value()
    {
        ValueKind::Basic(value) => Ok(value.into_int_value()),
        ValueKind::Instruction(_) => Err(LlvmAdapterError::Build(format!(
            "{name} did not return an integer"
        ))),
    }
}

fn define_utf8_validator<'ctx>(
    context: &'ctx Context,
    module: &LlvmModule<'ctx>,
    size: inkwell::types::IntType<'ctx>,
) -> Result<FunctionValue<'ctx>, LlvmAdapterError> {
    if let Some(function) = module.get_function("koven.entry.valid_utf8") {
        return Ok(function);
    }
    let pointer = context.ptr_type(AddressSpace::default());
    let function = module.add_function(
        "koven.entry.valid_utf8",
        context
            .bool_type()
            .fn_type(&[pointer.into(), size.into()], false),
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
    let remaining_phi = builder.build_phi(context.i8_type(), "remaining")?;
    let minimum_phi = builder.build_phi(context.i8_type(), "minimum")?;
    let maximum_phi = builder.build_phi(context.i8_type(), "maximum")?;
    let zero_size = size.const_zero();
    let zero8 = context.i8_type().const_zero();
    let min_cont = context.i8_type().const_int(0x80, false);
    let max_cont = context.i8_type().const_int(0xbf, false);
    index_phi.add_incoming(&[(&zero_size, entry)]);
    remaining_phi.add_incoming(&[(&zero8, entry)]);
    minimum_phi.add_incoming(&[(&min_cont, entry)]);
    maximum_phi.add_incoming(&[(&max_cont, entry)]);
    let index = index_phi.as_basic_value().into_int_value();
    let remaining = remaining_phi.as_basic_value().into_int_value();
    let minimum = minimum_phi.as_basic_value().into_int_value();
    let maximum = maximum_phi.as_basic_value().into_int_value();
    let remains = builder.build_int_compare(IntPredicate::ULT, index, length, "remains")?;
    builder.build_conditional_branch(remains, body, done)?;
    builder.position_at_end(done);
    let complete = builder.build_int_compare(IntPredicate::EQ, remaining, zero8, "complete")?;
    builder.build_return(Some(&complete))?;

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
    builder.build_return(Some(&context.bool_type().const_zero()))?;

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
    let next_index = builder.build_int_add(index, size.const_int(1, false), "next.index")?;
    builder.build_unconditional_branch(header)?;
    index_phi.add_incoming(&[(&next_index, advance)]);
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
