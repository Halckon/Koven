//! Borrowed direct/instance/function-pointer calls, ABI and rejection contracts.

use super::*;

#[test]
fn direct_borrow_call_preserves_move_only_owner_and_uses_pointer_abi() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("borrow-call");
    let module = program.module_mut(module_id).expect("module must exist");
    let payload = module
        .add_aggregate_type("Resource.payload", Vec::new())
        .expect("payload must be valid");
    let owner = module
        .declare_heap_owner("Resource")
        .expect("owner must be valid");
    module
        .define_heap_owner(owner, payload)
        .expect("owner definition must be valid");

    let callee = module
        .add_function("inspect", Vec::new(), origin.clone())
        .expect("callee must be valid");
    let callee_function = module.function_mut(callee).expect("callee must exist");
    let callee_entry = callee_function
        .add_block(
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: owner,
            }],
            origin.clone(),
        )
        .expect("borrow parameter must be valid");
    callee_function
        .set_terminator(
            callee_entry,
            TerminatorKind::Return { values: Vec::new() },
            origin.clone(),
        )
        .expect("callee must return");

    let caller = module
        .add_function("caller", Vec::new(), origin.clone())
        .expect("caller must be valid");
    let caller_function = module.function_mut(caller).expect("caller must exist");
    let entry = caller_function
        .add_block(vec![EntityType::Value(owner)], origin.clone())
        .expect("caller entry must be valid");
    let owner_value = value(caller_function.block(entry).expect("entry").parameters[0]);
    let (_, places) = caller_function
        .append_instruction(
            entry,
            Operation::RootPlace { owner: owner_value },
            vec![EntityType::Place(owner)],
            origin.clone(),
        )
        .expect("root place must append");
    let (_, loans) = caller_function
        .append_instruction(
            entry,
            Operation::BorrowBegin {
                place: place(places[0]),
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: owner,
            }],
            origin.clone(),
        )
        .expect("borrow must append");
    let loan = loan(loans[0]);
    caller_function
        .append_instruction(
            entry,
            Operation::DirectCall {
                callee,
                receiver: None,
                arguments: vec![EntityId::Loan(loan)],
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("borrow call must append");
    caller_function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan },
            Vec::new(),
            origin.clone(),
        )
        .expect("borrow end must append");
    caller_function
        .append_instruction(
            entry,
            Operation::Drop { owner: owner_value },
            Vec::new(),
            origin.clone(),
        )
        .expect("owner must remain available after borrow call");
    caller_function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("caller must return");

    verify_program(&program).expect("borrow call must not consume its MoveOnly owner");
    let rendered = render_program(&program);
    assert_diagnostic!(rendered.contains("call @f0(%l0)"), "{rendered}");
    let llvm = render_verified_program(&program).expect("borrow call must lower to LLVM");
    assert_diagnostic!(
        llvm.contains("define internal void @f0.inspect(ptr %l0)"),
        "{llvm}"
    );
    assert_diagnostic!(llvm.contains("call void @f0.inspect(ptr %p0)"), "{llvm}");
    assert_eq_diagnostic!(llvm.matches("call void @free").count(), 1, "{llvm}");
}

#[test]
fn instance_direct_call_models_receiver_before_explicit_arguments() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("instance-receiver");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let receiver_owner = module
        .declare_heap_owner("Receiver")
        .expect("receiver owner must be valid");
    let payload = module
        .add_aggregate_type("Receiver.payload", Vec::new())
        .expect("receiver payload must be valid");
    module
        .define_heap_owner(receiver_owner, payload)
        .expect("receiver owner must be defined");
    let receiver_type = EntityType::Loan {
        kind: LoanKind::Shared,
        target: receiver_owner,
    };

    let callee = module
        .add_instance_function("inspect", receiver_type, Vec::new(), origin.clone())
        .expect("instance callee must be valid");
    let callee_function = module.function_mut(callee).expect("callee must exist");
    let callee_entry = callee_function
        .add_block(
            vec![receiver_type, EntityType::Value(integer)],
            origin.clone(),
        )
        .expect("callee entry must be valid");
    callee_function
        .set_terminator(
            callee_entry,
            TerminatorKind::Return { values: Vec::new() },
            origin.clone(),
        )
        .expect("callee must return");

    let caller = module
        .add_function("caller", Vec::new(), origin.clone())
        .expect("caller must be valid");
    let caller_function = module.function_mut(caller).expect("caller must exist");
    let caller_entry = caller_function
        .add_block(
            vec![
                EntityType::Value(receiver_owner),
                EntityType::Value(integer),
            ],
            origin.clone(),
        )
        .expect("caller entry must be valid");
    let parameters = caller_function
        .block(caller_entry)
        .expect("caller entry must exist")
        .parameters
        .clone();
    let owner = value(parameters[0]);
    let argument = value(parameters[1]);
    let (_, places) = caller_function
        .append_instruction(
            caller_entry,
            Operation::RootPlace { owner },
            vec![EntityType::Place(receiver_owner)],
            origin.clone(),
        )
        .expect("receiver place must append");
    let (_, loans) = caller_function
        .append_instruction(
            caller_entry,
            Operation::BorrowBegin {
                place: place(places[0]),
                kind: LoanKind::Shared,
            },
            vec![receiver_type],
            origin.clone(),
        )
        .expect("receiver loan must append");
    let receiver = loan(loans[0]);
    caller_function
        .append_instruction(
            caller_entry,
            Operation::DirectCall {
                callee,
                receiver: Some(EntityId::Loan(receiver)),
                arguments: vec![EntityId::Value(argument)],
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("instance call must append");
    caller_function
        .append_instruction(
            caller_entry,
            Operation::BorrowEnd { loan: receiver },
            Vec::new(),
            origin.clone(),
        )
        .expect("receiver loan must end");
    caller_function
        .append_instruction(
            caller_entry,
            Operation::Drop { owner },
            Vec::new(),
            origin.clone(),
        )
        .expect("receiver owner must drop");
    caller_function
        .set_terminator(
            caller_entry,
            TerminatorKind::Return { values: Vec::new() },
            origin,
        )
        .expect("caller must return");

    verify_program(&program).expect("matching instance receiver must verify");
    let rendered = render_program(&program);
    assert_diagnostic!(
        rendered.contains("call @f0(receiver %l0; %v1)"),
        "{rendered}"
    );
    assert_diagnostic!(
        rendered.contains("func \"inspect\"(receiver "),
        "{rendered}"
    );
    let llvm = render_verified_program(&program).expect("instance receiver must lower to LLVM");
    assert_diagnostic!(
        llvm.contains("call void @f0.inspect(ptr %p0, i32 %v1)"),
        "{llvm}"
    );

    let Operation::DirectCall {
        receiver: call_receiver,
        ..
    } = &mut program.modules[0].functions[1].instructions[2].operation
    else {
        panic!("expected instance direct call");
    };
    *call_receiver = None;
    assert!(
        verify_program(&program)
            .expect_err("missing receiver must fail")
            .errors
            .iter()
            .any(|error| matches!(error.kind, VerifyErrorKind::OperationContract { .. }))
    );

    let Operation::DirectCall {
        receiver: call_receiver,
        ..
    } = &mut program.modules[0].functions[1].instructions[2].operation
    else {
        panic!("expected instance direct call");
    };
    *call_receiver = Some(EntityId::Loan(receiver));
    program.modules[0].functions[0].receiver = Some(EntityType::Loan {
        kind: LoanKind::Exclusive,
        target: receiver_owner,
    });
    assert!(
        verify_program(&program)
            .expect_err("receiver contract and entry mode mismatch must fail")
            .errors
            .iter()
            .any(|error| matches!(error.kind, VerifyErrorKind::OperationContract { .. }))
    );
}

#[test]
fn function_pointer_borrow_invoke_preserves_loan_delivery_and_pointer_abi() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("borrow-function-pointer");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let target = module
        .add_function("inspect", vec![integer], origin.clone())
        .expect("target");
    let target_function = module.function_mut(target).expect("target function");
    let target_entry = target_function
        .add_block(
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: integer,
            }],
            origin.clone(),
        )
        .expect("target entry");
    let target_loan = loan(
        target_function
            .block(target_entry)
            .expect("target entry")
            .parameters[0],
    );
    let (_, read) = target_function
        .append_instruction(
            target_entry,
            Operation::Read {
                source: crate::ssa::model::PlaceAccess::Loan(target_loan),
            },
            vec![EntityType::Value(integer)],
            origin.clone(),
        )
        .expect("target read");
    target_function
        .set_terminator(
            target_entry,
            TerminatorKind::Return {
                values: vec![value(read[0])],
            },
            origin.clone(),
        )
        .expect("target return");

    let pointer = module
        .add_function_pointer_type_with_parameters(
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: integer,
            }],
            vec![integer],
        )
        .expect("Borrow function pointer type");
    let caller = module
        .add_function("caller", vec![integer], origin.clone())
        .expect("caller");
    let caller_function = module.function_mut(caller).expect("caller function");
    let entry = caller_function
        .add_block(vec![EntityType::Value(integer)], origin.clone())
        .expect("caller entry");
    let input = value(caller_function.block(entry).expect("entry").parameters[0]);
    let (_, callable) = caller_function
        .append_instruction(
            entry,
            Operation::FunctionAddress { target },
            vec![EntityType::Value(pointer)],
            origin.clone(),
        )
        .expect("function address");
    let callable = value(callable[0]);
    let (_, places) = caller_function
        .append_instruction(
            entry,
            Operation::RootPlace { owner: input },
            vec![EntityType::Place(integer)],
            origin.clone(),
        )
        .expect("root place");
    let (_, loans) = caller_function
        .append_instruction(
            entry,
            Operation::BorrowBegin {
                place: place(places[0]),
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: integer,
            }],
            origin.clone(),
        )
        .expect("loan");
    let input_loan = loan(loans[0]);
    let (_, result) = caller_function
        .append_instruction(
            entry,
            Operation::CallableInvoke {
                callable,
                arguments: vec![EntityId::Loan(input_loan)],
            },
            vec![EntityType::Value(integer)],
            origin.clone(),
        )
        .expect("Borrow invoke");
    caller_function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan: input_loan },
            Vec::new(),
            origin.clone(),
        )
        .expect("loan end");
    caller_function
        .append_instruction(
            entry,
            Operation::Drop { owner: callable },
            Vec::new(),
            origin.clone(),
        )
        .expect("function pointer drop");
    caller_function
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: vec![value(result[0])],
            },
            origin,
        )
        .expect("caller return");

    verify_program(&program).expect("Borrow function pointer invoke must verify");
    let rendered = render_program(&program);
    assert_diagnostic!(
        rendered.contains("function_pointer (loan.shared !t0) -> (!t0)"),
        "{rendered}"
    );
    assert_diagnostic!(rendered.contains("invoke %v1(%l0)"), "{rendered}");
    let llvm = render_verified_program(&program).expect("Borrow invoke must lower to LLVM");
    assert_diagnostic!(
        llvm.contains("define internal i32 @f0.inspect(ptr %l0)"),
        "{llvm}"
    );
    assert_diagnostic!(llvm.contains("call i32 @f0.inspect(ptr %p0)"), "{llvm}");
}

#[test]
fn function_pointer_rejects_borrow_signature_and_invoke_mode_mismatches() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("invalid-borrow-function-pointer");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let target = module
        .add_function("value_target", Vec::new(), origin.clone())
        .expect("target");
    let target_function = module.function_mut(target).expect("target function");
    let target_entry = target_function
        .add_block(vec![EntityType::Value(integer)], origin.clone())
        .expect("target entry");
    target_function
        .set_terminator(
            target_entry,
            TerminatorKind::Return { values: Vec::new() },
            origin.clone(),
        )
        .expect("target return");
    let pointer = module
        .add_function_pointer_type_with_parameters(
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: integer,
            }],
            Vec::new(),
        )
        .expect("Borrow pointer type");
    let caller = module
        .add_function("caller", Vec::new(), origin.clone())
        .expect("caller");
    let caller_function = module.function_mut(caller).expect("caller function");
    let entry = caller_function
        .add_block(vec![EntityType::Value(integer)], origin.clone())
        .expect("caller entry");
    let input = value(caller_function.block(entry).expect("entry").parameters[0]);
    let (_, callable) = caller_function
        .append_instruction(
            entry,
            Operation::FunctionAddress { target },
            vec![EntityType::Value(pointer)],
            origin.clone(),
        )
        .expect("invalid address remains appendable");
    let callable = value(callable[0]);
    caller_function
        .append_instruction(
            entry,
            Operation::CallableInvoke {
                callable,
                arguments: vec![EntityId::Value(input)],
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("invalid invoke remains appendable");
    caller_function
        .append_instruction(
            entry,
            Operation::Drop { owner: callable },
            Vec::new(),
            origin.clone(),
        )
        .expect("pointer drop");
    caller_function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("caller return");

    let errors = verify_program(&program).expect_err("Borrow mode mismatches must fail");
    assert_eq_diagnostic!(
        errors
            .errors
            .iter()
            .filter(|error| matches!(error.kind, VerifyErrorKind::OperationContract { .. }))
            .count(),
        2,
        "{errors:#?}"
    );
}

#[test]
fn direct_call_rejects_value_for_borrow_and_wrong_loan_kind() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("invalid-borrow-call");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let shared = module
        .add_function("shared", Vec::new(), origin.clone())
        .expect("shared callee");
    let shared_function = module.function_mut(shared).expect("shared function");
    let shared_entry = shared_function
        .add_block(
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: integer,
            }],
            origin.clone(),
        )
        .expect("shared entry");
    shared_function
        .set_terminator(
            shared_entry,
            TerminatorKind::Return { values: Vec::new() },
            origin.clone(),
        )
        .expect("shared return");
    let exclusive = module
        .add_function("exclusive", Vec::new(), origin.clone())
        .expect("exclusive callee");
    let exclusive_function = module.function_mut(exclusive).expect("exclusive function");
    let exclusive_entry = exclusive_function
        .add_block(
            vec![EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: integer,
            }],
            origin.clone(),
        )
        .expect("exclusive entry");
    exclusive_function
        .set_terminator(
            exclusive_entry,
            TerminatorKind::Return { values: Vec::new() },
            origin.clone(),
        )
        .expect("exclusive return");

    let caller = module
        .add_function("caller", Vec::new(), origin.clone())
        .expect("caller");
    let caller_function = module.function_mut(caller).expect("caller function");
    let entry = caller_function
        .add_block(vec![EntityType::Value(integer)], origin.clone())
        .expect("caller entry");
    let input = value(caller_function.block(entry).expect("entry").parameters[0]);
    let (_, places) = caller_function
        .append_instruction(
            entry,
            Operation::RootPlace { owner: input },
            vec![EntityType::Place(integer)],
            origin.clone(),
        )
        .expect("root place");
    let (_, loans) = caller_function
        .append_instruction(
            entry,
            Operation::BorrowBegin {
                place: place(places[0]),
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: integer,
            }],
            origin.clone(),
        )
        .expect("shared loan");
    let shared_loan = loan(loans[0]);
    for (callee, argument) in [
        (shared, EntityId::Value(input)),
        (exclusive, EntityId::Loan(shared_loan)),
    ] {
        caller_function
            .append_instruction(
                entry,
                Operation::DirectCall {
                    callee,
                    receiver: None,
                    arguments: vec![argument],
                },
                Vec::new(),
                origin.clone(),
            )
            .expect("invalid call remains structurally appendable");
    }
    caller_function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan: shared_loan },
            Vec::new(),
            origin.clone(),
        )
        .expect("loan end");
    caller_function
        .set_terminator(entry, TerminatorKind::Abort, origin)
        .expect("caller abort");

    let errors = verify_program(&program).expect_err("delivery mode mismatches must fail");
    assert_eq_diagnostic!(
        errors
            .errors
            .iter()
            .filter(|error| matches!(error.kind, VerifyErrorKind::OperationContract { .. }))
            .count(),
        2,
        "{errors:#?}"
    );
}

#[test]
fn direct_call_rejects_inactive_borrow_argument() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("inactive-borrow-call");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let callee = module
        .add_function("inspect", Vec::new(), origin.clone())
        .expect("callee");
    let callee_function = module.function_mut(callee).expect("callee function");
    let callee_entry = callee_function
        .add_block(
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: integer,
            }],
            origin.clone(),
        )
        .expect("callee entry");
    callee_function
        .set_terminator(
            callee_entry,
            TerminatorKind::Return { values: Vec::new() },
            origin.clone(),
        )
        .expect("callee return");

    let caller = module
        .add_function("caller", Vec::new(), origin.clone())
        .expect("caller");
    let caller_function = module.function_mut(caller).expect("caller function");
    let entry = caller_function
        .add_block(vec![EntityType::Value(integer)], origin.clone())
        .expect("caller entry");
    let input = value(caller_function.block(entry).expect("entry").parameters[0]);
    let (_, places) = caller_function
        .append_instruction(
            entry,
            Operation::RootPlace { owner: input },
            vec![EntityType::Place(integer)],
            origin.clone(),
        )
        .expect("root place");
    let (_, loans) = caller_function
        .append_instruction(
            entry,
            Operation::BorrowBegin {
                place: place(places[0]),
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: integer,
            }],
            origin.clone(),
        )
        .expect("shared loan");
    let shared_loan = loan(loans[0]);
    caller_function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan: shared_loan },
            Vec::new(),
            origin.clone(),
        )
        .expect("loan end");
    caller_function
        .append_instruction(
            entry,
            Operation::DirectCall {
                callee,
                receiver: None,
                arguments: vec![EntityId::Loan(shared_loan)],
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("inactive call remains structurally appendable");
    caller_function
        .set_terminator(entry, TerminatorKind::Abort, origin)
        .expect("caller abort");

    let errors = verify_program(&program).expect_err("inactive loan call must fail");
    assert_diagnostic!(
        errors
            .errors
            .iter()
            .any(|error| matches!(error.kind, VerifyErrorKind::LoanInactive { loan } if loan == shared_loan)),
        "{errors:#?}"
    );
}
