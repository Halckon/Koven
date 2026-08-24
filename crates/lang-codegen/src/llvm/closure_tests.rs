use lang_frontend::source::SourceMap;

use crate::ssa::model::{
    ClosureCaptureMode, ClosureCaptureOperand, ClosureCaptureType, EntityId, EntityType, Operation,
    Origin, Program, SsaTypeKind, TerminatorKind, ValueId,
};

use super::render_verified_program;

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value")
    };
    value
}

#[test]
fn concrete_closure_and_function_pointer_lower_without_hidden_allocation() {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("closure.ko", "move { n }")
        .expect("source");
    let origin = Origin::Source(sources.span(source, 0, 4).expect("span"));
    let mut program = Program::default();
    let module_id = program.add_module("closure");
    let module = program.module_mut(module_id).expect("module");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let pointer = module
        .add_function_pointer_type(vec![], vec![])
        .expect("pointer");
    let environment = module
        .add_aggregate_type("Env", vec![integer])
        .expect("environment");
    let closure = module
        .add_concrete_closure_type(
            "Counter",
            vec![],
            vec![],
            environment,
            vec![ClosureCaptureType {
                mode: ClosureCaptureMode::Owned,
                ty: integer,
            }],
        )
        .expect("closure");

    let target = module
        .add_function("target", vec![], origin.clone())
        .expect("target");
    let target_entry = module
        .function_mut(target)
        .expect("target")
        .add_block(vec![], origin.clone())
        .expect("entry");
    module
        .function_mut(target)
        .expect("target")
        .set_terminator(
            target_entry,
            TerminatorKind::Return { values: vec![] },
            origin.clone(),
        )
        .expect("return");

    let thunk = module
        .add_function("thunk", vec![], origin.clone())
        .expect("thunk");
    let thunk_entry = module
        .function_mut(thunk)
        .expect("thunk")
        .add_block(vec![EntityType::Value(environment)], origin.clone())
        .expect("entry");
    module
        .function_mut(thunk)
        .expect("thunk")
        .set_terminator(
            thunk_entry,
            TerminatorKind::Return { values: vec![] },
            origin.clone(),
        )
        .expect("return");

    let main = module
        .add_function("main", vec![], origin.clone())
        .expect("main");
    let function = module.function_mut(main).expect("main");
    let entry = function
        .add_block(vec![EntityType::Value(integer)], origin.clone())
        .expect("entry");
    let captured = value(function.block(entry).expect("entry").parameters[0]);
    let address = value(
        function
            .append_instruction(
                entry,
                Operation::FunctionAddress { target },
                vec![EntityType::Value(pointer)],
                origin.clone(),
            )
            .expect("address")
            .1[0],
    );
    function
        .append_instruction(
            entry,
            Operation::CallableInvoke {
                callable: address,
                arguments: vec![],
            },
            vec![],
            origin.clone(),
        )
        .expect("invoke pointer");
    let closure_value = value(
        function
            .append_instruction(
                entry,
                Operation::ClosureConstruct {
                    closure,
                    thunk,
                    captures: vec![ClosureCaptureOperand::Owned(captured)],
                },
                vec![EntityType::Value(closure)],
                origin.clone(),
            )
            .expect("construct")
            .1[0],
    );
    for _ in 0..2 {
        function
            .append_instruction(
                entry,
                Operation::CallableInvoke {
                    callable: closure_value,
                    arguments: vec![],
                },
                vec![],
                origin.clone(),
            )
            .expect("invoke closure");
    }
    function
        .append_instruction(
            entry,
            Operation::Drop {
                owner: closure_value,
            },
            vec![],
            origin.clone(),
        )
        .expect("drop closure");
    function
        .append_instruction(
            entry,
            Operation::Drop { owner: address },
            vec![],
            origin.clone(),
        )
        .expect("drop pointer");
    function
        .set_terminator(entry, TerminatorKind::Return { values: vec![] }, origin)
        .expect("return");

    let llvm = render_verified_program(&program).expect("closure LLVM must verify");
    assert!(llvm.contains("type { ptr, %koven.t"));
    assert_eq!(llvm.matches("call void %").count(), 2);
    assert!(llvm.contains("call void @f0.target"));
    assert!(!llvm.contains("@malloc"));
    assert!(!llvm.contains("@free"));
    assert!(!llvm.contains("type_id"));
}
