//! UTF-8 String owner 的 LLVM 值布局与字节级 primitive lowering。

use inkwell::{
    IntPredicate,
    builder::Builder,
    context::Context,
    module::{Linkage, Module as LlvmModule},
    values::{FunctionValue, IntValue, PointerValue, StructValue},
};

use crate::ssa::model::SsaTypeId;
use lang_frontend::type_checking::MAX_STRING_BYTE_LENGTH;

use super::{LlvmAdapterError, runtime::RuntimeAbi, type_map::TypeMap};

#[derive(Clone, Copy)]
pub(super) struct StringView<'ctx> {
    bytes: PointerValue<'ctx>,
    length: IntValue<'ctx>,
}

impl<'ctx> StringView<'ctx> {
    /// The caller has proved readable, valid UTF-8 storage within the String length limit.
    pub(super) const fn from_valid_utf8(bytes: PointerValue<'ctx>, length: IntValue<'ctx>) -> Self {
        Self { bytes, length }
    }

    pub(super) const fn bytes(self) -> PointerValue<'ctx> {
        self.bytes
    }

    pub(super) const fn length(self) -> IntValue<'ctx> {
        self.length
    }
}

pub(super) fn view<'ctx>(
    builder: &Builder<'ctx>,
    value: StructValue<'ctx>,
    name: &str,
) -> Result<StringView<'ctx>, LlvmAdapterError> {
    Ok(StringView {
        bytes: builder
            .build_extract_value(value, 0, &format!("{name}.bytes"))?
            .into_pointer_value(),
        length: builder
            .build_extract_value(value, 1, &format!("{name}.length"))?
            .into_int_value(),
    })
}

pub(super) fn literal<'ctx>(
    llvm: &LlvmModule<'ctx>,
    builder: &Builder<'ctx>,
    types: &TypeMap<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    string: SsaTypeId,
    bytes: &[u8],
    name: &str,
) -> Result<StructValue<'ctx>, LlvmAdapterError> {
    let string_type = types.basic_type(string)?.into_struct_type();
    let pointer = if bytes.is_empty() {
        runtime.empty_string_bytes()?
    } else {
        let constant = llvm.get_context().const_string(bytes, false);
        let global = llvm.add_global(constant.get_type(), None, &format!("{name}.bytes"));
        global.set_linkage(Linkage::Private);
        global.set_constant(true);
        global.set_initializer(&constant);
        global.as_pointer_value()
    };
    let length = u64::try_from(bytes.len())
        .map_err(|_| LlvmAdapterError::Build("String literal 长度超出 u64".to_owned()))?;
    build_owner(
        builder,
        string_type,
        pointer,
        runtime.size_type().const_int(length, false),
        runtime.size_type().const_zero(),
        name,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn concat<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    string_type: inkwell::types::StructType<'ctx>,
    left: StringView<'ctx>,
    right: StringView<'ctx>,
    name: &str,
) -> Result<StructValue<'ctx>, LlvmAdapterError> {
    let length = builder.build_int_add(left.length, right.length, &format!("{name}.length"))?;
    let overflow = builder.build_int_compare(
        IntPredicate::ULT,
        length,
        left.length,
        &format!("{name}.overflow"),
    )?;
    let length_exceeded = builder.build_int_compare(
        IntPredicate::UGT,
        length,
        runtime.size_type().const_int(MAX_STRING_BYTE_LENGTH, false),
        &format!("{name}.length_exceeded"),
    )?;
    let invalid_length =
        builder.build_or(overflow, length_exceeded, &format!("{name}.invalid_length"))?;
    runtime.abort_if(
        builder,
        function,
        invalid_length,
        name,
        super::runtime::AbortMessage::Text("size overflow"),
    )?;

    let before_empty = builder
        .get_insert_block()
        .ok_or_else(|| LlvmAdapterError::Build("String concat 缺少 preheader".to_owned()))?;
    let empty = context.append_basic_block(function, &format!("{name}.empty"));
    let allocate = context.append_basic_block(function, &format!("{name}.allocate"));
    let ready = context.append_basic_block(function, &format!("{name}.ready"));
    let is_empty = builder.build_int_compare(
        IntPredicate::EQ,
        length,
        runtime.size_type().const_zero(),
        &format!("{name}.is_empty"),
    )?;
    builder.build_conditional_branch(is_empty, empty, allocate)?;

    builder.position_at_end(empty);
    let empty_value = build_owner(
        builder,
        string_type,
        runtime.empty_string_bytes()?,
        length,
        runtime.size_type().const_zero(),
        &format!("{name}.empty_value"),
    )?;
    let empty_block = builder.get_insert_block().unwrap_or(before_empty);
    builder.build_unconditional_branch(ready)?;

    builder.position_at_end(allocate);
    let allocation = runtime.allocate_string_bytes(builder, function, length, name)?;
    builder.build_memcpy(allocation, 1, left.bytes, 1, left.length)?;
    // SAFETY: allocation has checked `left.length + right.length` bytes, so offset left.length
    // starts the remaining right.length-byte suffix.
    let suffix = unsafe {
        builder.build_in_bounds_gep(
            context.i8_type(),
            allocation,
            &[left.length],
            &format!("{name}.suffix"),
        )?
    };
    builder.build_memcpy(suffix, 1, right.bytes, 1, right.length)?;
    let allocated_value = build_owner(
        builder,
        string_type,
        allocation,
        length,
        length,
        &format!("{name}.allocated_value"),
    )?;
    let allocated_block = builder
        .get_insert_block()
        .ok_or_else(|| LlvmAdapterError::Build("String concat 缺少 allocated block".to_owned()))?;
    builder.build_unconditional_branch(ready)?;

    builder.position_at_end(ready);
    let result = builder.build_phi(string_type, name)?;
    result.add_incoming(&[
        (&empty_value, empty_block),
        (&allocated_value, allocated_block),
    ]);
    Ok(result.as_basic_value().into_struct_value())
}

/// Clone always copies non-empty bytes, even when the source uses static storage.
pub(super) fn clone_owner<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    string_type: inkwell::types::StructType<'ctx>,
    source: StringView<'ctx>,
    name: &str,
) -> Result<StructValue<'ctx>, LlvmAdapterError> {
    let empty = context.append_basic_block(function, &format!("{name}.clone.empty"));
    let allocate = context.append_basic_block(function, &format!("{name}.clone.allocate"));
    let ready = context.append_basic_block(function, &format!("{name}.clone.ready"));
    let is_empty = builder.build_int_compare(
        IntPredicate::EQ,
        source.length,
        runtime.size_type().const_zero(),
        &format!("{name}.clone.is_empty"),
    )?;
    builder.build_conditional_branch(is_empty, empty, allocate)?;

    builder.position_at_end(empty);
    let empty_value = build_owner(
        builder,
        string_type,
        runtime.empty_string_bytes()?,
        runtime.size_type().const_zero(),
        runtime.size_type().const_zero(),
        &format!("{name}.clone.empty_value"),
    )?;
    builder.build_unconditional_branch(ready)?;

    builder.position_at_end(allocate);
    let allocation = runtime.allocate_string_bytes(builder, function, source.length, name)?;
    builder.build_memcpy(allocation, 1, source.bytes, 1, source.length)?;
    let allocated_value = build_owner(
        builder,
        string_type,
        allocation,
        source.length,
        source.length,
        &format!("{name}.clone.allocated_value"),
    )?;
    let allocated_block = builder
        .get_insert_block()
        .ok_or_else(|| LlvmAdapterError::Build("String clone 缺少 allocated block".to_owned()))?;
    builder.build_unconditional_branch(ready)?;

    builder.position_at_end(ready);
    let result = builder.build_phi(string_type, name)?;
    result.add_incoming(&[(&empty_value, empty), (&allocated_value, allocated_block)]);
    Ok(result.as_basic_value().into_struct_value())
}

pub(super) fn equal<'ctx>(
    context: &'ctx Context,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    left: StringView<'ctx>,
    right: StringView<'ctx>,
    name: &str,
) -> Result<IntValue<'ctx>, LlvmAdapterError> {
    let preheader = builder
        .get_insert_block()
        .ok_or_else(|| LlvmAdapterError::Build("String equality 缺少 preheader".to_owned()))?;
    let compare = context.append_basic_block(function, &format!("{name}.compare"));
    let body = context.append_basic_block(function, &format!("{name}.body"));
    let advance = context.append_basic_block(function, &format!("{name}.advance"));
    let equal = context.append_basic_block(function, &format!("{name}.equal"));
    let mismatch = context.append_basic_block(function, &format!("{name}.mismatch"));
    let done = context.append_basic_block(function, &format!("{name}.done"));
    let same_length = builder.build_int_compare(
        IntPredicate::EQ,
        left.length,
        right.length,
        &format!("{name}.same_length"),
    )?;
    builder.build_conditional_branch(same_length, compare, done)?;

    builder.position_at_end(compare);
    let index = builder.build_phi(runtime.size_type(), &format!("{name}.index"))?;
    index.add_incoming(&[(&runtime.size_type().const_zero(), preheader)]);
    let current = index.as_basic_value().into_int_value();
    let remains = builder.build_int_compare(
        IntPredicate::ULT,
        current,
        left.length,
        &format!("{name}.remains"),
    )?;
    builder.build_conditional_branch(remains, body, equal)?;

    builder.position_at_end(body);
    // SAFETY: the body is dominated by index < the equal left/right lengths.
    let left_byte = unsafe {
        builder.build_in_bounds_gep(context.i8_type(), left.bytes, &[current], "left.byte")?
    };
    // SAFETY: the same checked index is in bounds for the equal right length.
    let right_byte = unsafe {
        builder.build_in_bounds_gep(context.i8_type(), right.bytes, &[current], "right.byte")?
    };
    let left_byte = builder
        .build_load(context.i8_type(), left_byte, "left.value")?
        .into_int_value();
    let right_byte = builder
        .build_load(context.i8_type(), right_byte, "right.value")?
        .into_int_value();
    let same_byte = builder.build_int_compare(
        IntPredicate::EQ,
        left_byte,
        right_byte,
        &format!("{name}.same_byte"),
    )?;
    builder.build_conditional_branch(same_byte, advance, mismatch)?;

    builder.position_at_end(advance);
    let next = builder.build_int_add(
        current,
        runtime.size_type().const_int(1, false),
        &format!("{name}.next"),
    )?;
    let backedge = builder
        .get_insert_block()
        .ok_or_else(|| LlvmAdapterError::Build("String equality 缺少 backedge".to_owned()))?;
    builder.build_unconditional_branch(compare)?;
    index.add_incoming(&[(&next, backedge)]);

    builder.position_at_end(equal);
    builder.build_unconditional_branch(done)?;
    builder.position_at_end(mismatch);
    builder.build_unconditional_branch(done)?;
    builder.position_at_end(done);
    let result = builder.build_phi(context.bool_type(), name)?;
    let false_value = context.bool_type().const_zero();
    let true_value = context.bool_type().const_int(1, false);
    result.add_incoming(&[
        (&false_value, preheader),
        (&true_value, equal),
        (&false_value, mismatch),
    ]);
    Ok(result.as_basic_value().into_int_value())
}

pub(super) fn print<'ctx>(
    llvm: &LlvmModule<'ctx>,
    builder: &Builder<'ctx>,
    function: FunctionValue<'ctx>,
    runtime: &RuntimeAbi<'ctx>,
    value: StringView<'ctx>,
    name: &str,
) -> Result<(), LlvmAdapterError> {
    runtime.emit_write_bytes(builder, function, value.bytes, value.length, name)?;
    runtime.emit_print_literal(llvm, builder, function, b"\n", &format!("{name}.newline"))
}

pub(super) fn build_owner<'ctx>(
    builder: &Builder<'ctx>,
    ty: inkwell::types::StructType<'ctx>,
    bytes: PointerValue<'ctx>,
    length: IntValue<'ctx>,
    capacity: IntValue<'ctx>,
    name: &str,
) -> Result<StructValue<'ctx>, LlvmAdapterError> {
    let value = builder
        .build_insert_value(ty.const_zero(), bytes, 0, &format!("{name}.with_bytes"))?
        .into_struct_value();
    let value = builder
        .build_insert_value(value, length, 1, &format!("{name}.with_length"))?
        .into_struct_value();
    Ok(builder
        .build_insert_value(value, capacity, 2, &format!("{name}.with_capacity"))?
        .into_struct_value())
}

pub(crate) mod entry {
    //! Explicit Koven entry validation and C ABI process wrapper.

    use inkwell::{
        AddressSpace, IntPredicate,
        context::Context,
        module::Module as LlvmModule,
        values::{BasicMetadataValueEnum, FunctionValue, IntValue, ValueKind},
    };

    use crate::ssa::model::{
        EntityId, EntityType, FunctionId, LoanKind, Module, SequentialContainerKind, SsaTypeId,
        SsaTypeKind,
    };

    use super::super::{
        LlvmAdapterError, container, runtime::RuntimeAbi, string, type_map::TypeMap,
    };

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
                Self::NoArguments { function } | Self::BorrowedArguments { function, .. } => {
                    function
                }
            }
        }
    }

    impl From<FunctionId> for NativeEntryPlan {
        fn from(function: FunctionId) -> Self {
            Self::NoArguments { function }
        }
    }

    pub(crate) fn validate(module: &Module, plan: NativeEntryPlan) -> Result<(), LlvmAdapterError> {
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
        if function.never_returns || !function.return_types.is_empty() {
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
            "parameterized native entry must borrow exactly Array<String> and return Unit"
                .to_owned(),
        )
    }

    pub(in crate::llvm) fn define_wrapper<'ctx>(
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
        let utf8 = super::super::utf8::define_error_offset(context, module, size)?;

        builder.position_at_end(entry);
        let negative =
            builder.build_int_compare(IntPredicate::SLT, argc, zero32, "argc.negative")?;
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
        // Reject before UTF-8 traversal or allocation: argv failure is operational, not Abort.
        let length_exceeded = builder.build_int_compare(
            IntPredicate::UGT,
            length,
            size.const_int(lang_frontend::type_checking::MAX_STRING_BYTE_LENGTH, false),
            "argv.length_exceeded",
        )?;
        let length_valid = context.append_basic_block(wrapper, "argv.preflight.length_valid");
        builder.build_conditional_branch(length_exceeded, failure, length_valid)?;
        builder.position_at_end(length_valid);
        let error_offset = call_int(
            &builder,
            utf8,
            &[item.into(), length.into()],
            "argv.preflight.utf8",
        )?;
        let valid = builder.build_int_compare(
            IntPredicate::EQ,
            error_offset,
            size.const_all_ones(),
            "argv.preflight.valid_utf8",
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
        let construct_remains = builder.build_int_compare(
            IntPredicate::SLT,
            current,
            argc,
            "argv.construct.remains",
        )?;
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
            runtime.empty_string_bytes()?,
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
}
