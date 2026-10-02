use lang_frontend::source::SourceMap;

use crate::ssa::{
    model::{
        BlockId, EntityId, EntityType, Function, Operation, Origin, Program,
        SequentialContainerKind, SsaTypeId, SsaTypeKind, TerminatorKind, ValueId,
    },
    verify::verify_program,
};

use super::render_verified_program;

fn origin() -> Origin {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("unit-storage.ko", "fun make(): Unit {}")
        .expect("test source must be unique");
    Origin::Source(sources.span(source, 0, 3).expect("test span must be valid"))
}

fn append_value(
    function: &mut Function,
    block: BlockId,
    operation: Operation,
    ty: SsaTypeId,
    origin: &Origin,
) -> ValueId {
    let results = function
        .append_instruction(
            block,
            operation,
            vec![EntityType::Value(ty)],
            origin.clone(),
        )
        .expect("test instruction must append")
        .1;
    let [EntityId::Value(value)] = results.as_slice() else {
        panic!("expected one value result, got {results:?}");
    };
    *value
}

#[test]
fn empty_unit_container_verified_ssa_lowers_to_llvm() {
    for kind in [
        SequentialContainerKind::Array,
        SequentialContainerKind::List,
        SequentialContainerKind::MutableList,
    ] {
        let origin = origin();
        let mut program = Program::default();
        let module_id = program.add_module("empty-unit-container");
        let module = program.module_mut(module_id).expect("module exists");
        let unit = module.intern_type(SsaTypeKind::Unit);
        let container = module
            .add_sequential_container_type(kind, unit)
            .expect("Unit is a concrete container element type");
        let make = module
            .add_function("make", vec![container], origin.clone())
            .expect("factory signature is valid");
        let function = module.function_mut(make).expect("factory exists");
        let entry = function.add_block(vec![], origin.clone()).expect("entry");
        let owner = append_value(
            function,
            entry,
            Operation::ContainerConstruct {
                container,
                elements: vec![],
            },
            container,
            &origin,
        );
        function
            .set_terminator(
                entry,
                TerminatorKind::Return {
                    values: vec![owner],
                },
                origin.clone(),
            )
            .expect("factory returns its owner");

        verify_program(&program)
            .expect("empty Unit container must verify independently of frontend");
        let llvm = render_verified_program(&program)
            .expect("empty Unit container storage must lower to verified LLVM");
        assert!(llvm.contains("ret %koven.container"), "{llvm}");
        assert!(!llvm.contains("call ptr @malloc"), "{llvm}");
        assert!(!llvm.contains("call void @free"), "{llvm}");
    }
}

#[test]
fn unit_storage_matches_target_datalayout_without_changing_container_headers() {
    use super::{layout::TargetLayoutPlan, native_target_machine, type_map::TypeMap};
    use inkwell::{AddressSpace, context::Context};

    let mut program = Program::default();
    let module_id = program.add_module("unit-layout");
    let module = program.module_mut(module_id).expect("module exists");
    let unit = module.intern_type(SsaTypeKind::Unit);
    let containers = [
        (SequentialContainerKind::Array, 2),
        (SequentialContainerKind::List, 2),
        (SequentialContainerKind::MutableList, 3),
    ]
    .map(|(kind, fields)| {
        (
            module
                .add_sequential_container_type(kind, unit)
                .expect("container"),
            fields,
        )
    });
    let context = Context::create();
    let target = native_target_machine()
        .expect("supported target")
        .1
        .get_target_data();
    let plan = TargetLayoutPlan::build(&context, module, &target).expect("Unit layout");
    let types = TypeMap::lower(&context, module, &target).expect("Unit container storage");
    let empty = context.struct_type(&[], false);
    let actual = types
        .container_element_type(unit)
        .expect("bounded Unit storage query");
    assert_eq!(actual, empty.into());
    assert_eq!(target.get_store_size(&actual), 0);
    assert_eq!(target.get_abi_size(&actual), 0);
    assert_eq!(plan.layout(unit).unwrap().size, target.get_abi_size(&empty));
    assert_eq!(
        plan.layout(unit).unwrap().alignment,
        target.get_abi_alignment(&empty)
    );
    assert!(matches!(
        types.basic_type(unit),
        Err(super::LlvmAdapterError::Unsupported(_))
    ));
    assert!(types.aggregate_type(unit).is_err());

    let pointer = context.ptr_type(AddressSpace::default());
    let size_type = context.ptr_sized_int_type(&target, None);
    for (container, fields) in containers {
        let layout = types.container_layout(container).expect("container layout");
        let mut expected_fields = vec![pointer.into(), size_type.into()];
        if fields == 3 {
            expected_fields.push(size_type.into());
        }
        let expected = context.struct_type(&expected_fields, false);
        assert_eq!(layout.header.get_field_types(), expected_fields);
        assert_eq!(layout.header.count_fields(), fields);
        assert_eq!(layout.element, actual);
        assert_eq!(layout.stride, 0);
        assert_eq!(layout.element_alignment, target.get_abi_alignment(&empty));
        assert_eq!(
            plan.layout(container).unwrap().size,
            target.get_abi_size(&expected)
        );
        assert_eq!(
            target.get_abi_size(&layout.header),
            target.get_abi_size(&expected)
        );
        assert_eq!(
            target.get_abi_alignment(&layout.header),
            target.get_abi_alignment(&expected)
        );
        for field in 0..fields {
            assert_eq!(
                target.offset_of_element(&layout.header, field),
                target.offset_of_element(&expected, field),
            );
        }
    }
}

#[test]
fn container_unit_storage_does_not_enable_general_value_callable_abi() {
    use super::{LlvmAdapterError, native_target_machine, type_map::TypeMap};
    use crate::ssa::model::CallableSignature;
    use inkwell::context::Context;

    let mut program = Program::default();
    let module_id = program.add_module("unit-callable-boundary");
    let module = program.module_mut(module_id).expect("module");
    let unit = module.intern_type(SsaTypeKind::Unit);
    module
        .add_sequential_container_type(SequentialContainerKind::Array, unit)
        .expect("array");
    let context = Context::create();
    let target = native_target_machine()
        .expect("supported target")
        .1
        .get_target_data();
    let types = TypeMap::lower(&context, module, &target).expect("container type map");
    for signature in [
        CallableSignature {
            parameters: vec![EntityType::Value(unit)],
            returns: vec![],
        },
        CallableSignature {
            parameters: vec![],
            returns: vec![unit],
        },
    ] {
        assert!(matches!(
            types.callable_function_type(&signature, false),
            Err(LlvmAdapterError::Unsupported(_)),
        ));
    }
    let void = types
        .callable_function_type(
            &CallableSignature {
                parameters: vec![],
                returns: vec![],
            },
            false,
        )
        .expect("existing Unit producer uses no SSA result");
    assert!(void.get_return_type().is_none());
}

#[test]
fn container_unit_storage_does_not_enable_aggregate_fields_or_shared_payloads() {
    use super::{LlvmAdapterError, native_target_machine, type_map::TypeMap};
    use inkwell::context::Context;

    for shared in [false, true] {
        let mut program = Program::default();
        let module_id = program.add_module("unit-composite-boundary");
        let module = program.module_mut(module_id).expect("module");
        let unit = module.intern_type(SsaTypeKind::Unit);
        module
            .add_sequential_container_type(SequentialContainerKind::Array, unit)
            .expect("array");
        if shared {
            let owner = module
                .declare_shared_owner("UnitPayload")
                .expect("shared owner");
            module
                .define_shared_owner(owner, unit)
                .expect("Unit payload model");
        } else {
            module
                .add_aggregate_type("UnitField", vec![unit])
                .expect("Unit field model");
        }
        let context = Context::create();
        let target = native_target_machine()
            .expect("supported target")
            .1
            .get_target_data();
        assert!(matches!(
            TypeMap::lower(&context, module, &target),
            Err(LlvmAdapterError::Unsupported(_)),
        ));
    }
}

fn no_results(function: &mut Function, block: BlockId, operation: Operation, origin: &Origin) {
    let (_, results) = function
        .append_instruction(block, operation, vec![], origin.clone())
        .expect("effect instruction");
    assert!(results.is_empty());
}

fn return_void(function: &mut Function, block: BlockId, origin: &Origin) {
    function
        .set_terminator(
            block,
            TerminatorKind::Return { values: vec![] },
            origin.clone(),
        )
        .expect("void return");
}

fn unit_probe_program(
    kind: SequentialContainerKind,
    count: usize,
    index: i64,
) -> (SourceMap, Program, crate::ssa::model::FunctionId) {
    unit_probe_program_with_cfg(kind, count, index, false)
}

fn unit_probe_program_with_cfg(
    kind: SequentialContainerKind,
    count: usize,
    index: i64,
    carry_loan: bool,
) -> (SourceMap, Program, crate::ssa::model::FunctionId) {
    use crate::ssa::model::{Edge, LoanKind, PlaceAccess, ScalarConstant};

    let mut sources = SourceMap::default();
    let source = sources
        .add_source("unit-bounds.ko", "fun app(): Unit {}")
        .expect("source");
    let origin = Origin::Source(sources.span(source, 0, 3).expect("source origin"));
    let mut program = Program::default();
    let module_id = program.add_module("unit-bounds");
    let module = program.module_mut(module_id).expect("module");
    let unit = module.intern_type(SsaTypeKind::Unit);
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let container = module
        .add_sequential_container_type(kind, unit)
        .expect("Unit container");

    let producer = module
        .add_function("produce", vec![], origin.clone())
        .expect("producer");
    let function = module.function_mut(producer).expect("producer");
    let block = function
        .add_block(vec![], origin.clone())
        .expect("producer entry");
    return_void(function, block, &origin);

    let make = module
        .add_function("make", vec![container], origin.clone())
        .expect("factory");
    let function = module.function_mut(make).expect("factory");
    let block = function
        .add_block(vec![], origin.clone())
        .expect("factory entry");
    let mut elements = Vec::new();
    for _ in 0..count {
        no_results(
            function,
            block,
            Operation::DirectCall {
                callee: producer,
                receiver: None,
                arguments: vec![],
            },
            &origin,
        );
        elements.push(append_value(
            function,
            block,
            Operation::Constant(ScalarConstant::Unit),
            unit,
            &origin,
        ));
    }
    let owner = append_value(
        function,
        block,
        Operation::ContainerConstruct {
            container,
            elements,
        },
        container,
        &origin,
    );
    function
        .set_terminator(
            block,
            TerminatorKind::Return {
                values: vec![owner],
            },
            origin.clone(),
        )
        .expect("return container header");

    // The checked index is a function parameter, so the LLVM oracle sees dynamic signed guards.
    // This is not a Borrow<Unit> call: only a container owner and Int cross the call boundary.
    let read = module
        .add_function("read", vec![], origin.clone())
        .expect("read function");
    let function = module.function_mut(read).expect("read function");
    let block = function
        .add_block(
            vec![EntityType::Value(container), EntityType::Value(integer)],
            origin.clone(),
        )
        .expect("read entry");
    let parameters = &function.block(block).unwrap().parameters;
    let (EntityId::Value(owner), EntityId::Value(index_value)) = (parameters[0], parameters[1])
    else {
        panic!("read parameters are values");
    };
    let (_, results) = function
        .append_instruction(
            block,
            Operation::ContainerElementPlace {
                owner: EntityId::Value(owner),
                index: index_value,
            },
            vec![EntityType::Place(unit)],
            origin.clone(),
        )
        .expect("checked Unit element place");
    let [EntityId::Place(place)] = results.as_slice() else {
        panic!("Unit element place");
    };
    let (block, owner, access) = if carry_loan {
        let loan_type = EntityType::Loan {
            kind: LoanKind::Shared,
            target: unit,
        };
        let (_, loans) = function
            .append_instruction(
                block,
                Operation::BorrowBegin {
                    place: *place,
                    kind: LoanKind::Shared,
                },
                vec![loan_type],
                origin.clone(),
            )
            .expect("borrow Unit element");
        let [EntityId::Loan(loan)] = loans.as_slice() else {
            panic!("element loan");
        };
        let next = function
            .add_block(
                vec![EntityType::Value(container), loan_type],
                origin.clone(),
            )
            .expect("loan continuation");
        function
            .set_terminator(
                block,
                TerminatorKind::Branch(Edge {
                    target: next,
                    arguments: vec![EntityId::Value(owner), EntityId::Loan(*loan)],
                }),
                origin.clone(),
            )
            .expect("carry owner and Unit loan");
        let parameters = &function.block(next).expect("continuation").parameters;
        let (EntityId::Value(owner), EntityId::Loan(loan)) = (parameters[0], parameters[1]) else {
            panic!("continuation owner and loan");
        };
        (next, owner, PlaceAccess::Loan(loan))
    } else {
        (block, owner, PlaceAccess::Place(*place))
    };
    append_value(
        function,
        block,
        Operation::Read { source: access },
        unit,
        &origin,
    );
    if let PlaceAccess::Loan(loan) = access {
        no_results(function, block, Operation::BorrowEnd { loan }, &origin);
    }
    no_results(
        function,
        block,
        Operation::PrintLiteral {
            bytes: b"read-ok\n".to_vec(),
        },
        &origin,
    );
    no_results(function, block, Operation::Drop { owner }, &origin);
    return_void(function, block, &origin);

    let app = module
        .add_function("app", vec![], origin.clone())
        .expect("app");
    let function = module.function_mut(app).expect("app");
    let block = function
        .add_block(vec![], origin.clone())
        .expect("app entry");
    let owner = append_value(
        function,
        block,
        Operation::DirectCall {
            callee: make,
            receiver: None,
            arguments: vec![],
        },
        container,
        &origin,
    );
    let index_value = append_value(
        function,
        block,
        Operation::Constant(ScalarConstant::Integer(i128::from(index))),
        integer,
        &origin,
    );
    no_results(
        function,
        block,
        Operation::DirectCall {
            callee: read,
            receiver: None,
            arguments: vec![EntityId::Value(owner), EntityId::Value(index_value)],
        },
        &origin,
    );
    return_void(function, block, &origin);
    verify_program(&program).expect("Unit place/read fixture must satisfy typed SSA and ownership");
    (sources, program, app)
}

fn function_body<'a>(llvm: &'a str, symbol: &str) -> &'a str {
    let start = llvm
        .lines()
        .find(|line| line.starts_with("define ") && line.contains(symbol))
        .expect("function definition");
    llvm.split_once(start)
        .expect("definition start")
        .1
        .split_once("\n}")
        .expect("definition end")
        .0
}

#[test]
fn unit_container_ir_keeps_sentinel_void_abi_and_signed_bounds_without_element_memory() {
    use super::native_target_machine;
    use inkwell::context::Context;

    let context = Context::create();
    let target = native_target_machine()
        .expect("supported target")
        .1
        .get_target_data();
    let alignment = target.get_abi_alignment(&context.struct_type(&[], false));
    for kind in [
        SequentialContainerKind::Array,
        SequentialContainerKind::List,
        SequentialContainerKind::MutableList,
    ] {
        for count in [0, 1, 3] {
            let (_, program, _) = unit_probe_program(kind, count, 0);
            let llvm = render_verified_program(&program).expect("Unit fixture LLVM verifies");
            let sentinel = llvm
                .lines()
                .find(|line| line.starts_with("@koven.zst.sentinel ="))
                .expect("non-null sentinel global");
            assert!(sentinel.contains("private constant i8 0"), "{sentinel}");
            assert!(
                sentinel.ends_with(&format!("align {alignment}")),
                "{sentinel}"
            );
            let factory = function_body(&llvm, "@f1.make(");
            assert!(factory.contains("ptr @koven.zst.sentinel"), "{factory}");
            assert!(!factory.contains("ptr null"), "{factory}");
            assert!(factory.contains(&format!("i64 {count}")), "{factory}");
            assert_eq!(factory.matches("call void @f0.produce()").count(), count);
            assert!(factory.contains("ret %koven.container"), "{factory}");
            assert!(
                llvm.contains("define internal void @f0.produce()"),
                "{llvm}"
            );
            assert!(function_body(&llvm, "@f0.produce(").contains("ret void"));
            assert!(llvm.contains("define internal void @f3.app()"), "{llvm}");
            assert!(function_body(&llvm, "@f3.app(").contains("ret void"));
            assert_no_unit_element_memory_or_allocation(&llvm);

            let read = function_body(&llvm, "@f2.read(");
            let negative = read
                .find("icmp slt i32 %v1, 0")
                .expect("signed negative index guard");
            let upper = read
                .find("icmp sge i32 %v1")
                .expect("signed upper bound guard");
            let branch = read
                .find("br i1 %p0.invalid")
                .expect("invalid bounds branch");
            let marker = read
                .find("call i64 @write")
                .expect("post-read marker write");
            assert!(
                negative < upper && upper < branch && branch < marker,
                "{read}"
            );
            assert!(read.contains("label %p0.abort, label %p0.valid"), "{read}");
            let valid = read
                .split_once("p0.valid:")
                .expect("bounds success block")
                .1;
            assert!(valid.contains("@write"), "{valid}");
            // This direct-place read is synthesized from the zero-size constant. Do not forbid
            // all load {} in other programs: a CFG-carried loan may legitimately use one.
            assert!(!read.contains("load {}"), "{read}");
            let drop = function_body(&llvm, "@koven.drop.");
            assert!(!drop.contains("drop.loop"), "{drop}");
            assert!(!drop.contains("@free"), "{drop}");
        }
    }
}

#[test]
fn unit_container_value_parameter_and_explicit_ssa_return_remain_unsupported() {
    use super::LlvmAdapterError;

    for parameter in [false, true] {
        let origin = origin();
        let mut program = Program::default();
        let module_id = program.add_module("unit-value-abi-boundary");
        let module = program.module_mut(module_id).expect("module");
        let unit = module.intern_type(SsaTypeKind::Unit);
        module
            .add_sequential_container_type(SequentialContainerKind::Array, unit)
            .expect("array");
        let returns = if parameter { vec![] } else { vec![unit] };
        let id = module
            .add_function("unsupported", returns, origin.clone())
            .expect("SSA signature");
        let function = module.function_mut(id).expect("function");
        let parameters = if parameter {
            vec![EntityType::Value(unit)]
        } else {
            vec![]
        };
        let block = function
            .add_block(parameters, origin.clone())
            .expect("entry");
        let values = if parameter {
            vec![]
        } else {
            vec![append_value(
                function,
                block,
                Operation::Constant(crate::ssa::model::ScalarConstant::Unit),
                unit,
                &origin,
            )]
        };
        function
            .set_terminator(block, TerminatorKind::Return { values }, origin.clone())
            .expect("return");
        verify_program(&program)
            .expect("valid SSA is broader than this storage-only backend slice");
        assert!(matches!(
            render_verified_program(&program),
            Err(LlvmAdapterError::Unsupported(_))
        ));
    }
}

#[test]
fn unit_container_native_bounds_abort_before_marker_and_valid_indices_read() {
    use super::emit_verified_object;
    use std::{
        fs,
        path::PathBuf,
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Directory(PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("remove owned native fixture directory");
        }
    }
    let directory = Directory(std::env::temp_dir().join(format!(
        "koven-unit-bounds-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
    )));
    fs::create_dir(&directory.0).expect("create unique fixture directory");
    for (label, count, index, valid) in [
        ("first", 3, 0, true),
        ("last", 3, 2, true),
        ("negative", 3, -1, false),
        ("at_length", 3, 3, false),
        ("empty", 0, 0, false),
    ] {
        let (sources, program, entry) =
            unit_probe_program(SequentialContainerKind::Array, count, index);
        let object = directory.0.join(format!("{label}.o"));
        let executable = directory.0.join(label);
        emit_verified_object(&program, &sources, entry, &object)
            .expect("verified SSA emits real object");
        crate::test_support::assert_native_object(&fs::read(&object).expect("read native object"));
        let link = Command::new(crate::test_support::clang())
            .arg(&object)
            .arg("-o")
            .arg(&executable)
            .output()
            .expect("clang launches");
        assert!(link.status.success(), "{label}: {link:?}");
        let run = Command::new(&executable)
            .current_dir(&directory.0)
            .output()
            .expect("native executable launches");
        if valid {
            assert!(run.status.success(), "{label}: {run:?}");
            assert_eq!(run.stdout, b"read-ok\n", "{label}: {run:?}");
        } else {
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(run.status.signal(), Some(6), "{label}: {run:?}");
            assert!(
                run.stdout.is_empty(),
                "marker must not precede bounds abort: {label}: {run:?}"
            );
        }
        assert!(run.stderr.is_empty(), "{label}: {run:?}");
    }
}

fn assert_no_unit_element_memory_or_allocation(llvm: &str) {
    for line in llvm.lines() {
        let is_call = line.contains("call ");
        assert!(
            !(is_call && (line.contains("@malloc(") || line.contains("@free("))),
            "Unit container must neither allocate nor free its buffer: {line}",
        );
        assert!(
            !(line.contains("getelementptr") && (line.contains("{}") || line.contains(".slot"))),
            "Unit element addressing is unnecessary: {line}",
        );
        assert!(
            !line.trim_start().starts_with("store {}"),
            "Unit element store: {line}"
        );
    }
}

#[test]
fn cfg_carried_unit_element_loan_permits_only_a_zero_byte_read_after_bounds() {
    use super::native_target_machine;
    use inkwell::context::Context;

    let (_, program, _) = unit_probe_program_with_cfg(SequentialContainerKind::Array, 3, 2, true);
    let llvm = render_verified_program(&program).expect("CFG-carried Unit loan LLVM verifies");
    assert_no_unit_element_memory_or_allocation(&llvm);
    let context = Context::create();
    let target = native_target_machine()
        .expect("supported target")
        .1
        .get_target_data();
    let unit_storage = context.struct_type(&[], false);
    assert_eq!(target.get_store_size(&unit_storage), 0);
    assert_eq!(target.get_abi_size(&unit_storage), 0);
    let alignment = target.get_abi_alignment(&unit_storage);
    let read = function_body(&llvm, "@f2.read(");
    assert!(read.contains("icmp slt i32 %v1, 0"), "{read}");
    assert!(read.contains("icmp sge i32 %v1"), "{read}");
    assert!(
        read.contains("br i1 %p0.invalid, label %p0.abort, label %p0.valid"),
        "{read}"
    );
    let success = read.split_once("p0.valid:").expect("success block").1;
    assert!(success.contains("br label %bb1"), "{success}");
    let next = read
        .split_once("bb1:")
        .expect("carried loan block")
        .1
        .split_once("p0.abort:")
        .expect("continuation end")
        .0;
    let phi = next
        .lines()
        .find(|line| line.contains("= phi ptr"))
        .expect("loan pointer crosses CFG via phi");
    assert!(phi.contains("%p0.valid"), "{phi}");
    let pointer = phi.split_once(" = ").expect("phi assignment").0.trim();
    let loads = next
        .lines()
        .filter(|line| line.contains("load {}"))
        .collect::<Vec<_>>();
    assert_eq!(loads.len(), 1, "one source-level Unit read: {next}");
    assert!(
        loads[0].contains(&format!("ptr {pointer}, align {alignment}")),
        "{}",
        loads[0]
    );
    let loaded = next.find("load {}").expect("zero-byte load");
    let marker = next.find("@write").expect("marker after read");
    assert!(loaded < marker, "{next}");
    let abort = read
        .split_once("p0.abort:")
        .expect("abort block")
        .1
        .split_once("p0.valid:")
        .expect("abort end")
        .0;
    assert!(abort.contains("call void @abort()"), "{abort}");
    assert!(abort.contains("unreachable"), "{abort}");
    assert!(!abort.contains("@write"), "{abort}");
}

#[test]
fn unit_container_element_storage_accessor_has_no_unknown_or_opaque_fallback() {
    use super::{LlvmAdapterError, native_target_machine, type_map::TypeMap};
    use crate::ssa::model::Ownership;
    use inkwell::context::Context;

    let mut program = Program::default();
    let foreign_id = program.add_module("foreign-unit");
    let foreign_unit = program
        .module_mut(foreign_id)
        .expect("foreign module")
        .intern_type(SsaTypeKind::Unit);
    let module_id = program.add_module("bounded-unit-storage");
    let module = program.module_mut(module_id).expect("module");
    let unit = module.intern_type(SsaTypeKind::Unit);
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let opaque = module.intern_type(SsaTypeKind::Opaque {
        name: "Unrepresented".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let unknown = SsaTypeId {
        module: module_id,
        index: module.types.len(),
    };
    let context = Context::create();
    let target = native_target_machine()
        .expect("supported target")
        .1
        .get_target_data();
    let types = TypeMap::lower(&context, module, &target).expect("opaque need not be materialized");
    assert_eq!(
        types.container_element_type(unit).unwrap(),
        context.struct_type(&[], false).into()
    );
    assert_eq!(
        types.container_element_type(integer).unwrap(),
        types.basic_type(integer).unwrap()
    );
    for absent in [foreign_unit, opaque, unknown] {
        assert!(matches!(
            types.container_element_type(absent),
            Err(LlvmAdapterError::Unsupported(_))
        ));
        assert!(matches!(
            types.basic_type(absent),
            Err(LlvmAdapterError::Unsupported(_))
        ));
    }
}
