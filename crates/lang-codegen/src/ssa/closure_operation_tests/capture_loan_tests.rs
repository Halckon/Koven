use super::*;

#[derive(Clone, Copy)]
enum CaptureLoan {
    Independent,
    Reuse,
    Child,
}

#[derive(Clone, Copy)]
enum CaptureFlow {
    SameBlock,
    Forward,
    Swap { rebind_loans: bool },
}

fn verify_capture_cleanup(
    second_capture: CaptureLoan,
    parent_first: bool,
    flow: CaptureFlow,
) -> (Result<(), crate::ssa::verify::VerifyErrors>, ValueId) {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("capture-loan-owner");
    let module = program.module_mut(module_id).expect("module");
    let resource = module.intern_type(SsaTypeKind::ZeroSized {
        name: "Resource".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let reference = module
        .add_shared_reference_type(resource)
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
                ty: resource,
            }],
        )
        .expect("closure");
    let (thunk, thunk_entry, _) = add_thunk(module, "thunk", environment, &[], vec![], &origin);
    module
        .function_mut(thunk)
        .expect("thunk")
        .set_terminator(
            thunk_entry,
            TerminatorKind::Return { values: vec![] },
            origin.clone(),
        )
        .expect("return");

    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let (main, entry, parameters) =
        add_function(module, "main", &[resource, boolean], vec![], &origin);
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
    let begin = |function: &mut Function| {
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
        loan
    };
    let first_loan = begin(function);
    let first = append_values(
        function,
        entry,
        Operation::ClosureConstruct {
            closure,
            thunk,
            captures: vec![ClosureCaptureOperand::Shared(first_loan)],
        },
        &[closure],
        &origin,
    )[0];
    let second_loan = match second_capture {
        CaptureLoan::Independent => begin(function),
        CaptureLoan::Reuse => first_loan,
        CaptureLoan::Child => {
            let EntityId::Loan(loan) = function
                .append_instruction(
                    entry,
                    Operation::SharedReborrow { source: first_loan },
                    vec![EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: resource,
                    }],
                    origin.clone(),
                )
                .expect("reborrow")
                .1[0]
            else {
                panic!("reborrow result");
            };
            loan
        }
    };
    let second = append_values(
        function,
        entry,
        Operation::ClosureConstruct {
            closure,
            thunk,
            captures: vec![ClosureCaptureOperand::Shared(second_loan)],
        },
        &[closure],
        &origin,
    )[0];
    let (entry, owner, first, second) = if matches!(flow, CaptureFlow::SameBlock) {
        (entry, owner, first, second)
    } else {
        let rebind = !matches!(
            flow,
            CaptureFlow::Swap {
                rebind_loans: false
            }
        );
        let mut types = vec![
            EntityType::Value(resource),
            EntityType::Value(closure),
            EntityType::Value(closure),
        ];
        if rebind {
            types.extend(
                [EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: resource,
                }; 2],
            );
        }
        let target = function
            .add_block(types, origin.clone())
            .expect("continuation");
        let edge = |first, second, first_loan, second_loan| {
            let mut arguments = vec![
                EntityId::Value(owner),
                EntityId::Value(first),
                EntityId::Value(second),
            ];
            if rebind {
                arguments.extend([EntityId::Loan(first_loan), EntityId::Loan(second_loan)]);
            }
            Edge { target, arguments }
        };
        let same_order = edge(first, second, first_loan, second_loan);
        let terminator = if matches!(flow, CaptureFlow::Swap { .. }) {
            TerminatorKind::Conditional {
                condition: parameters[1],
                when_true: same_order,
                when_false: edge(second, first, second_loan, first_loan),
            }
        } else {
            TerminatorKind::Branch(same_order)
        };
        function
            .set_terminator(entry, terminator, origin.clone())
            .expect("branch");
        let parameters = &function.block(target).expect("continuation").parameters;
        (
            target,
            value(parameters[0]),
            value(parameters[1]),
            value(parameters[2]),
        )
    };
    let drop = |function: &mut Function, owner| {
        append_values(function, entry, Operation::Drop { owner }, &[], &origin);
    };
    if parent_first {
        drop(function, first);
    }
    let loses_second_loan = matches!(second_capture, CaptureLoan::Reuse)
        || matches!(
            flow,
            CaptureFlow::Swap {
                rebind_loans: false
            }
        );
    if loses_second_loan {
        // Without a unique owner, the first drop also releases the second environment's loan.
        drop(function, owner);
    }
    append_values(
        function,
        entry,
        Operation::CallableInvoke {
            callable: second,
            arguments: vec![],
        },
        &[],
        &origin,
    );
    drop(function, second);
    if !parent_first {
        drop(function, first);
    }
    if !loses_second_loan {
        drop(function, owner);
    }
    function
        .set_terminator(
            entry,
            TerminatorKind::Return { values: vec![] },
            origin.clone(),
        )
        .expect("return");

    let holder = if matches!(flow, CaptureFlow::Swap { .. }) {
        second
    } else {
        first
    };
    (verify_program(&program), holder)
}

#[test]
fn two_closure_environments_cannot_take_the_same_capture_loan() {
    verify_capture_cleanup(CaptureLoan::Independent, true, CaptureFlow::SameBlock)
        .0
        .expect("two independent shared loans of one root are legal");
    let (result, holder) = verify_capture_cleanup(CaptureLoan::Reuse, true, CaptureFlow::SameBlock);
    let errors = result.expect_err("same loan cannot be given to two environments");
    assert!(
        errors
            .errors
            .iter()
            .any(|error| { error.kind == VerifyErrorKind::OwnerLoanConflict { value: holder } })
    );
}

#[test]
fn dropping_capture_owner_cannot_end_a_loan_with_a_live_child() {
    for flow in [CaptureFlow::SameBlock, CaptureFlow::Forward] {
        verify_capture_cleanup(CaptureLoan::Child, false, flow)
            .0
            .expect("dropping the child environment first ends its reborrow");
        let errors = verify_capture_cleanup(CaptureLoan::Child, true, flow)
            .0
            .expect_err("the parent environment cannot release the child's loan");
        assert!(
            errors
                .errors
                .iter()
                .any(|error| matches!(error.kind, VerifyErrorKind::LoanDependencyActive { .. }))
        );
    }
}

#[test]
fn swapped_capture_owners_need_distinct_loan_identities_at_the_join() {
    let flow = |rebind_loans| CaptureFlow::Swap { rebind_loans };
    verify_capture_cleanup(CaptureLoan::Independent, true, flow(true))
        .0
        .expect("the loan block parameters retain the corresponding owner");
    let (result, holder) = verify_capture_cleanup(CaptureLoan::Independent, true, flow(false));
    let errors =
        result.expect_err("one closure must not end both loans and leave the other dangling");
    assert!(
        errors
            .errors
            .iter()
            .any(|error| { error.kind == VerifyErrorKind::OwnerLoanConflict { value: holder } })
    );
}

#[derive(Clone, Copy)]
enum OwnedCaptureFlow {
    Local,
    BeforeMove,
    AfterMove { rebind: bool },
    IndependentLoan { swap: bool },
}

fn verify_owned_capture_cleanup(
    flow: OwnedCaptureFlow,
    early_end: bool,
) -> (Result<(), crate::ssa::verify::VerifyErrors>, ValueId) {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("owned-nested-capture");
    let module = program.module_mut(module_id).expect("module");
    let resource = module.intern_type(SsaTypeKind::ZeroSized {
        name: "Resource".to_owned(),
        ownership: Ownership::MoveOnly,
    });
    let reference = module
        .add_shared_reference_type(resource)
        .expect("reference");
    let mut make_closure = |name: &str, mode, ty, field| {
        let environment = module
            .add_aggregate_type(format!("{name}Env"), vec![field])
            .expect("env");
        let closure = module
            .add_concrete_closure_type(
                name.to_owned(),
                vec![],
                vec![],
                environment,
                vec![ClosureCaptureType { mode, ty }],
            )
            .expect("closure");
        let (thunk, entry, _) = add_thunk(module, name, environment, &[], vec![], &origin);
        module
            .function_mut(thunk)
            .expect("thunk")
            .set_terminator(
                entry,
                TerminatorKind::Return { values: vec![] },
                origin.clone(),
            )
            .expect("return");
        (closure, thunk)
    };
    let (inner, inner_thunk) =
        make_closure("Inner", ClosureCaptureMode::Shared, resource, reference);
    let (outer, outer_thunk) = make_closure("Outer", ClosureCaptureMode::Owned, inner, inner);
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let (main, entry, parameters) =
        add_function(module, "main", &[resource, boolean], vec![], &origin);
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
        panic!("place result");
    };
    let loan_type = EntityType::Loan {
        kind: LoanKind::Shared,
        target: resource,
    };
    let begin_loan = |function: &mut Function| {
        function
            .append_instruction(
                entry,
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                },
                vec![loan_type],
                origin.clone(),
            )
            .expect("loan")
            .1[0]
    };
    let loan = begin_loan(function);
    let EntityId::Loan(capture) = loan else {
        panic!("loan result")
    };
    let inner_owner = append_values(
        function,
        entry,
        Operation::ClosureConstruct {
            closure: inner,
            thunk: inner_thunk,
            captures: vec![ClosureCaptureOperand::Shared(capture)],
        },
        &[inner],
        &origin,
    )[0];
    let wrap = |function: &mut Function, block, inner_owner| {
        append_values(
            function,
            block,
            Operation::ClosureConstruct {
                closure: outer,
                thunk: outer_thunk,
                captures: vec![ClosureCaptureOperand::Owned(inner_owner)],
            },
            &[outer],
            &origin,
        )[0]
    };
    let capture_after_edge = matches!(flow, OwnedCaptureFlow::BeforeMove);
    let carried = if capture_after_edge {
        inner_owner
    } else {
        wrap(function, entry, inner_owner)
    };
    let (block, owner, carried, loan, independent) = if matches!(flow, OwnedCaptureFlow::Local) {
        (entry, owner, carried, loan, None)
    } else {
        let carried_type = if capture_after_edge { inner } else { outer };
        let mut types = vec![EntityType::Value(resource), EntityType::Value(carried_type)];
        let mut arguments = vec![EntityId::Value(owner), EntityId::Value(carried)];
        if !matches!(flow, OwnedCaptureFlow::AfterMove { rebind: false }) {
            types.push(loan_type);
            arguments.push(loan);
        }
        if matches!(flow, OwnedCaptureFlow::IndependentLoan { .. }) {
            types.push(loan_type);
            arguments.push(begin_loan(function));
        }
        let target = function
            .add_block(types, origin.clone())
            .expect("continuation");
        let edge = Edge { target, arguments };
        let terminator = if let OwnedCaptureFlow::IndependentLoan { swap } = flow {
            let mut alternate = edge.clone();
            if swap {
                alternate.arguments.swap(2, 3);
            }
            TerminatorKind::Conditional {
                condition: parameters[1],
                when_true: edge,
                when_false: alternate,
            }
        } else {
            TerminatorKind::Branch(edge)
        };
        function
            .set_terminator(entry, terminator, origin.clone())
            .expect("branch");
        let params = &function.block(target).expect("continuation").parameters;
        (
            target,
            value(params[0]),
            value(params[1]),
            params.get(2).copied().unwrap_or(loan),
            params.get(3).copied(),
        )
    };
    let outer_owner = if capture_after_edge {
        wrap(function, block, carried)
    } else {
        carried
    };
    let EntityId::Loan(loan) = loan else {
        panic!("loan parameter")
    };
    if early_end {
        append_values(function, block, Operation::BorrowEnd { loan }, &[], &origin);
        append_values(function, block, Operation::Drop { owner }, &[], &origin);
    }
    append_values(
        function,
        block,
        Operation::CallableInvoke {
            callable: outer_owner,
            arguments: vec![],
        },
        &[],
        &origin,
    );
    append_values(
        function,
        block,
        Operation::Drop { owner: outer_owner },
        &[],
        &origin,
    );
    if matches!(flow, OwnedCaptureFlow::IndependentLoan { swap: false }) {
        let Some(EntityId::Loan(independent)) = independent else {
            panic!("independent loan")
        };
        append_values(
            function,
            block,
            Operation::BorrowEnd { loan: independent },
            &[],
            &origin,
        );
    }
    if !early_end {
        append_values(function, block, Operation::Drop { owner }, &[], &origin);
    }
    function
        .set_terminator(block, TerminatorKind::Return { values: vec![] }, origin)
        .expect("return");
    (verify_program(&program), outer_owner)
}

#[test]
fn dropping_an_owned_wrapper_ends_its_inner_capture_loan() {
    for flow in [
        OwnedCaptureFlow::Local,
        OwnedCaptureFlow::BeforeMove,
        OwnedCaptureFlow::AfterMove { rebind: true },
        OwnedCaptureFlow::AfterMove { rebind: false },
    ] {
        verify_owned_capture_cleanup(flow, false)
            .0
            .expect("the outer drop ends the nested capture before the root drop");
    }
}

#[test]
fn an_owned_wrapper_prevents_an_explicit_end_of_its_inner_capture_loan() {
    for flow in [
        OwnedCaptureFlow::Local,
        OwnedCaptureFlow::BeforeMove,
        OwnedCaptureFlow::AfterMove { rebind: true },
    ] {
        let (result, holder) = verify_owned_capture_cleanup(flow, true);
        let errors = result.expect_err("ending the loan would leave a live nested capture");
        assert!(errors.errors.iter().any(|error| {
            error.kind == VerifyErrorKind::OwnerLoanConflict { value: holder }
                && matches!(
                    error.location,
                    crate::ssa::verify::VerifyLocation::Instruction(_)
                )
        }));
    }
}

#[test]
fn owned_wrapper_cannot_end_an_independent_loan_at_a_join() {
    verify_owned_capture_cleanup(OwnedCaptureFlow::IndependentLoan { swap: false }, false)
        .0
        .expect("consistent loan parameters preserve the independent explicit end");
    let errors =
        verify_owned_capture_cleanup(OwnedCaptureFlow::IndependentLoan { swap: true }, false)
            .0
            .expect_err("a join must not make the wrapper also own the independent loan");
    assert!(errors.errors.iter().any(|error| {
        matches!(error.kind, VerifyErrorKind::OwnerLoanConflict { .. })
            && matches!(
                error.location,
                crate::ssa::verify::VerifyLocation::Edge { .. }
            )
    }));
}

#[test]
fn loan_mutation_waits_for_the_shared_reborrow_to_end() {
    for reborrow_live in [false, true] {
        let origin = origin();
        let mut program = Program::default();
        let module_id = program.add_module("exclusive-write");
        let module = program.module_mut(module_id).expect("module");
        let integer = module.intern_type(SsaTypeKind::Integer {
            bits: 64,
            signed: true,
        });
        let (main, entry, parameters) = add_function(module, "main", &[integer], vec![], &origin);
        let function = module.function_mut(main).expect("main");
        let value = parameters[0];
        let EntityId::Place(place) = function
            .append_instruction(
                entry,
                Operation::RootPlace { owner: value },
                vec![EntityType::Place(integer)],
                origin.clone(),
            )
            .expect("place")
            .1[0]
        else {
            panic!("place")
        };
        let EntityId::Loan(parent) = function
            .append_instruction(
                entry,
                Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Exclusive,
                },
                vec![EntityType::Loan {
                    kind: LoanKind::Exclusive,
                    target: integer,
                }],
                origin.clone(),
            )
            .expect("parent")
            .1[0]
        else {
            panic!("parent")
        };
        let EntityId::Loan(child) = function
            .append_instruction(
                entry,
                Operation::SharedReborrow { source: parent },
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: integer,
                }],
                origin.clone(),
            )
            .expect("child")
            .1[0]
        else {
            panic!("child")
        };

        let mutate = Operation::LoanMutate {
            loan: parent,
            value,
        };
        let end_child = Operation::BorrowEnd { loan: child };
        let operations = if reborrow_live {
            [mutate, end_child]
        } else {
            [end_child, mutate]
        };
        for operation in operations
            .into_iter()
            .chain([Operation::BorrowEnd { loan: parent }])
        {
            function
                .append_instruction(entry, operation, vec![], origin.clone())
                .expect("operation");
        }
        function
            .set_terminator(entry, TerminatorKind::Return { values: vec![] }, origin)
            .expect("return");

        if reborrow_live {
            assert!(has_error(&program, |kind| {
                *kind
                    == VerifyErrorKind::LoanDependencyActive {
                        parent,
                        dependent: child,
                    }
            }));
        } else {
            verify_program(&program).expect("ending the reborrow restores write permission");
        }
    }
}
