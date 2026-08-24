use inkwell::context::Context;

use crate::ssa::model::{
    ClosureCaptureMode, ClosureCaptureType, Ownership, Program, SequentialContainerKind,
    SsaTypeKind,
};

use super::{
    LlvmAdapterError, first_target_machine,
    layout::{
        LayoutFailure, LayoutQuantity, RawLayout, TargetLayoutError, TargetLayoutPlan,
        checked_record,
    },
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
    let target = first_target_machine()
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
    assert_eq!(first.layout(payload).unwrap().size, 40);
    assert_eq!(first.layout(environment).unwrap().size, 8);
    assert_eq!(first.layout(closure).unwrap().size, 16);
    assert_eq!(first.layout(list).unwrap().size, 16);
    assert_eq!(first.layout(zst).unwrap().size, 0);
    assert_eq!(first.layout(mutable_zst).unwrap().size, 24);

    for aggregate in [padded, nested, payload, environment] {
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
    let target = first_target_machine()
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
    let target = first_target_machine()
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
