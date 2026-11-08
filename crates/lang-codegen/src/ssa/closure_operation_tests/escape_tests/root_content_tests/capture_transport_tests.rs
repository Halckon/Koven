use super::*;

#[derive(Clone, Copy, Debug)]
enum Transport {
    Take,
    SeparatePlaceWrite,
    MutatedTake,
    EndedStored,
    ReusedStored,
    MergedWrite,
}

fn borrow(
    function: &mut Function,
    block: BlockId,
    place: PlaceId,
    ty: SsaTypeId,
    at: &Origin,
) -> LoanId {
    let EntityId::Loan(loan) = function
        .append_instruction(
            block,
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: ty,
            }],
            at.clone(),
        )
        .expect("capture loan")
        .1[0]
    else {
        panic!("loan result");
    };
    loan
}

fn ambiguous_write(
    module: &mut Module,
    closure: SsaTypeId,
    thunk: FunctionId,
    source: SsaTypeId,
    at: &Origin,
) {
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let id = module
        .add_function("join", vec![], at.clone())
        .expect("join");
    let function = module.function_mut(id).expect("join");
    let shared = EntityType::Loan {
        kind: LoanKind::Shared,
        target: source,
    };
    let entry = function
        .add_block(vec![EntityType::Value(boolean), shared, shared], at.clone())
        .expect("entry");
    let parameters = &function.block(entry).expect("entry").parameters;
    let condition = value(parameters[0]);
    let [EntityId::Loan(first), EntityId::Loan(second)] = parameters[1..] else {
        panic!("entry loans");
    };
    let arms = [false, true].map(|_| function.add_block(vec![], at.clone()).expect("arm"));
    let join = function
        .add_block(vec![EntityType::Value(closure); 2], at.clone())
        .expect("join");
    function
        .set_terminator(
            entry,
            TerminatorKind::Conditional {
                condition,
                when_true: Edge {
                    target: arms[0],
                    arguments: vec![],
                },
                when_false: Edge {
                    target: arms[1],
                    arguments: vec![],
                },
            },
            at.clone(),
        )
        .expect("choose");
    for (arm, order) in arms.into_iter().zip([[first, second], [second, first]]) {
        // Construct distinct physical owners on each predecessor. Their capture
        // sets merge to the same union, while the actual loans are opposite.
        let arguments = order
            .into_iter()
            .map(|loan| {
                EntityId::Value(
                    append_values(
                        function,
                        arm,
                        Operation::ClosureConstruct {
                            closure,
                            thunk,
                            captures: vec![ClosureCaptureOperand::Shared(loan)],
                        },
                        &[closure],
                        at,
                    )[0],
                )
            })
            .collect();
        function
            .set_terminator(
                arm,
                TerminatorKind::Branch(Edge {
                    target: join,
                    arguments,
                }),
                at.clone(),
            )
            .expect("pass closures");
    }
    let parameters = &function.block(join).expect("join").parameters;
    let (owner, written) = (value(parameters[0]), value(parameters[1]));
    let place = root_place(function, join, owner, closure, at);
    append_values(
        function,
        join,
        Operation::Mutate {
            place,
            value: written,
        },
        &[],
        at,
    );
    let result = append_values(
        function,
        join,
        Operation::RootPlaceTake { owner, place },
        &[closure],
        at,
    )[0];
    append_values(function, join, Operation::Drop { owner: result }, &[], at);
    function
        .set_terminator(join, TerminatorKind::Return { values: vec![] }, at.clone())
        .expect("return");
}

fn verify_transport(
    transport: Transport,
    early_end: bool,
) -> (Program, Option<(ValueId, VerifyLocation)>) {
    let at = origin();
    let mut program = Program::default();
    let module_id = program.add_module("capture-root-transport");
    let module = program.module_mut(module_id).expect("module");
    let (source_ty, env, closure) = shared_closure_type(module);
    let (thunk, thunk_entry, _) = add_thunk(module, "thunk", env, &[], vec![], &at);
    module
        .function_mut(thunk)
        .expect("thunk")
        .set_terminator(
            thunk_entry,
            TerminatorKind::Return { values: vec![] },
            at.clone(),
        )
        .expect("return");
    if matches!(transport, Transport::MergedWrite) {
        ambiguous_write(module, closure, thunk, source_ty, &at);
        return (program, None);
    }
    let (main, block, sources) = add_function(module, "main", &[source_ty, source_ty], vec![], &at);
    let function = module.function_mut(main).expect("main");
    let mut capture = |source| {
        let place = root_place(function, block, source, source_ty, &at);
        let loan = borrow(function, block, place, source_ty, &at);
        let owner = append_values(
            function,
            block,
            Operation::ClosureConstruct {
                closure,
                thunk,
                captures: vec![ClosureCaptureOperand::Shared(loan)],
            },
            &[closure],
            &at,
        )[0];
        (owner, loan)
    };
    let (left, left_capture) = capture(sources[0]);
    let (right, right_capture) = capture(sources[1]);
    let left_place = root_place(function, block, left, closure, &at);
    if matches!(
        transport,
        Transport::MutatedTake
            | Transport::SeparatePlaceWrite
            | Transport::EndedStored
            | Transport::ReusedStored
    ) {
        let destination = if !matches!(transport, Transport::SeparatePlaceWrite) {
            left_place
        } else {
            root_place(function, block, left, closure, &at)
        };
        append_values(
            function,
            block,
            Operation::Mutate {
                place: destination,
                value: right,
            },
            &[],
            &at,
        );
    }
    if matches!(transport, Transport::EndedStored) {
        append_values(
            function,
            block,
            Operation::BorrowEnd {
                loan: right_capture,
            },
            &[],
            &at,
        );
    }
    let other_holder = matches!(transport, Transport::ReusedStored).then(|| {
        append_values(
            function,
            block,
            Operation::ClosureConstruct {
                closure,
                thunk,
                captures: vec![ClosureCaptureOperand::Shared(right_capture)],
            },
            &[closure],
            &at,
        )[0]
    });
    let result = append_values(
        function,
        block,
        Operation::RootPlaceTake {
            owner: left,
            place: left_place,
        },
        &[closure],
        &at,
    )[0];
    let owners = match transport {
        Transport::Take => vec![result, right],
        Transport::SeparatePlaceWrite => {
            append_values(
                function,
                block,
                Operation::BorrowEnd {
                    loan: right_capture,
                },
                &[],
                &at,
            );
            vec![result]
        }
        Transport::MergedWrite => unreachable!("handled before creating main"),
        Transport::MutatedTake | Transport::EndedStored | Transport::ReusedStored => {
            append_values(
                function,
                block,
                Operation::BorrowEnd { loan: left_capture },
                &[],
                &at,
            );
            vec![result]
        }
    };
    let expected = if early_end {
        let loan = if matches!(transport, Transport::MutatedTake) {
            right_capture
        } else {
            left_capture
        };
        let instruction = function
            .append_instruction(block, Operation::BorrowEnd { loan }, vec![], at.clone())
            .expect("premature end")
            .0;
        Some((result, VerifyLocation::Instruction(instruction)))
    } else if matches!(transport, Transport::EndedStored | Transport::ReusedStored) {
        let take = function
            .block(block)
            .expect("block")
            .instructions
            .iter()
            .copied()
            .find(|id| {
                matches!(
                    function.instruction(*id).expect("instruction").operation,
                    Operation::RootPlaceTake { .. }
                )
            })
            .expect("take");
        Some((
            other_holder.unwrap_or(result),
            VerifyLocation::Instruction(take),
        ))
    } else {
        None
    };
    if let Some(owner) = other_holder {
        append_values(function, block, Operation::Consume { owner }, &[], &at);
    }
    for owner in owners {
        append_values(
            function,
            block,
            Operation::CallableInvoke {
                callable: owner,
                arguments: vec![],
            },
            &[],
            &at,
        );
        append_values(function, block, Operation::Drop { owner }, &[], &at);
    }
    function
        .set_terminator(block, TerminatorKind::Return { values: vec![] }, at)
        .expect("return");
    (program, expected)
}

#[test]
fn root_capture_transport_drops_the_loan_at_its_new_owner() {
    for transport in [
        Transport::Take,
        Transport::SeparatePlaceWrite,
        Transport::MutatedTake,
    ] {
        verify_program(&verify_transport(transport, false).0)
            .unwrap_or_else(|errors| panic!("{transport:?}: {errors:?}"));
    }
}

#[test]
fn root_capture_transport_rejects_early_end_after_take() {
    for transport in [Transport::Take, Transport::MutatedTake] {
        let (program, expected) = verify_transport(transport, true);
        let (value, location) = expected.expect("expected loan owner and location");
        let errors = verify_program(&program).expect_err("result still owns the loan");
        assert!(
            errors.errors.iter().any(|error| {
                error.kind == VerifyErrorKind::OwnerLoanConflict { value }
                    && error.location == location
            }),
            "{transport:?}: {errors:?}"
        );
    }
}

#[test]
fn root_capture_transport_rejects_taking_a_slot_with_an_ended_capture() {
    let (program, expected) = verify_transport(Transport::EndedStored, false);
    let (_, location) = expected.expect("take location");
    let errors = verify_program(&program).expect_err("take cannot revive a capture loan");
    assert!(
        errors.errors.iter().any(|error| {
            matches!(error.kind, VerifyErrorKind::LoanInactive { .. }) && error.location == location
        }),
        "{errors:?}"
    );
}

#[test]
fn root_capture_transport_rejects_a_stored_capture_claimed_by_another_owner() {
    let (program, expected) = verify_transport(Transport::ReusedStored, false);
    let (value, location) = expected.expect("other holder and take location");
    let errors = verify_program(&program).expect_err("take cannot duplicate a live capture holder");
    assert!(
        errors.errors.iter().any(|error| {
            error.kind == VerifyErrorKind::OwnerLoanConflict { value } && error.location == location
        }),
        "{errors:?}"
    );
}

#[test]
fn root_capture_transport_rejects_ambiguous_capture_ownership_after_write() {
    let (program, _) = verify_transport(Transport::MergedWrite, false);
    let errors = verify_program(&program)
        .expect_err("overlapping join loans cannot identify the slot contents");
    assert!(
        errors.errors.iter().any(|error| matches!(
            error.kind,
            VerifyErrorKind::OperationContract {
                reason: "root take has ambiguous capture loans after a raw place write"
            }
        )),
        "{errors:?}"
    );
}
