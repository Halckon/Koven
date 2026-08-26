use lang_frontend::source::SourceMap;

use crate::ssa::model::{
    ClosureCaptureMode, ClosureCaptureOperand, ClosureCaptureType, EntityId, EntityType, LoanKind,
    Operation, Origin, Ownership, Program, SsaTypeKind, TerminatorKind, ValueId,
};

use super::render_verified_program;

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value")
    };
    value
}

#[test]
fn shared_capture_lowers_existing_loan_pointer_into_inline_environment() {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("shared.ko", "{ resource }")
        .expect("source");
    let origin = Origin::Source(sources.span(source, 0, 4).expect("span"));
    let mut program = Program::default();
    let module_id = program.add_module("shared_closure");
    let module = program.module_mut(module_id).expect("module");
    let resource = module.intern_type(SsaTypeKind::ZeroSized {
        name: "Resource".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let reference = module
        .add_shared_reference_type(resource)
        .expect("reference");
    let environment = module
        .add_aggregate_type("SharedEnv", vec![reference])
        .expect("environment");
    let closure = module
        .add_concrete_closure_type(
            "SharedClosure",
            vec![],
            vec![],
            environment,
            vec![ClosureCaptureType {
                mode: ClosureCaptureMode::Shared,
                ty: resource,
            }],
        )
        .expect("closure");
    let thunk = module
        .add_function("thunk", vec![], origin.clone())
        .expect("thunk");
    let thunk_entry = module
        .function_mut(thunk)
        .expect("thunk")
        .add_block(
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: environment,
            }],
            origin.clone(),
        )
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
        .add_block(vec![EntityType::Value(resource)], origin.clone())
        .expect("entry");
    let owner = value(function.block(entry).expect("entry").parameters[0]);
    let EntityId::Place(place) = function
        .append_instruction(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(resource)],
            origin.clone(),
        )
        .expect("place")
        .1[0]
    else {
        panic!("place")
    };
    let EntityId::Loan(loan) = function
        .append_instruction(
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: resource,
            }],
            origin.clone(),
        )
        .expect("loan")
        .1[0]
    else {
        panic!("loan")
    };
    let closure_value = value(
        function
            .append_instruction(
                entry,
                Operation::ClosureConstruct {
                    closure,
                    thunk,
                    captures: vec![ClosureCaptureOperand::Shared(loan)],
                },
                vec![EntityType::Value(closure)],
                origin.clone(),
            )
            .expect("construct")
            .1[0],
    );
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
        .expect("invoke");
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
        .append_instruction(entry, Operation::Drop { owner }, vec![], origin.clone())
        .expect("drop owner");
    function
        .set_terminator(entry, TerminatorKind::Return { values: vec![] }, origin)
        .expect("return");

    let llvm = render_verified_program(&program).expect("shared closure LLVM must verify");
    assert!(llvm.contains("%koven.t2 = type { ptr }"));
    assert!(llvm.contains("type { ptr, %koven.t2 }"));
    assert!(llvm.contains("call void %"));
    assert!(!llvm.contains("@malloc"));
    assert!(!llvm.contains("@free"));
}

#[test]
fn mixed_closure_drop_skips_shared_slot_and_drops_owned_slots_in_reverse() {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("mixed.ko", "move { a; shared; b }")
        .expect("source");
    let origin = Origin::Source(sources.span(source, 0, 4).expect("span"));
    let mut program = Program::default();
    let module_id = program.add_module("mixed_closure");
    let module = program.module_mut(module_id).expect("module");
    let first = module.intern_type(SsaTypeKind::ZeroSized {
        name: "First".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let resource = module.intern_type(SsaTypeKind::ZeroSized {
        name: "Resource".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let last = module.intern_type(SsaTypeKind::ZeroSized {
        name: "Last".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let reference = module
        .add_shared_reference_type(resource)
        .expect("reference");
    let environment = module
        .add_aggregate_type("MixedEnv", vec![first, reference, last])
        .expect("environment");
    let closure = module
        .add_concrete_closure_type(
            "MixedClosure",
            vec![],
            vec![],
            environment,
            vec![
                ClosureCaptureType {
                    mode: ClosureCaptureMode::Owned,
                    ty: first,
                },
                ClosureCaptureType {
                    mode: ClosureCaptureMode::Shared,
                    ty: resource,
                },
                ClosureCaptureType {
                    mode: ClosureCaptureMode::Owned,
                    ty: last,
                },
            ],
        )
        .expect("closure");
    let thunk = module
        .add_function("thunk", vec![], origin.clone())
        .expect("thunk");
    let thunk_entry = module
        .function_mut(thunk)
        .expect("thunk")
        .add_block(
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: environment,
            }],
            origin.clone(),
        )
        .expect("entry");
    module
        .function_mut(thunk)
        .expect("thunk")
        .set_terminator(thunk_entry, TerminatorKind::Abort, origin.clone())
        .expect("abort");
    let main = module
        .add_function("main", vec![], origin.clone())
        .expect("main");
    let function = module.function_mut(main).expect("main");
    let entry = function
        .add_block(
            vec![
                EntityType::Value(first),
                EntityType::Value(resource),
                EntityType::Value(last),
            ],
            origin.clone(),
        )
        .expect("entry");
    let parameters = function.block(entry).expect("entry").parameters.clone();
    let (first_value, owner, last_value) = (
        value(parameters[0]),
        value(parameters[1]),
        value(parameters[2]),
    );
    let EntityId::Place(place) = function
        .append_instruction(
            entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(resource)],
            origin.clone(),
        )
        .expect("place")
        .1[0]
    else {
        panic!("place")
    };
    let EntityId::Loan(loan) = function
        .append_instruction(
            entry,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: resource,
            }],
            origin.clone(),
        )
        .expect("loan")
        .1[0]
    else {
        panic!("loan")
    };
    let closure_value = value(
        function
            .append_instruction(
                entry,
                Operation::ClosureConstruct {
                    closure,
                    thunk,
                    captures: vec![
                        ClosureCaptureOperand::Owned(first_value),
                        ClosureCaptureOperand::Shared(loan),
                        ClosureCaptureOperand::Owned(last_value),
                    ],
                },
                vec![EntityType::Value(closure)],
                origin.clone(),
            )
            .expect("construct")
            .1[0],
    );
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
        .append_instruction(entry, Operation::Drop { owner }, vec![], origin.clone())
        .expect("drop resource");
    function
        .set_terminator(entry, TerminatorKind::Return { values: vec![] }, origin)
        .expect("return");

    let llvm = render_verified_program(&program).expect("mixed closure LLVM must verify");
    let helper = llvm
        .split("define internal void @koven.drop.t5")
        .nth(1)
        .expect("closure drop helper");
    let last_drop = helper
        .find("call void @koven.drop.t2")
        .expect("last owned capture drop");
    let first_drop = helper
        .find("call void @koven.drop.t0")
        .expect("first owned capture drop");
    assert!(last_drop < first_drop);
    assert!(!helper[..first_drop].contains("call void @koven.drop.t1"));
    assert!(!llvm.contains("@malloc"));
    assert!(!llvm.contains("@free"));
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
        .add_block(
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: environment,
            }],
            origin.clone(),
        )
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
