use inkwell::context::Context;

use crate::ssa::model::{
    ClosureCaptureMode, ClosureCaptureType, Ownership, Program, SequentialContainerKind,
    SsaTypeKind,
};

use super::{
    LlvmAdapterError,
    layout::{
        LayoutFailure, LayoutQuantity, RawLayout, TargetLayoutError, TargetLayoutPlan,
        checked_record,
    },
    native_target_machine,
    type_map::TypeMap,
};

#[test]
fn preflight_matches_the_first_target_for_closed_composite_shapes() {
    let mut program = Program::default();
    let module_id = program.add_module("layout-preflight");
    let module = program.module_mut(module_id).expect("module must exist");
    let byte = module.intern_type(SsaTypeKind::Integer {
        bits: 8,
        signed: false,
    });
    let word = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: false,
    });
    let padded = module
        .add_aggregate_type("Padded", vec![byte, word, byte])
        .expect("padded aggregate must be valid");
    let nested = module
        .add_aggregate_type("Nested", vec![byte, padded])
        .expect("nested aggregate must be valid");
    let owner = module
        .declare_heap_owner("Node")
        .expect("owner declaration must be valid");
    let payload = module
        .add_aggregate_type("Node.payload", vec![owner, nested])
        .expect("recursive payload must be valid through owner handle");
    module
        .define_heap_owner(owner, payload)
        .expect("owner definition must be valid");
    let shared = module
        .declare_shared_owner("Shared")
        .expect("shared owner declaration must be valid");
    let shared_payload = module
        .add_aggregate_type("Shared.payload", vec![byte, word])
        .expect("shared payload must be valid");
    module
        .define_shared_owner(shared, shared_payload)
        .expect("shared owner definition must be valid");
    let environment = module
        .add_aggregate_type("Closure.environment", vec![word])
        .expect("closure environment must be valid");
    let closure = module
        .add_concrete_closure_type(
            "Closure",
            Vec::new(),
            Vec::new(),
            environment,
            vec![ClosureCaptureType {
                mode: ClosureCaptureMode::Owned,
                ty: word,
            }],
        )
        .expect("closure type must be valid");
    let list = module
        .add_sequential_container_type(SequentialContainerKind::List, padded)
        .expect("list type must be valid");
    let zst = module.intern_type(SsaTypeKind::ZeroSized {
        name: "Token".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let mutable_zst = module
        .add_sequential_container_type(SequentialContainerKind::MutableList, zst)
        .expect("mutable ZST list must be valid");

    let context = Context::create();
    let target = native_target_machine()
        .expect("first target must exist")
        .1
        .get_target_data();
    let first = TargetLayoutPlan::build(&context, module, &target)
        .expect("closed layouts must pass preflight");
    let second = TargetLayoutPlan::build(&context, module, &target)
        .expect("repeated preflight must succeed");
    let types = TypeMap::lower(&context, module, &target)
        .expect("valid preflight must permit LLVM type construction");

    assert_eq!(first.layout(padded), second.layout(padded));
    assert_eq!(first.layout(padded).unwrap().size, 24);
    assert_eq!(first.layout(padded).unwrap().alignment, 8);
    assert_eq!(first.layout(nested).unwrap().size, 32);
    assert_eq!(first.layout(owner).unwrap().size, 8);
    assert_eq!(first.layout(shared).unwrap().size, 8);
    assert_eq!(first.shared_control_layout(shared).unwrap().size, 24);
    assert_eq!(first.shared_control_layout(shared).unwrap().alignment, 8);
    assert_eq!(first.layout(payload).unwrap().size, 40);
    assert_eq!(first.layout(environment).unwrap().size, 8);
    assert_eq!(first.layout(closure).unwrap().size, 16);
    assert_eq!(first.layout(list).unwrap().size, 16);
    assert_eq!(first.layout(zst).unwrap().size, 0);
    assert_eq!(first.layout(mutable_zst).unwrap().size, 24);

    for aggregate in [padded, nested, payload, shared_payload, environment] {
        let planned = first.layout(aggregate).unwrap();
        let actual = types.aggregate_type(aggregate).unwrap();
        assert_eq!(planned.size, target.get_abi_size(&actual));
        assert_eq!(planned.alignment, target.get_abi_alignment(&actual));
    }
    let actual_owner = types.basic_type(owner).unwrap();
    assert_eq!(
        first.layout(owner).unwrap().size,
        target.get_abi_size(&actual_owner)
    );
    let actual_control = types.shared_control(shared).unwrap();
    assert_eq!(
        first.shared_control_layout(shared).unwrap().size,
        target.get_abi_size(&actual_control)
    );
    assert_eq!(types.shared_control_size(shared).unwrap(), 24);
    let actual_closure = types.closure_layout(closure).unwrap().value;
    assert_eq!(
        first.layout(closure).unwrap().size,
        target.get_abi_size(&actual_closure)
    );
    for container in [list, mutable_zst] {
        let planned = first.layout(container).unwrap();
        let actual = types.container_layout(container).unwrap();
        assert_eq!(planned.size, target.get_abi_size(&actual.header));
        assert_eq!(planned.alignment, target.get_abi_alignment(&actual.header));
    }
    assert_eq!(types.container_layout(list).unwrap().stride, 24);
    assert_eq!(types.container_layout(mutable_zst).unwrap().stride, 0);
}

#[test]
fn oversized_recursive_shape_fails_deterministically_before_type_lowering() {
    let mut program = Program::default();
    let module_id = program.add_module("oversized-layout");
    let module = program.module_mut(module_id).expect("module must exist");
    let mut large = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: false,
    });
    for depth in 0..61 {
        large = module
            .add_aggregate_type(format!("Large{depth}"), vec![large, large])
            .expect("compact nested aggregate graph must be valid");
    }

    let context = Context::create();
    let target = native_target_machine()
        .expect("first target must exist")
        .1
        .get_target_data();
    let first = TargetLayoutPlan::build(&context, module, &target)
        .expect_err("2^64-byte aggregate must exceed AArch64 size_t");
    let second = TargetLayoutPlan::build(&context, module, &target)
        .expect_err("repeated oversized preflight must fail");
    let type_map_error = match TypeMap::lower(&context, module, &target) {
        Ok(_) => panic!("type map must run preflight before creating composite bodies"),
        Err(error) => error,
    };

    assert_eq!(first, second);
    assert_eq!(first, type_map_error);
    assert!(matches!(
        first,
        LlvmAdapterError::InvalidLayout(TargetLayoutError {
            ty,
            quantity: LayoutQuantity::Size,
            failure: LayoutFailure::ExceedsTarget {
                value,
                maximum
            },
        }) if ty == large && value == (1_u128 << 64) && maximum == u128::from(u64::MAX)
    ));
}

#[test]
fn deep_layout_preflight_is_iterative() {
    let mut program = Program::default();
    let module_id = program.add_module("deep-layout");
    let module = program.module_mut(module_id).expect("module must exist");
    let mut nested = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: false,
    });
    for depth in 0..4_096 {
        nested = module
            .add_aggregate_type(format!("Nested{depth}"), vec![nested])
            .expect("source-ordered nested aggregate must be valid");
    }

    let context = Context::create();
    let target = native_target_machine()
        .expect("first target must exist")
        .1
        .get_target_data();
    let plan = TargetLayoutPlan::build(&context, module, &target)
        .expect("deep source-ordered layout must not consume the Rust call stack");

    assert_eq!(plan.layout(nested).unwrap().size, 8);
    assert_eq!(plan.layout(nested).unwrap().alignment, 8);
}

#[test]
fn record_layout_reports_checked_arithmetic_overflow() {
    let mut program = Program::default();
    let module_id = program.add_module("layout-overflow");
    let module = program.module_mut(module_id).expect("module must exist");
    let ty = module.intern_type(SsaTypeKind::Integer {
        bits: 8,
        signed: false,
    });

    let error = checked_record(
        ty,
        &[
            RawLayout {
                size: u128::MAX,
                alignment: 1,
            },
            RawLayout {
                size: 1,
                alignment: 1,
            },
        ],
        u128::MAX,
    )
    .expect_err("record size addition must be checked");

    assert_eq!(
        error,
        LlvmAdapterError::InvalidLayout(TargetLayoutError {
            ty,
            quantity: LayoutQuantity::Size,
            failure: LayoutFailure::ArithmeticOverflow,
        })
    );
}

pub(crate) mod aggregate_tests {
use inkwell::context::Context;
use lang_frontend::source::SourceMap;

use crate::ssa::model::{
    Edge, EntityId, EntityType, LoanKind, Operation, Origin, Program, SsaTypeKind, TerminatorKind,
    ValueId,
};

use super::super::{native_target_machine, render_verified_program, type_map::TypeMap};

fn origin() -> Origin {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("aggregate-llvm.ko", "value class Point")
        .expect("test source must be unique");
    Origin::Source(sources.span(source, 0, 5).expect("test span must be valid"))
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value, got {entity:?}");
    };
    value
}

#[test]
fn target_data_layout_drives_aggregate_size_alignment_and_field_offsets() {
    let mut program = Program::default();
    let module_id = program.add_module("layout");
    let module = program.module_mut(module_id).expect("module must exist");
    let byte = module.intern_type(SsaTypeKind::Integer {
        bits: 8,
        signed: false,
    });
    let word = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: false,
    });
    let aggregate = module
        .add_aggregate_type("Padded", vec![byte, word, byte])
        .expect("aggregate type must be valid");

    let context = Context::create();
    let target = super::super::native_target_machine()
        .expect("target machine must exist")
        .1
        .get_target_data();
    let types = TypeMap::lower(&context, module, &target).expect("type lowering must succeed");
    let (_, machine) = native_target_machine().expect("first target must be available");
    let layout = types
        .aggregate_layout(&machine.get_target_data(), aggregate)
        .expect("aggregate layout must be computable");

    assert_eq!(layout.store_size, 24);
    assert_eq!(layout.abi_size, 24);
    assert_eq!(layout.abi_alignment, 8);
    assert_eq!(layout.field_offsets, vec![0, 8, 16]);
}

#[test]
fn aggregate_values_calls_returns_and_phi_lower_as_first_class_llvm_values() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("aggregate_values");
    let module = program.module_mut(module_id).expect("module must exist");
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let point = module
        .add_aggregate_type("Point", vec![integer, integer])
        .expect("point type must be valid");
    let large = module
        .add_aggregate_type("Large", vec![integer; 8])
        .expect("large aggregate must be valid");

    let swap_id = module
        .add_function("swap", vec![point], origin.clone())
        .expect("swap signature must be valid");
    let swap = module.function_mut(swap_id).expect("swap must exist");
    let entry = swap
        .add_block(vec![EntityType::Value(point)], origin.clone())
        .expect("swap entry must be valid");
    let input = value(swap.block(entry).expect("entry").parameters[0]);
    let mut fields = Vec::new();
    for field in 0..2 {
        fields.push(value(
            swap.append_instruction(
                entry,
                Operation::AggregateProject {
                    aggregate: input,
                    field,
                },
                vec![EntityType::Value(integer)],
                origin.clone(),
            )
            .expect("project must be appendable")
            .1[0],
        ));
    }
    let swapped = value(
        swap.append_instruction(
            entry,
            Operation::AggregateConstruct {
                aggregate: point,
                fields: vec![fields[1], fields[0]],
            },
            vec![EntityType::Value(point)],
            origin.clone(),
        )
        .expect("construct must be appendable")
        .1[0],
    );
    swap.set_terminator(
        entry,
        TerminatorKind::Return {
            values: vec![swapped],
        },
        origin.clone(),
    )
    .expect("swap return must be valid");

    let caller_id = module
        .add_function("caller", vec![point], origin.clone())
        .expect("caller signature must be valid");
    let caller = module.function_mut(caller_id).expect("caller must exist");
    let entry = caller
        .add_block(vec![EntityType::Value(point)], origin.clone())
        .expect("caller entry must be valid");
    let input = value(caller.block(entry).expect("entry").parameters[0]);
    let called = value(
        caller
            .append_instruction(
                entry,
                Operation::DirectCall {
                    callee: swap_id,
                    receiver: None,
                    arguments: vec![EntityId::Value(input)],
                },
                vec![EntityType::Value(point)],
                origin.clone(),
            )
            .expect("call must be appendable")
            .1[0],
    );
    caller
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: vec![called],
            },
            origin.clone(),
        )
        .expect("caller return must be valid");

    let choose_id = module
        .add_function("choose", vec![point], origin.clone())
        .expect("choose signature must be valid");
    let choose = module.function_mut(choose_id).expect("choose must exist");
    let entry = choose
        .add_block(
            vec![
                EntityType::Value(boolean),
                EntityType::Value(point),
                EntityType::Value(point),
            ],
            origin.clone(),
        )
        .expect("choose entry must be valid");
    let parameters = choose.block(entry).expect("entry").parameters.clone();
    let when_true = choose
        .add_block(Vec::new(), origin.clone())
        .expect("true block must be valid");
    let when_false = choose
        .add_block(Vec::new(), origin.clone())
        .expect("false block must be valid");
    let join = choose
        .add_block(vec![EntityType::Value(point)], origin.clone())
        .expect("join must be valid");
    choose
        .set_terminator(
            entry,
            TerminatorKind::Conditional {
                condition: value(parameters[0]),
                when_true: Edge {
                    target: when_true,
                    arguments: Vec::new(),
                },
                when_false: Edge {
                    target: when_false,
                    arguments: Vec::new(),
                },
            },
            origin.clone(),
        )
        .expect("conditional must be valid");
    choose
        .set_terminator(
            when_true,
            TerminatorKind::Branch(Edge {
                target: join,
                arguments: vec![parameters[1]],
            }),
            origin.clone(),
        )
        .expect("true branch must be valid");
    choose
        .set_terminator(
            when_false,
            TerminatorKind::Branch(Edge {
                target: join,
                arguments: vec![parameters[2]],
            }),
            origin.clone(),
        )
        .expect("false branch must be valid");
    let joined = value(choose.block(join).expect("join").parameters[0]);
    choose
        .set_terminator(
            join,
            TerminatorKind::Return {
                values: vec![joined],
            },
            origin.clone(),
        )
        .expect("join return must be valid");

    let identity_id = module
        .add_function("large_identity", vec![large], origin.clone())
        .expect("large identity signature must be valid");
    let identity = module
        .function_mut(identity_id)
        .expect("large identity must exist");
    let entry = identity
        .add_block(vec![EntityType::Value(large)], origin.clone())
        .expect("large entry must be valid");
    let input = value(identity.block(entry).expect("entry").parameters[0]);
    identity
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: vec![input],
            },
            origin,
        )
        .expect("large return must be valid");

    let first = render_verified_program(&program).expect("aggregate LLVM lowering must succeed");
    let second = render_verified_program(&program).expect("repeat lowering must succeed");
    crate::test_support::diagnostics::assert_text_eq!(first, second);
    assert!(first.contains("%koven.t2 = type { i32, i32 }"));
    assert!(first.contains("%koven.t3 = type { i32, i32, i32, i32, i32, i32, i32, i32 }"));
    assert!(first.contains("extractvalue %koven.t2"));
    assert!(first.contains("insertvalue %koven.t2"));
    assert!(first.contains("call %koven.t2 @f0.swap"));
    assert!(first.contains("phi %koven.t2"));
    assert!(!first.contains("malloc"));
    assert!(!first.contains("free"));
    assert!(!first.contains("koven.drop"));
}

#[test]
fn heap_owner_drop_lowers_to_explicit_unique_free_glue() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("pending_drop");
    let module = program.module_mut(module_id).expect("module must exist");
    let payload = module
        .add_aggregate_type("Owner.payload", Vec::new())
        .expect("payload must be valid");
    let owner = module
        .declare_heap_owner("Owner")
        .expect("owner declaration must be valid");
    module
        .define_heap_owner(owner, payload)
        .expect("owner definition must be valid");
    let function_id = module
        .add_function("drop_owner", Vec::new(), origin.clone())
        .expect("function must be valid");
    let function = module
        .function_mut(function_id)
        .expect("function must exist");
    let entry = function
        .add_block(vec![EntityType::Value(owner)], origin.clone())
        .expect("entry must be valid");
    let owner = value(function.block(entry).expect("entry").parameters[0]);
    function
        .append_instruction(entry, Operation::Drop { owner }, Vec::new(), origin.clone())
        .expect("drop must be appendable");
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("return must be valid");

    let llvm = render_verified_program(&program).expect("heap owner drop glue must lower");
    assert!(llvm.contains("define internal void @koven.drop.t1(ptr %0)"));
    assert_eq!(llvm.matches("call void @free(ptr %0)").count(), 1);
    assert_eq!(llvm.matches("call void @koven.drop.t1").count(), 1);
}

#[test]
fn shared_heap_field_loan_loads_the_handle_before_projecting_payload() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("heap_field_loan");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let delegate_payload = module
        .add_aggregate_type("Reader.payload", vec![integer])
        .expect("delegate payload must be valid");
    let delegate = module
        .declare_heap_owner("Reader")
        .expect("delegate owner declaration must be valid");
    module
        .define_heap_owner(delegate, delegate_payload)
        .expect("delegate owner definition must be valid");
    let payload = module
        .add_aggregate_type("Host.payload", vec![delegate])
        .expect("host payload must be valid");
    let host = module
        .declare_heap_owner("Host")
        .expect("host owner declaration must be valid");
    module
        .define_heap_owner(host, payload)
        .expect("host owner definition must be valid");
    let delegate_receiver = EntityType::Loan {
        kind: LoanKind::Shared,
        target: delegate,
    };
    let read_id = module
        .add_instance_function("read", delegate_receiver, Vec::new(), origin.clone())
        .expect("delegate function must be valid");
    let read = module
        .function_mut(read_id)
        .expect("delegate function must exist");
    let read_entry = read
        .add_block(vec![delegate_receiver], origin.clone())
        .expect("delegate entry must be valid");
    read.set_terminator(
        read_entry,
        TerminatorKind::Return { values: Vec::new() },
        origin.clone(),
    )
    .expect("delegate return must be valid");
    let receiver_type = EntityType::Loan {
        kind: LoanKind::Shared,
        target: host,
    };
    let function_id = module
        .add_instance_function("project", receiver_type, Vec::new(), origin.clone())
        .expect("function must be valid");
    let function = module
        .function_mut(function_id)
        .expect("function must exist");
    let entry = function
        .add_block(vec![receiver_type], origin.clone())
        .expect("entry must be valid");
    let EntityId::Loan(receiver) = function.block(entry).expect("entry").parameters[0] else {
        panic!("receiver loan");
    };
    let field_loan = function
        .append_instruction(
            entry,
            Operation::SharedHeapFieldLoan {
                base: receiver,
                field: 0,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: delegate,
            }],
            origin.clone(),
        )
        .expect("field loan must be appendable")
        .1[0];
    let EntityId::Loan(field_loan) = field_loan else {
        panic!("field loan result");
    };
    function
        .append_instruction(
            entry,
            Operation::DirectCall {
                callee: read_id,
                receiver: Some(EntityId::Loan(field_loan)),
                arguments: Vec::new(),
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("delegate call must be appendable");
    function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan: field_loan },
            Vec::new(),
            origin.clone(),
        )
        .expect("field loan end must be appendable");
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("return must be valid");

    let llvm = render_verified_program(&program).expect("heap field loan LLVM must verify");
    assert!(llvm.contains("%l1.handle = load ptr, ptr %l0"));
    assert!(llvm.contains("getelementptr inbounds nuw"));
    assert!(llvm.contains("ptr %l1.handle"));
    assert!(llvm.contains("call void @f0.read(ptr %l1)"));
    assert!(!llvm.contains("load ptr, ptr %l1"));
    assert!(!llvm.contains("extractvalue"));
}
}
