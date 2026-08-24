use lang_frontend::source::SourceMap;

use super::{
    model::{
        BlockId, ClosureCaptureMode, ClosureCaptureOperand, ClosureCaptureType, Edge, EntityId,
        EntityType, Function, FunctionId, LoanKind, ModelError, Module, Operation, Origin,
        Ownership, Program, SsaTypeId, SsaTypeKind, TerminatorKind, ValueId,
    },
    render::render_program,
    verify::{VerifyErrorKind, verify_program},
};

fn origin() -> Origin {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("closure-ssa.ko", "val f = move { value }")
        .expect("test source must be unique");
    Origin::Source(sources.span(source, 0, 5).expect("test span must be valid"))
}

fn add_function(
    module: &mut Module,
    name: &str,
    parameters: &[SsaTypeId],
    returns: Vec<SsaTypeId>,
    origin: &Origin,
) -> (FunctionId, BlockId, Vec<ValueId>) {
    let id = module
        .add_function(name, returns, origin.clone())
        .expect("function signature must be valid");
    let function = module.function_mut(id).expect("function must exist");
    let entry = function
        .add_block(
            parameters.iter().copied().map(EntityType::Value).collect(),
            origin.clone(),
        )
        .expect("entry block must be valid");
    let parameters = function
        .block(entry)
        .expect("entry block must exist")
        .parameters
        .iter()
        .copied()
        .map(value)
        .collect();
    (id, entry, parameters)
}

fn append_values(
    function: &mut Function,
    block: BlockId,
    operation: Operation,
    result_types: &[SsaTypeId],
    origin: &Origin,
) -> Vec<ValueId> {
    function
        .append_instruction(
            block,
            operation,
            result_types
                .iter()
                .copied()
                .map(EntityType::Value)
                .collect(),
            origin.clone(),
        )
        .expect("instruction must be appendable")
        .1
        .into_iter()
        .map(value)
        .collect()
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value, got {entity:?}");
    };
    value
}

fn has_error(program: &Program, expected: impl Fn(&VerifyErrorKind) -> bool) -> bool {
    verify_program(program)
        .expect_err("program must fail verification")
        .errors
        .iter()
        .any(|error| expected(&error.kind))
}

#[test]
fn callable_types_preserve_concrete_capture_identity_and_reject_invalid_shapes() {
    let mut program = Program::default();
    let module_id = program.add_module("closures");
    let foreign_id = program.add_module("foreign");
    let foreign = program
        .module_mut(foreign_id)
        .expect("foreign module must exist")
        .intern_type(SsaTypeKind::Boolean);
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let resource = module.intern_type(SsaTypeKind::Opaque {
        name: "Resource".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let shared_resource = module
        .add_shared_reference_type(resource)
        .expect("shared reference must be local");
    let environment = module
        .add_aggregate_type("CounterEnv", vec![integer, resource])
        .expect("environment must be valid");
    let captures = vec![
        ClosureCaptureType {
            mode: ClosureCaptureMode::Owned,
            ty: integer,
        },
        ClosureCaptureType {
            mode: ClosureCaptureMode::Owned,
            ty: resource,
        },
    ];
    let closure = module
        .add_concrete_closure_type(
            "CounterClosure",
            vec![integer],
            vec![integer],
            environment,
            captures.clone(),
        )
        .expect("closure type must be valid");
    let second = module
        .add_concrete_closure_type(
            "OtherCounterClosure",
            vec![integer],
            vec![integer],
            environment,
            captures,
        )
        .expect("distinct closure provenance must stay distinct");
    let pointer = module
        .add_function_pointer_type(vec![integer], vec![integer])
        .expect("function pointer must be valid");

    assert_ne!(closure, second);
    assert_eq!(module.type_ownership(pointer), Some(Ownership::MoveOnly));
    assert_eq!(module.type_ownership(closure), Some(Ownership::MoveOnly));
    assert_eq!(
        module
            .add_function_pointer_type(vec![integer], vec![integer])
            .expect("same signature must intern"),
        pointer
    );
    assert_eq!(
        module.add_function_pointer_type(vec![foreign], Vec::new()),
        Err(ModelError::WrongTypeOwner {
            expected: module_id,
            actual: foreign_id,
        })
    );
    assert_eq!(
        module.add_function_pointer_type(Vec::new(), vec![integer, integer]),
        Err(ModelError::InvalidCallableReturnArity)
    );
    assert_eq!(
        module.add_concrete_closure_type(
            "EmptyClosure",
            Vec::new(),
            Vec::new(),
            environment,
            Vec::new(),
        ),
        Err(ModelError::EmptyClosureCaptures)
    );
    let shared_environment = module
        .add_aggregate_type("SharedEnv", vec![shared_resource])
        .expect("shared environment must be valid");
    module
        .add_concrete_closure_type(
            "SharedClosure",
            Vec::new(),
            Vec::new(),
            shared_environment,
            vec![ClosureCaptureType {
                mode: ClosureCaptureMode::Shared,
                ty: resource,
            }],
        )
        .expect("shared capture layout must be representable before formation support");

    verify_program(&program).expect("callable type definitions must verify");
    let rendered = render_program(&program);
    assert_eq!(rendered, render_program(&program));
    assert!(rendered.contains("function_pointer (!t0) -> (!t0)"));
    assert!(rendered.contains("closure \"CounterClosure\" env !t3"));
    assert!(rendered.contains("captures (owned !t0, owned !t1) (!t0) -> (!t0)"));
    assert!(rendered.contains("captures (shared !t1) () -> ()"));
}

#[test]
fn verifier_rejects_corrupted_inline_closure_environment_cycle() {
    let mut program = Program::default();
    let module_id = program.add_module("cyclic-closure");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let environment = module
        .add_aggregate_type("Env", vec![integer])
        .expect("environment must be valid");
    let closure = module
        .add_concrete_closure_type(
            "Closure",
            Vec::new(),
            Vec::new(),
            environment,
            vec![ClosureCaptureType {
                mode: ClosureCaptureMode::Owned,
                ty: integer,
            }],
        )
        .expect("closure must initially be valid");
    let SsaTypeKind::Aggregate {
        fields, ownership, ..
    } = &mut module.types[environment.index()]
    else {
        panic!("environment must remain aggregate");
    };
    fields[0] = closure;
    *ownership = Ownership::MoveOnly;

    assert!(has_error(&program, |kind| matches!(
        kind,
        VerifyErrorKind::InvalidTypeDefinition { reason }
            if *reason == "closure environment must not form an inline cycle"
    )));
}

#[test]
fn function_address_owned_closure_invoke_and_drop_verify_together() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("closure-operations");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let resource = module.intern_type(SsaTypeKind::Opaque {
        name: "Resource".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let pointer = module
        .add_function_pointer_type(vec![integer], vec![integer])
        .expect("function pointer must be valid");
    let copy_environment = module
        .add_aggregate_type("CopyEnv", vec![integer])
        .expect("copy environment must be valid");
    let copy_closure = module
        .add_concrete_closure_type(
            "CopyClosure",
            vec![integer],
            vec![integer],
            copy_environment,
            vec![ClosureCaptureType {
                mode: ClosureCaptureMode::Owned,
                ty: integer,
            }],
        )
        .expect("copy closure must be valid");
    let move_environment = module
        .add_aggregate_type("MoveEnv", vec![resource])
        .expect("move environment must be valid");
    let move_closure = module
        .add_concrete_closure_type(
            "MoveClosure",
            Vec::new(),
            Vec::new(),
            move_environment,
            vec![ClosureCaptureType {
                mode: ClosureCaptureMode::Owned,
                ty: resource,
            }],
        )
        .expect("move closure must be valid");

    let (identity, identity_entry, identity_parameters) =
        add_function(module, "identity", &[integer], vec![integer], &origin);
    module
        .function_mut(identity)
        .expect("identity must exist")
        .set_terminator(
            identity_entry,
            TerminatorKind::Return {
                values: vec![identity_parameters[0]],
            },
            origin.clone(),
        )
        .expect("identity must return");

    let (copy_thunk, copy_entry, copy_parameters) = add_function(
        module,
        "copy_thunk",
        &[copy_environment, integer],
        vec![integer],
        &origin,
    );
    module
        .function_mut(copy_thunk)
        .expect("copy thunk must exist")
        .set_terminator(
            copy_entry,
            TerminatorKind::Return {
                values: vec![copy_parameters[1]],
            },
            origin.clone(),
        )
        .expect("copy thunk must return");

    let (move_thunk, move_entry, _) = add_function(
        module,
        "move_thunk",
        &[move_environment],
        Vec::new(),
        &origin,
    );
    module
        .function_mut(move_thunk)
        .expect("move thunk must exist")
        .set_terminator(move_entry, TerminatorKind::Abort, origin.clone())
        .expect("abort path needs no environment ownership discharge");

    let (main, entry, parameters) =
        add_function(module, "main", &[integer, resource], Vec::new(), &origin);
    let integer_value = parameters[0];
    let resource_value = parameters[1];
    let function = module.function_mut(main).expect("main must exist");
    let address = append_values(
        function,
        entry,
        Operation::FunctionAddress { target: identity },
        &[pointer],
        &origin,
    )[0];
    let _shared = append_values(
        function,
        entry,
        Operation::CallableInvoke {
            callable: address,
            arguments: vec![integer_value],
        },
        &[integer],
        &origin,
    );
    append_values(
        function,
        entry,
        Operation::CallableInvoke {
            callable: address,
            arguments: vec![integer_value],
        },
        &[integer],
        &origin,
    );
    let copied = append_values(
        function,
        entry,
        Operation::ClosureConstruct {
            closure: copy_closure,
            thunk: copy_thunk,
            captures: vec![ClosureCaptureOperand::Owned(integer_value)],
        },
        &[copy_closure],
        &origin,
    )[0];
    append_values(
        function,
        entry,
        Operation::CallableInvoke {
            callable: copied,
            arguments: vec![integer_value],
        },
        &[integer],
        &origin,
    );
    append_values(
        function,
        entry,
        Operation::CallableInvoke {
            callable: copied,
            arguments: vec![integer_value],
        },
        &[integer],
        &origin,
    );
    let moved = append_values(
        function,
        entry,
        Operation::ClosureConstruct {
            closure: move_closure,
            thunk: move_thunk,
            captures: vec![ClosureCaptureOperand::Owned(resource_value)],
        },
        &[move_closure],
        &origin,
    )[0];
    for owner in [address, copied, moved] {
        append_values(function, entry, Operation::Drop { owner }, &[], &origin);
    }
    function
        .set_terminator(
            entry,
            TerminatorKind::Return { values: Vec::new() },
            origin.clone(),
        )
        .expect("main must return");

    verify_program(&program).expect("owned closure operations must verify");
    let rendered = render_program(&program);
    assert!(rendered.contains("function_address @f0"));
    assert!(rendered.contains("closure.construct !t4, @f1(owned %v0)"));
    assert!(rendered.matches("invoke %v2(%v0)").count() == 2);
}

#[test]
fn shared_capture_loan_follows_closure_across_edge_and_ends_on_drop() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("shared-closure");
    let module = program.module_mut(module_id).expect("module must exist");
    let resource = module.intern_type(SsaTypeKind::ZeroSized {
        name: "Resource".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let shared_ref = module
        .add_shared_reference_type(resource)
        .expect("shared ref");
    let environment = module
        .add_aggregate_type("SharedEnv", vec![shared_ref])
        .expect("env");
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
    let (thunk, thunk_entry, _) = add_function(module, "thunk", &[environment], vec![], &origin);
    module
        .function_mut(thunk)
        .expect("thunk")
        .set_terminator(
            thunk_entry,
            TerminatorKind::Return { values: vec![] },
            origin.clone(),
        )
        .expect("return");
    let (main, entry, parameters) = add_function(module, "main", &[resource], vec![], &origin);
    let function = module.function_mut(main).expect("main");
    let owner = parameters[0];
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
    let closure_value = append_values(
        function,
        entry,
        Operation::ClosureConstruct {
            closure,
            thunk,
            captures: vec![ClosureCaptureOperand::Shared(loan)],
        },
        &[closure],
        &origin,
    )[0];
    let continuation = function
        .add_block(
            vec![
                EntityType::Value(closure),
                EntityType::Value(resource),
                EntityType::Place(resource),
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: resource,
                },
            ],
            origin.clone(),
        )
        .expect("continuation");
    function
        .set_terminator(
            entry,
            TerminatorKind::Branch(Edge {
                target: continuation,
                arguments: vec![
                    EntityId::Value(closure_value),
                    EntityId::Value(owner),
                    EntityId::Place(place),
                    EntityId::Loan(loan),
                ],
            }),
            origin.clone(),
        )
        .expect("branch");
    let parameters = function
        .block(continuation)
        .expect("continuation")
        .parameters
        .clone();
    let (
        EntityId::Value(closure_value),
        EntityId::Value(owner),
        EntityId::Place(_place),
        EntityId::Loan(_loan),
    ) = (parameters[0], parameters[1], parameters[2], parameters[3])
    else {
        panic!("parameter kinds")
    };
    append_values(
        function,
        continuation,
        Operation::CallableInvoke {
            callable: closure_value,
            arguments: vec![],
        },
        &[],
        &origin,
    );
    append_values(
        function,
        continuation,
        Operation::Drop {
            owner: closure_value,
        },
        &[],
        &origin,
    );
    append_values(
        function,
        continuation,
        Operation::Drop { owner },
        &[],
        &origin,
    );
    function
        .set_terminator(
            continuation,
            TerminatorKind::Return { values: vec![] },
            origin,
        )
        .expect("return");

    verify_program(&program).expect("shared closure loan must transfer and end with closure drop");
}

#[test]
fn shared_capture_rejects_explicit_loan_end_before_closure_drop() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("early-end");
    let module = program.module_mut(module_id).expect("module");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let reference = module
        .add_shared_reference_type(integer)
        .expect("reference");
    let environment = module
        .add_aggregate_type("Env", vec![reference])
        .expect("env");
    let closure = module
        .add_concrete_closure_type(
            "Borrowed",
            vec![],
            vec![],
            environment,
            vec![ClosureCaptureType {
                mode: ClosureCaptureMode::Shared,
                ty: integer,
            }],
        )
        .expect("closure");
    let (thunk, thunk_entry, _) = add_function(module, "thunk", &[environment], vec![], &origin);
    module
        .function_mut(thunk)
        .expect("thunk")
        .set_terminator(
            thunk_entry,
            TerminatorKind::Return { values: vec![] },
            origin.clone(),
        )
        .expect("return");
    let (main, entry, parameters) = add_function(module, "main", &[integer], vec![], &origin);
    let function = module.function_mut(main).expect("main");
    let EntityId::Place(place) = function
        .append_instruction(
            entry,
            Operation::RootPlace {
                owner: parameters[0],
            },
            vec![EntityType::Place(integer)],
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
                target: integer,
            }],
            origin.clone(),
        )
        .expect("loan")
        .1[0]
    else {
        panic!("loan")
    };
    append_values(
        function,
        entry,
        Operation::ClosureConstruct {
            closure,
            thunk,
            captures: vec![ClosureCaptureOperand::Shared(loan)],
        },
        &[closure],
        &origin,
    );
    function
        .append_instruction(entry, Operation::BorrowEnd { loan }, vec![], origin.clone())
        .expect("end");
    function
        .set_terminator(entry, TerminatorKind::Return { values: vec![] }, origin)
        .expect("return");

    assert!(has_error(&program, |kind| matches!(
        kind,
        VerifyErrorKind::OwnerLoanConflict { .. }
    )));
    assert!(has_error(&program, |kind| matches!(
        kind,
        VerifyErrorKind::MissingOwnedExit { .. }
    )));
    assert!(has_error(&program, |kind| matches!(
        kind,
        VerifyErrorKind::ActiveLoanAtExit { .. }
    )));
}

#[test]
fn wrong_thunk_shared_formation_and_move_after_capture_fail_before_llvm() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("invalid-closures");
    let module = program.module_mut(module_id).expect("module must exist");
    let resource = module.intern_type(SsaTypeKind::Opaque {
        name: "Resource".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let environment = module
        .add_aggregate_type("MoveEnv", vec![resource])
        .expect("environment must be valid");
    let closure = module
        .add_concrete_closure_type(
            "MoveClosure",
            Vec::new(),
            Vec::new(),
            environment,
            vec![ClosureCaptureType {
                mode: ClosureCaptureMode::Owned,
                ty: resource,
            }],
        )
        .expect("closure must be valid");
    let shared_ref = module
        .add_shared_reference_type(resource)
        .expect("shared reference must be valid");
    let shared_environment = module
        .add_aggregate_type("SharedEnv", vec![shared_ref])
        .expect("shared environment must be valid");
    let shared_closure = module
        .add_concrete_closure_type(
            "SharedClosure",
            Vec::new(),
            Vec::new(),
            shared_environment,
            vec![ClosureCaptureType {
                mode: ClosureCaptureMode::Shared,
                ty: resource,
            }],
        )
        .expect("shared closure type must be valid");
    let (wrong_thunk, wrong_entry, _) =
        add_function(module, "wrong_thunk", &[], Vec::new(), &origin);
    module
        .function_mut(wrong_thunk)
        .expect("wrong thunk must exist")
        .set_terminator(wrong_entry, TerminatorKind::Abort, origin.clone())
        .expect("wrong thunk must terminate");
    let (main, entry, parameters) = add_function(module, "main", &[resource], Vec::new(), &origin);
    let resource_value = parameters[0];
    let function = module.function_mut(main).expect("main must exist");
    let root = function
        .append_instruction(
            entry,
            Operation::RootPlace {
                owner: resource_value,
            },
            vec![EntityType::Place(resource)],
            origin.clone(),
        )
        .expect("root place must append")
        .1[0];
    let EntityId::Place(root) = root else {
        panic!("root place result must be a place");
    };
    let loan = function
        .append_instruction(
            entry,
            Operation::BorrowBegin {
                place: root,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: resource,
            }],
            origin.clone(),
        )
        .expect("borrow must append")
        .1[0];
    let EntityId::Loan(loan) = loan else {
        panic!("borrow result must be a loan");
    };
    append_values(
        function,
        entry,
        Operation::ClosureConstruct {
            closure,
            thunk: wrong_thunk,
            captures: vec![ClosureCaptureOperand::Owned(resource_value)],
        },
        &[closure],
        &origin,
    );
    append_values(
        function,
        entry,
        Operation::ClosureConstruct {
            closure: shared_closure,
            thunk: wrong_thunk,
            captures: vec![ClosureCaptureOperand::Shared(loan)],
        },
        &[shared_closure],
        &origin,
    );
    function
        .append_instruction(entry, Operation::BorrowEnd { loan }, vec![], origin.clone())
        .expect("borrow end must append");
    function
        .set_terminator(entry, TerminatorKind::Abort, origin.clone())
        .expect("invalid function still needs a terminator");

    assert!(has_error(&program, |kind| matches!(
        kind,
        VerifyErrorKind::OperationContract { .. }
    )));

    let mut moved_program = Program::default();
    let moved_module_id = moved_program.add_module("moved-capture");
    let moved_module = moved_program
        .module_mut(moved_module_id)
        .expect("module must exist");
    let moved_resource = moved_module.intern_type(SsaTypeKind::Opaque {
        name: "Resource".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let moved_environment = moved_module
        .add_aggregate_type("MoveEnv", vec![moved_resource])
        .expect("environment must be valid");
    let moved_closure = moved_module
        .add_concrete_closure_type(
            "MoveClosure",
            Vec::new(),
            Vec::new(),
            moved_environment,
            vec![ClosureCaptureType {
                mode: ClosureCaptureMode::Owned,
                ty: moved_resource,
            }],
        )
        .expect("closure must be valid");
    let (valid_thunk, valid_entry, _) = add_function(
        moved_module,
        "valid_thunk",
        &[moved_environment],
        Vec::new(),
        &origin,
    );
    moved_module
        .function_mut(valid_thunk)
        .expect("valid thunk must exist")
        .set_terminator(valid_entry, TerminatorKind::Abort, origin.clone())
        .expect("valid thunk must terminate");
    let (moved_main, moved_entry, moved_parameters) =
        add_function(moved_module, "main", &[moved_resource], Vec::new(), &origin);
    let moved_source = moved_parameters[0];
    let moved_function = moved_module
        .function_mut(moved_main)
        .expect("main must exist");
    let closure_value = append_values(
        moved_function,
        moved_entry,
        Operation::ClosureConstruct {
            closure: moved_closure,
            thunk: valid_thunk,
            captures: vec![ClosureCaptureOperand::Owned(moved_source)],
        },
        &[moved_closure],
        &origin,
    )[0];
    append_values(
        moved_function,
        moved_entry,
        Operation::Consume {
            owner: moved_source,
        },
        &[],
        &origin,
    );
    append_values(
        moved_function,
        moved_entry,
        Operation::Drop {
            owner: closure_value,
        },
        &[],
        &origin,
    );
    moved_function
        .set_terminator(
            moved_entry,
            TerminatorKind::Return { values: Vec::new() },
            origin,
        )
        .expect("main must return");

    assert!(has_error(&moved_program, |kind| matches!(
        kind,
        VerifyErrorKind::ValueUnavailable { value } if *value == moved_source
    )));
}
