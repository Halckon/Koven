use lang_frontend::source::SourceMap;

use crate::ssa::model::{
    Edge, EntityId, EntityType, LoanKind, Operation, Origin, Program, TerminatorKind, ValueId,
};

use super::render_verified_program;

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value");
    };
    value
}

#[test]
fn nullable_handle_uses_one_pointer_non_null_phi_and_conditional_drop() {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("nullable-llvm.ko", "if (node != null) node")
        .expect("source");
    let origin = Origin::Source(sources.span(source, 0, 2).expect("span"));
    let mut program = Program::default();
    let module_id = program.add_module("nullable_llvm");
    let module = program.module_mut(module_id).expect("module");
    let payload = module
        .add_aggregate_type("Node.payload", Vec::new())
        .expect("payload");
    let inner = module.declare_heap_owner("Node").expect("owner");
    module
        .define_heap_owner(inner, payload)
        .expect("definition");
    let nullable = module.add_nullable_handle_type(inner).expect("nullable");

    let function_id = module
        .add_function("inspect", Vec::new(), origin.clone())
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let entry = function
        .add_block(vec![EntityType::Value(nullable)], origin.clone())
        .expect("entry");
    let owner = value(function.block(entry).expect("block").parameters[0]);
    let null_block = function
        .add_block(vec![EntityType::Value(nullable)], origin.clone())
        .expect("null block");
    let non_null_block = function
        .add_block(
            vec![
                EntityType::Value(nullable),
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: inner,
                },
            ],
            origin.clone(),
        )
        .expect("non-null block");
    let null_owner = value(function.block(null_block).expect("block").parameters[0]);
    let parameters = function
        .block(non_null_block)
        .expect("block")
        .parameters
        .clone();
    let non_null_owner = value(parameters[0]);
    let EntityId::Loan(view) = parameters[1] else {
        panic!("expected non-null view");
    };
    function
        .set_terminator(
            entry,
            TerminatorKind::NullableBranch {
                owner,
                when_null: Edge {
                    target: null_block,
                    arguments: vec![EntityId::Value(owner)],
                },
                when_non_null: Edge {
                    target: non_null_block,
                    arguments: vec![EntityId::Value(owner)],
                },
                view,
            },
            origin.clone(),
        )
        .expect("branch");
    function
        .append_instruction(
            null_block,
            Operation::Drop { owner: null_owner },
            Vec::new(),
            origin.clone(),
        )
        .expect("nullable drop");
    function
        .set_terminator(
            null_block,
            TerminatorKind::Return { values: Vec::new() },
            origin.clone(),
        )
        .expect("return");
    let taken = value(
        function
            .append_instruction(
                non_null_block,
                Operation::NullableTake {
                    owner: non_null_owner,
                    proof: view,
                },
                vec![EntityType::Value(inner)],
                origin.clone(),
            )
            .expect("take")
            .1[0],
    );
    function
        .append_instruction(
            non_null_block,
            Operation::Drop { owner: taken },
            Vec::new(),
            origin.clone(),
        )
        .expect("inner drop");
    function
        .set_terminator(
            non_null_block,
            TerminatorKind::Return { values: Vec::new() },
            origin,
        )
        .expect("return");

    let ir = render_verified_program(&program).expect("nullable LLVM lowering");
    assert!(ir.contains("phi ptr"), "{ir}");
    assert!(ir.contains("icmp eq ptr"), "{ir}");
    assert!(
        ir.contains("define internal void @koven.drop.t2(ptr"),
        "{ir}"
    );
    assert!(ir.contains("call void @koven.drop.t1(ptr"), "{ir}");
    assert!(
        !ir.contains("koven.enum"),
        "nullable pointer must not gain a tag: {ir}"
    );
    assert!(
        !ir.contains("malloc"),
        "nullable wrapping must not allocate: {ir}"
    );
}
