use super::*;
use crate::ssa::{
    model::{BlockId, Edge, Function, InstructionId, PlaceAccess, PlaceId},
    verify::{VerifyErrorKind, VerifyLocation},
};

#[derive(Clone, Copy, Debug)]
enum Route {
    Local,
    Reborrow,
    Helper,
    BorrowHelper,
    Captured,
    SeparateSlot,
    Edge,
}

fn append(
    function: &mut Function,
    block: BlockId,
    operation: Operation,
    results: Vec<EntityType>,
    at: &Origin,
) -> (InstructionId, Vec<EntityId>) {
    function
        .append_instruction(block, operation, results, at.clone())
        .expect("operation")
}

fn root(
    function: &mut Function,
    block: BlockId,
    owner: ValueId,
    ty: SsaTypeId,
    at: &Origin,
) -> PlaceId {
    let EntityId::Place(place) = append(
        function,
        block,
        Operation::RootPlace { owner },
        vec![EntityType::Place(ty)],
        at,
    )
    .1[0] else {
        panic!("root place");
    };
    place
}

fn borrow(
    function: &mut Function,
    block: BlockId,
    place: PlaceId,
    target: SsaTypeId,
    at: &Origin,
) -> LoanId {
    loan(
        &append(
            function,
            block,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![view_loan(target)],
            at,
        )
        .1,
    )
}

fn captured_view_thunk(module: &mut Module, types: &Types, at: &Origin) -> (SsaTypeId, FunctionId) {
    let reference = module
        .add_shared_reference_type(types.view)
        .expect("view reference");
    let environment = module
        .add_aggregate_type("CapturedView", vec![reference])
        .expect("env");
    let wrapper = module
        .add_concrete_closure_type(
            "InvokeCapturedView",
            vec![types.int],
            vec![types.int],
            environment,
            vec![ClosureCaptureType {
                mode: ClosureCaptureMode::Shared,
                ty: types.view,
            }],
        )
        .expect("wrapper");
    let thunk = module
        .add_function("invoke_captured_view", vec![types.int], at.clone())
        .expect("thunk");
    let function = module.function_mut(thunk).expect("thunk");
    let entry = function
        .add_block(
            vec![view_loan(environment), EntityType::Value(types.int)],
            at.clone(),
        )
        .expect("entry");
    let parameters = &function.block(entry).expect("entry").parameters;
    let (env, argument) = (loan(&parameters[..1]), parameters[1]);
    let slot = loan(
        &append(
            function,
            entry,
            Operation::SharedFieldLoan {
                base: env,
                field: 0,
            },
            vec![view_loan(reference)],
            at,
        )
        .1,
    );
    let view = loan(
        &append(
            function,
            entry,
            Operation::SharedReferenceFollow { source: slot },
            vec![view_loan(types.view)],
            at,
        )
        .1,
    );
    let result = append(
        function,
        entry,
        Operation::CallableInvokeView {
            view,
            arguments: vec![argument],
        },
        vec![EntityType::Value(types.int)],
        at,
    )
    .1;
    function
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: values(&result),
            },
            at.clone(),
        )
        .expect("return");
    (wrapper, thunk)
}

fn borrow_helper(module: &mut Module, types: &Types, at: &Origin) -> FunctionId {
    let helper = module
        .add_function("apply_borrow", vec![types.int], at.clone())
        .expect("helper");
    let function = module.function_mut(helper).expect("helper");
    function.borrow_return = Some(1);
    let entry = function
        .add_block(
            vec![
                view_loan(types.view),
                view_loan(types.int),
                EntityType::Value(types.int),
            ],
            at.clone(),
        )
        .expect("entry");
    let parameters = &function.block(entry).expect("entry").parameters;
    let (view, source, argument) = (
        loan(&parameters[..1]),
        loan(&parameters[1..2]),
        parameters[2],
    );
    append(
        function,
        entry,
        Operation::CallableInvokeView {
            view,
            arguments: vec![argument],
        },
        vec![EntityType::Value(types.int)],
        at,
    );
    function
        .set_terminator(
            entry,
            TerminatorKind::BorrowReturn { loan: source },
            at.clone(),
        )
        .expect("return");
    helper
}

fn program(route: Route, end_early: bool) -> (Program, LoanId, InstructionId) {
    let at = origin();
    let (mut program, types) = module_with_types();
    let module = &mut program.modules[0];
    let helper = apply(module, &types, view_loan(types.view));
    let borrow_helper =
        matches!(route, Route::BorrowHelper).then(|| borrow_helper(module, &types, &at));
    let wrapper =
        matches!(route, Route::Captured).then(|| captured_view_thunk(module, &types, &at));
    let reference = module
        .add_shared_reference_type(types.int)
        .expect("reference");
    let environment = module
        .add_aggregate_type("CapturedInt", vec![reference])
        .expect("env");
    let closure = module
        .add_concrete_closure_type(
            "ReadInt",
            vec![types.int],
            vec![types.int],
            environment,
            vec![ClosureCaptureType {
                mode: ClosureCaptureMode::Shared,
                ty: types.int,
            }],
        )
        .expect("closure");
    let thunk = module
        .add_function("read", vec![types.int], at.clone())
        .expect("thunk");
    let function = module.function_mut(thunk).expect("thunk");
    let entry = function
        .add_block(
            vec![view_loan(environment), EntityType::Value(types.int)],
            at.clone(),
        )
        .expect("entry");
    let env = loan(&function.block(entry).expect("entry").parameters[..1]);
    let slot = loan(
        &append(
            function,
            entry,
            Operation::SharedFieldLoan {
                base: env,
                field: 0,
            },
            vec![view_loan(reference)],
            &at,
        )
        .1,
    );
    let captured = loan(
        &append(
            function,
            entry,
            Operation::SharedReferenceFollow { source: slot },
            vec![view_loan(types.int)],
            &at,
        )
        .1,
    );
    let result = append(
        function,
        entry,
        Operation::Read {
            source: PlaceAccess::Loan(captured),
        },
        vec![EntityType::Value(types.int)],
        &at,
    )
    .1;
    function
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: values(&result),
            },
            at.clone(),
        )
        .expect("return");

    let main = module
        .add_function("main", vec![types.int], at.clone())
        .expect("main");
    let function = module.function_mut(main).expect("main");
    let entry = function
        .add_block(vec![EntityType::Value(types.int); 2], at.clone())
        .expect("entry");
    let inputs = values(&function.block(entry).expect("entry").parameters);
    let mut form = |source| {
        let place = root(function, entry, source, types.int, &at);
        let capture = borrow(function, entry, place, types.int, &at);
        let (_, built) = append(
            function,
            entry,
            Operation::ClosureConstruct {
                closure,
                thunk,
                captures: vec![ClosureCaptureOperand::Shared(capture)],
            },
            vec![EntityType::Value(closure)],
            &at,
        );
        (values(&built)[0], capture)
    };
    let (original, _) = form(inputs[0]);
    let (replacement, capture) = form(inputs[1]);
    let separate = matches!(route, Route::SeparateSlot).then(|| form(inputs[0]));
    let place = root(function, entry, original, closure, &at);
    append(
        function,
        entry,
        Operation::Mutate {
            place,
            value: replacement,
        },
        vec![],
        &at,
    );
    if let Some((other, _)) = separate {
        let other_place = root(function, entry, original, closure, &at);
        append(
            function,
            entry,
            Operation::Mutate {
                place: other_place,
                value: other,
            },
            vec![],
            &at,
        );
    }
    let source = borrow(function, entry, place, closure, &at);
    let view = loan(
        &append(
            function,
            entry,
            Operation::CallableViewOf { source },
            vec![view_loan(types.view)],
            &at,
        )
        .1,
    );
    let (block, original, source, view, capture) = if matches!(route, Route::Edge) {
        let target = function
            .add_block(
                vec![
                    EntityType::Value(closure),
                    view_loan(closure),
                    view_loan(types.view),
                    view_loan(types.int),
                ],
                at.clone(),
            )
            .expect("target");
        function
            .set_terminator(
                entry,
                TerminatorKind::Branch(Edge {
                    target,
                    arguments: vec![
                        EntityId::Value(original),
                        EntityId::Loan(source),
                        EntityId::Loan(view),
                        EntityId::Loan(capture),
                    ],
                }),
                at.clone(),
            )
            .expect("branch");
        let params = &function.block(target).expect("target").parameters;
        (
            target,
            values(&params[..1])[0],
            loan(&params[1..2]),
            loan(&params[2..3]),
            loan(&params[3..]),
        )
    } else {
        (entry, original, source, view, capture)
    };
    let reborrowed = matches!(route, Route::Reborrow).then(|| {
        loan(
            &append(
                function,
                block,
                Operation::SharedReborrow { source: view },
                vec![view_loan(types.view)],
                &at,
            )
            .1,
        )
    });
    let outer = wrapper.map(|(closure, thunk)| {
        values(
            &append(
                function,
                block,
                Operation::ClosureConstruct {
                    closure,
                    thunk,
                    captures: vec![ClosureCaptureOperand::Shared(view)],
                },
                vec![EntityType::Value(closure)],
                &at,
            )
            .1,
        )[0]
    });
    if end_early {
        append(
            function,
            block,
            Operation::BorrowEnd { loan: capture },
            vec![],
            &at,
        );
    }
    let argument = EntityId::Value(inputs[0]);
    let (call, result) = if let Some(callee) = borrow_helper {
        let argument_place = root(function, block, inputs[0], types.int, &at);
        let other = borrow(function, block, argument_place, types.int, &at);
        let (call, borrowed) = append(
            function,
            block,
            Operation::BorrowCall {
                callee,
                arguments: vec![EntityId::Loan(view), EntityId::Loan(other), argument],
                source: other,
            },
            vec![view_loan(types.int)],
            &at,
        );
        let borrowed = loan(&borrowed);
        let result = append(
            function,
            block,
            Operation::Read {
                source: PlaceAccess::Loan(borrowed),
            },
            vec![EntityType::Value(types.int)],
            &at,
        )
        .1;
        for loan in [borrowed, other] {
            append(function, block, Operation::BorrowEnd { loan }, vec![], &at);
        }
        (call, result)
    } else {
        let operation = if let Some(callable) = outer {
            Operation::CallableInvoke {
                callable,
                arguments: vec![argument],
            }
        } else if matches!(route, Route::Helper) {
            Operation::DirectCall {
                callee: helper,
                receiver: None,
                arguments: vec![EntityId::Loan(view), argument],
            }
        } else {
            Operation::CallableInvokeView {
                view: reborrowed.unwrap_or(view),
                arguments: vec![argument],
            }
        };
        append(
            function,
            block,
            operation,
            vec![EntityType::Value(types.int)],
            &at,
        )
    };
    if let Some(owner) = outer {
        append(function, block, Operation::Drop { owner }, vec![], &at);
    } else {
        for loan in reborrowed.into_iter().chain([view]) {
            append(function, block, Operation::BorrowEnd { loan }, vec![], &at);
        }
    }
    append(
        function,
        block,
        Operation::BorrowEnd { loan: source },
        vec![],
        &at,
    );
    if let Some((_, loan)) = separate {
        append(function, block, Operation::BorrowEnd { loan }, vec![], &at);
    }
    append(
        function,
        block,
        Operation::Drop { owner: original },
        vec![],
        &at,
    );
    if !end_early {
        append(
            function,
            block,
            Operation::BorrowEnd { loan: capture },
            vec![],
            &at,
        );
    }
    function
        .set_terminator(
            block,
            TerminatorKind::Return {
                values: values(&result),
            },
            at,
        )
        .expect("return");
    (program, capture, call)
}

#[test]
fn callable_view_reads_current_capture_after_raw_slot_write() {
    for route in [
        Route::Local,
        Route::Reborrow,
        Route::Helper,
        Route::BorrowHelper,
        Route::Captured,
        Route::SeparateSlot,
        Route::Edge,
    ] {
        let (valid, _, _) = program(route, false);
        verify_program(&valid).unwrap_or_else(|errors| panic!("{route:?}: {errors:?}"));
        let (invalid, capture, call) = program(route, true);
        let errors =
            verify_program(&invalid).expect_err("calling through the view reads the ended capture");
        assert!(
            errors.errors.iter().any(|error| {
                error.kind == VerifyErrorKind::LoanInactive { loan: capture }
                    && error.location == VerifyLocation::Instruction(call)
            }),
            "{route:?}: {errors:?}"
        );
    }
}

#[test]
fn following_an_arbitrary_view_reference_does_not_prove_its_capture_contents() {
    let at = origin();
    let (mut program, types) = module_with_types();
    let module = &mut program.modules[0];
    let reference = module
        .add_shared_reference_type(types.view)
        .expect("view reference");
    let id = module
        .add_function("opaque", vec![types.int], at.clone())
        .expect("opaque");
    let function = module.function_mut(id).expect("opaque");
    let entry = function
        .add_block(
            vec![view_loan(reference), EntityType::Value(types.int)],
            at.clone(),
        )
        .expect("entry");
    let parameters = &function.block(entry).expect("entry").parameters;
    let (source, argument) = (loan(&parameters[..1]), parameters[1]);
    let view = loan(
        &append(
            function,
            entry,
            Operation::SharedReferenceFollow { source },
            vec![view_loan(types.view)],
            &at,
        )
        .1,
    );
    let (invoke, result) = append(
        function,
        entry,
        Operation::CallableInvokeView {
            view,
            arguments: vec![argument],
        },
        vec![EntityType::Value(types.int)],
        &at,
    );
    function
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: values(&result),
            },
            at,
        )
        .expect("return");
    let errors =
        verify_program(&program).expect_err("a raw reference does not prove callable contents");
    assert!(
        errors.errors.iter().any(|error| {
            error.kind
                == VerifyErrorKind::OperationContract {
                    reason: "callable view requires proved current capture contents",
                }
                && error.location == VerifyLocation::Instruction(invoke)
        }),
        "{errors:?}"
    );
}

#[test]
fn owned_reference_capture_cannot_certify_a_thunk_view_projection() {
    let at = origin();
    let (mut program, types) = module_with_types();
    let module = &mut program.modules[0];
    let (shared, thunk) = captured_view_thunk(module, &types, &at);
    let (environment, _) = module.concrete_closure(shared).expect("layout");
    let reference = module
        .add_shared_reference_type(types.view)
        .expect("reference");
    let owned = module
        .add_concrete_closure_type(
            "OwnedReference",
            vec![types.int],
            vec![types.int],
            environment,
            vec![ClosureCaptureType {
                mode: ClosureCaptureMode::Owned,
                ty: reference,
            }],
        )
        .expect("owned reference layout");
    let id = module
        .add_function("construct", vec![], at.clone())
        .expect("construct");
    let function = module.function_mut(id).expect("construct");
    let entry = function
        .add_block(vec![EntityType::Value(reference)], at.clone())
        .expect("entry");
    let operand = values(&function.block(entry).expect("entry").parameters)[0];
    let closure = values(
        &append(
            function,
            entry,
            Operation::ClosureConstruct {
                closure: owned,
                thunk,
                captures: vec![ClosureCaptureOperand::Owned(operand)],
            },
            vec![EntityType::Value(owned)],
            &at,
        )
        .1,
    )[0];
    append(
        function,
        entry,
        Operation::Drop { owner: closure },
        vec![],
        &at,
    );
    function
        .set_terminator(entry, TerminatorKind::Return { values: vec![] }, at.clone())
        .expect("return");
    assert_unproved_thunk(&program, thunk);
}

#[test]
fn direct_call_cannot_inherit_a_closure_thunks_environment_proof() {
    let at = origin();
    let (mut program, _, _) = program(Route::Captured, false);
    let module = &mut program.modules[0];
    let thunk = module
        .functions
        .iter()
        .find(|function| function.name == "invoke_captured_view")
        .expect("thunk");
    let id = thunk.id;
    let entry = thunk.blocks[0]
        .parameters
        .iter()
        .map(|entity| thunk.entity(*entity).expect("parameter").ty)
        .collect();
    let returns = thunk.return_types.clone();
    let caller = module
        .add_function("alternate_entry", returns.clone(), at.clone())
        .expect("alternate");
    let function = module.function_mut(caller).expect("alternate");
    let block = function.add_block(entry, at.clone()).expect("entry");
    let arguments = function.block(block).expect("entry").parameters.clone();
    let result = append(
        function,
        block,
        Operation::DirectCall {
            callee: id,
            receiver: None,
            arguments,
        },
        returns.into_iter().map(EntityType::Value).collect(),
        &at,
    )
    .1;
    function
        .set_terminator(
            block,
            TerminatorKind::Return {
                values: values(&result),
            },
            at,
        )
        .expect("return");
    assert_unproved_thunk(&program, id);
}

fn assert_unproved_thunk(program: &Program, thunk: FunctionId) {
    let function = program.modules[0].function(thunk).expect("thunk");
    let call = function
        .instructions
        .iter()
        .find(|instruction| matches!(instruction.operation, Operation::CallableInvokeView { .. }))
        .expect("invoke");
    let errors =
        verify_program(program).expect_err("the thunk's environment provenance is unproved");
    assert!(
        errors.errors.iter().any(|error| {
            error.kind
                == VerifyErrorKind::OperationContract {
                    reason: "callable view requires proved current capture contents",
                }
                && error.location == VerifyLocation::Instruction(call.id)
        }),
        "{errors:?}"
    );
}
