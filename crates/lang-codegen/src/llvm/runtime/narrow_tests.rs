use super::*;

#[test]
fn narrow_target_checks_physical_buffer_and_logical_length_bridge() {
    let context = Context::create();
    let llvm = context.create_module("narrow-container-buffer");
    let target = TargetData::create("e-p:16:16:16:16");
    let index_bits = pointer_index_bits(&target).expect("pointer index width");
    llvm.set_data_layout(&target.get_data_layout());
    let builder = context.create_builder();
    let pointer = context.ptr_type(AddressSpace::default());
    let size_type = context.i16_type();
    let function = llvm.add_function("allocate", pointer.fn_type(&[], false), None);
    let entry = context.append_basic_block(function, "entry");
    builder.position_at_end(entry);
    let malloc = llvm.add_function("malloc", pointer.fn_type(&[size_type.into()], false), None);
    let abort = llvm.add_function("abort", context.void_type().fn_type(&[], false), None);
    let sentinel = llvm.add_global(context.i8_type(), None, "sentinel");
    sentinel.set_initializer(&context.i8_type().const_zero());
    let runtime = RuntimeAbi {
        context: &context,
        size_type,
        max_buffer_bytes: (1_u64 << (index_bits - 1)) - 1,
        malloc: Some(malloc),
        abort: Some(abort),
        write: None,
        free: None,
        allocation_sizes: BTreeMap::new(),
        shared_allocation_sizes: BTreeMap::new(),
        zst_sentinel: Some(sentinel.as_pointer_value()),
        drop_functions: BTreeMap::new(),
    };
    let buffer = runtime
        .allocate_buffer(
            &llvm,
            &builder,
            function,
            size_type.const_int(1 << 15, false),
            1,
            "buffer",
        )
        .expect("buffer guard lowers");
    builder.build_return(Some(&buffer)).expect("return buffer");

    let zst_function = llvm.add_function("allocate_zst", pointer.fn_type(&[], false), None);
    let zst_entry = context.append_basic_block(zst_function, "entry");
    builder.position_at_end(zst_entry);
    let zst = runtime
        .allocate_buffer(
            &llvm,
            &builder,
            zst_function,
            size_type.const_int(1 << 15, false),
            0,
            "zst",
        )
        .expect("ZST logical length needs no physical allocation");
    builder.build_return(Some(&zst)).expect("return sentinel");

    let bridge = llvm.add_function(
        "bridge_length",
        size_type.fn_type(&[context.i32_type().into()], false),
        None,
    );
    let bridge_entry = context.append_basic_block(bridge, "entry");
    builder.position_at_end(bridge_entry);
    let input = bridge
        .get_first_param()
        .expect("length input")
        .into_int_value();
    input.set_name("length");
    let converted = runtime
        .container_int_to_size(&builder, bridge, input, "length")
        .expect("length bridge lowers");
    builder
        .build_return(Some(&converted))
        .expect("return length");

    let header = context.struct_type(&[pointer.into(), size_type.into()], false);
    let read = llvm.add_function(
        "read_length",
        context.i32_type().fn_type(&[header.into()], false),
        None,
    );
    let read_entry = context.append_basic_block(read, "entry");
    builder.position_at_end(read_entry);
    let owner = read
        .get_first_param()
        .expect("container header")
        .into_struct_value();
    let read_length =
        super::super::container::length(&builder, owner, context.i32_type(), "length")
            .expect("header bridge lowers");
    builder
        .build_return(Some(&read_length))
        .expect("return logical length");
    llvm.verify().expect("narrow-size LLVM verifies");
    let ir = llvm.print_to_string().to_string();
    assert_eq!(index_bits, 16);
    assert!(ir.contains("icmp ugt i16 %buffer.bytes, 32767"), "{ir}");
    assert!(
        ir.contains("br i1 %buffer.too_large, label %buffer.abort"),
        "{ir}"
    );
    assert!(ir.contains("llvm.umul.with.overflow.i16"), "{ir}");
    assert!(ir.contains("icmp slt i32 %length, 0"), "{ir}");
    assert!(ir.contains("icmp ugt i32 %length, 65535"), "{ir}");
    assert!(ir.contains("trunc i32 %length to i16"), "{ir}");
    assert!(ir.contains("zext i16"), "{ir}");
}

#[test]
fn pointer_index_width_uses_data_layout_index_field() {
    let target = TargetData::create("e-p:64:64:64:32");
    assert_eq!(pointer_index_bits(&target).unwrap(), 32);

    let context = Context::create();
    let llvm = context.create_module("narrow-pointer-index");
    llvm.set_data_layout(&target.get_data_layout());
    let mut program = crate::ssa::model::Program::default();
    let module_id = program.add_module("narrow-pointer-index");
    let module = program.module(module_id).expect("module exists");
    let types = TypeMap::lower(&context, module, &target).expect("type map lowers");
    let runtime =
        RuntimeAbi::lower(&context, &llvm, module, &types, &target, None).expect("runtime lowers");
    assert_eq!(runtime.max_buffer_bytes, i32::MAX as u64);
}
