use inkwell::context::Context;
use lang_frontend::source::SourceMap;

use crate::ssa::model::{
    Edge, EntityId, EntityType, Operation, Origin, Program, SsaTypeKind, TerminatorKind, ValueId,
};

use super::{LlvmAdapterError, first_target_machine, render_verified_program, type_map::TypeMap};

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
    let types = TypeMap::lower(&context, module).expect("type lowering must succeed");
    let (_, machine) = first_target_machine().expect("first target must be available");
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
                    arguments: vec![input],
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
    assert_eq!(first, second);
    assert!(first.contains("%koven.t2 = type { i32, i32 }"));
    assert!(first.contains("%koven.t3 = type { i32, i32, i32, i32, i32, i32, i32, i32 }"));
    assert!(first.contains("extractvalue %koven.t2"));
    assert!(first.contains("insertvalue %koven.t2"));
    assert!(first.contains("call %koven.t2 @f0.swap"));
    assert!(first.contains("phi %koven.t2"));
    assert!(!first.contains("malloc"));
    assert!(!first.contains("free"));
}

#[test]
fn aggregate_lowering_does_not_silently_erase_pending_move_only_drop() {
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

    let error = render_verified_program(&program)
        .expect_err("pending drop glue must not be silently erased");
    assert!(matches!(error, LlvmAdapterError::Unsupported(message) if message.contains("drop")));
}
