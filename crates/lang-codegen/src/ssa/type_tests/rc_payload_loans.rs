//! Shared Rc handle-slot loans retain their parent across CFG edges.
use super::shared_owner_operation_tests::{append, origin, value};
use crate::ssa::{
    model::{
        Edge, EntityId, EntityType, LoanKind, Operation, Program, SsaTypeKind, TerminatorKind,
    },
    verify::{VerifyErrorKind, VerifyLocation, verify_program},
};

#[test]
fn shared_payload_loan_requires_an_active_parent_until_child_end() {
    for end_at in [0, 1, 2] {
        let origin = origin();
        let mut program = Program::default();
        let module_id = program.add_module("borrowed-rc");
        let module = program.module_mut(module_id).unwrap();
        let payload = module.intern_type(SsaTypeKind::Integer {
            bits: 64,
            signed: true,
        });
        let owner = module.declare_shared_owner("Rc").unwrap();
        module.define_shared_owner(owner, payload).unwrap();
        let function_id = module
            .add_function("borrowed", vec![], origin.clone())
            .unwrap();
        let function = module.function_mut(function_id).unwrap();
        let entry = function
            .add_block(
                vec![EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: owner,
                }],
                origin.clone(),
            )
            .unwrap();
        let EntityId::Loan(parent) = function.block(entry).unwrap().parameters[0] else {
            panic!("expected shared owner parameter");
        };
        let mut parent_end = None;
        if end_at == 0 {
            parent_end = Some(
                append(
                    function,
                    entry,
                    Operation::BorrowEnd { loan: parent },
                    vec![],
                    &origin,
                )
                .0,
            );
        }
        let (projection, results) = append(
            function,
            entry,
            Operation::SharedPayloadLoan { source: parent },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: payload,
            }],
            &origin,
        );
        let EntityId::Loan(child) = results[0] else {
            panic!("expected payload loan");
        };
        if end_at == 1 {
            parent_end = Some(
                append(
                    function,
                    entry,
                    Operation::BorrowEnd { loan: parent },
                    vec![],
                    &origin,
                )
                .0,
            );
        }
        append(
            function,
            entry,
            Operation::BorrowEnd { loan: child },
            vec![],
            &origin,
        );
        if end_at == 2 {
            append(
                function,
                entry,
                Operation::BorrowEnd { loan: parent },
                vec![],
                &origin,
            );
        }
        function
            .set_terminator(entry, TerminatorKind::Return { values: vec![] }, origin)
            .unwrap();
        if end_at == 2 {
            verify_program(&program).expect("end the payload before its source slot");
        } else {
            let errors = verify_program(&program)
                .expect_err("invalid loan dependency")
                .errors;
            let (instruction, kind) = if end_at == 0 {
                (projection, VerifyErrorKind::LoanInactive { loan: parent })
            } else {
                (
                    parent_end.unwrap(),
                    VerifyErrorKind::LoanDependencyActive {
                        parent,
                        dependent: child,
                    },
                )
            };
            assert!(errors.iter().any(|err| {
                err.location == VerifyLocation::Instruction(instruction) && err.kind == kind
            }));
        }
    }
}

#[test]
fn shared_payload_loan_protects_the_rebound_nullable_proof() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("nullable-rc");
    let module = program.module_mut(module_id).unwrap();
    let payload = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let owner = module.declare_shared_owner("Rc").unwrap();
    module.define_shared_owner(owner, payload).unwrap();
    let nullable = module.add_nullable_handle_type(owner).unwrap();
    let id = module
        .add_function("inspect", vec![], origin.clone())
        .unwrap();
    let function = module.function_mut(id).unwrap();
    let storage = EntityType::Value(nullable);
    let owner_loan = EntityType::Loan {
        kind: LoanKind::Shared,
        target: owner,
    };
    let payload_loan = EntityType::Loan {
        kind: LoanKind::Shared,
        target: payload,
    };
    let entry = function.add_block(vec![storage], origin.clone()).unwrap();
    let null = function.add_block(vec![storage], origin.clone()).unwrap();
    let present = function
        .add_block(vec![storage, owner_loan], origin.clone())
        .unwrap();
    let next = function
        .add_block(vec![storage, owner_loan, payload_loan], origin.clone())
        .unwrap();
    let input = function.block(entry).unwrap().parameters[0];
    let [present_owner, EntityId::Loan(proof)] =
        *function.block(present).unwrap().parameters.as_slice()
    else {
        panic!("non-null successor parameters");
    };
    function
        .set_terminator(
            entry,
            TerminatorKind::NullableBranch {
                owner: value(input),
                when_null: Edge {
                    target: null,
                    arguments: vec![input],
                },
                when_non_null: Edge {
                    target: present,
                    arguments: vec![input],
                },
                view: proof,
            },
            origin.clone(),
        )
        .unwrap();

    let (_, projected) = append(
        function,
        present,
        Operation::SharedPayloadLoan { source: proof },
        vec![payload_loan],
        &origin,
    );
    function
        .set_terminator(
            present,
            TerminatorKind::Branch(Edge {
                target: next,
                arguments: vec![present_owner, EntityId::Loan(proof), projected[0]],
            }),
            origin.clone(),
        )
        .unwrap();

    let [next_owner, EntityId::Loan(parent), EntityId::Loan(child)] =
        *function.block(next).unwrap().parameters.as_slice()
    else {
        panic!("rebound successor parameters");
    };
    append(
        function,
        next,
        Operation::BorrowEnd { loan: child },
        vec![],
        &origin,
    );
    let end = append(
        function,
        next,
        Operation::BorrowEnd { loan: parent },
        vec![],
        &origin,
    )
    .0;
    for (block, owner) in [
        (null, function.block(null).unwrap().parameters[0]),
        (next, next_owner),
    ] {
        append(
            function,
            block,
            Operation::Drop {
                owner: value(owner),
            },
            vec![],
            &origin,
        );
        function
            .set_terminator(
                block,
                TerminatorKind::Return { values: vec![] },
                origin.clone(),
            )
            .unwrap();
    }

    verify_program(&program).expect("end the rebound child before its parent");
    let function = program
        .module_mut(module_id)
        .unwrap()
        .function_mut(id)
        .unwrap();
    function.blocks[next.index()].instructions.swap(0, 1);
    let errors = verify_program(&program)
        .expect_err("rebound proof still holds its child")
        .errors;
    assert!(errors.iter().any(|err| {
        err.location == VerifyLocation::Instruction(end)
            && err.kind
                == VerifyErrorKind::LoanDependencyActive {
                    parent,
                    dependent: child,
                }
    }));
}

#[test]
fn shared_payload_place_rejects_a_borrowed_handle_slot() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("borrowed-rc");
    let module = program.module_mut(module_id).unwrap();
    let payload = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });
    let owner = module.declare_shared_owner("Rc").unwrap();
    module.define_shared_owner(owner, payload).unwrap();
    let id = module
        .add_function("inspect", vec![], origin.clone())
        .unwrap();
    let function = module.function_mut(id).unwrap();
    let entry = function
        .add_block(
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: owner,
            }],
            origin.clone(),
        )
        .unwrap();
    let input = function.block(entry).unwrap().parameters[0];
    let projection = append(
        function,
        entry,
        Operation::SharedPayloadPlace { owner: input },
        vec![EntityType::Place(payload)],
        &origin,
    )
    .0;
    function
        .set_terminator(entry, TerminatorKind::Return { values: vec![] }, origin)
        .unwrap();
    let errors = verify_program(&program)
        .expect_err("use the tracked payload loan operation")
        .errors;
    assert!(errors.iter().any(|err| {
        err.location == VerifyLocation::Instruction(projection)
            && matches!(err.kind, VerifyErrorKind::OperationContract { .. })
    }));
}
