use super::*;
use crate::ssa::{
    model::{ClosureCaptureMode, ClosureCaptureOperand, ClosureCaptureType, InstructionId},
    verify::VerifyLocation,
};

fn nullable_capture(before_take: bool) -> (Program, ValueId, InstructionId) {
    let (mut program, non_null_owner, _) = nullable_operations_program(None);
    let at = origin();
    let module = &mut program.modules[0];
    let inspect = module.functions[0].id;
    let non_null_block = module.functions[0].blocks[2].id;
    let proof = loan(module.functions[0].blocks[2].parameters[1]);
    let inner = module.functions[0].loans[proof.index()].ty.semantic_type();
    let reference = module.add_shared_reference_type(inner).expect("reference");
    let env = module
        .add_aggregate_type("Env", vec![reference])
        .expect("environment");
    let closure = module
        .add_concrete_closure_type(
            "ViewCapture",
            vec![],
            vec![],
            env,
            vec![ClosureCaptureType {
                mode: ClosureCaptureMode::Shared,
                ty: inner,
            }],
        )
        .expect("closure");
    let thunk = module
        .add_function("thunk", vec![], at.clone())
        .expect("thunk");
    let function = module.function_mut(thunk).expect("thunk");
    let entry = function
        .add_block(
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: env,
            }],
            at.clone(),
        )
        .expect("environment parameter");
    function
        .set_terminator(entry, TerminatorKind::Return { values: vec![] }, at.clone())
        .expect("return");

    let function = module.function_mut(inspect).expect("inspect");
    let block = &mut function.blocks[non_null_block.index()];
    let terminator = block.terminator.take().expect("return");
    let old = std::mem::take(&mut block.instructions);
    let take = old[0];
    assert!(matches!(
        function.instruction(take).expect("take").operation,
        Operation::NullableTake { owner, .. } if owner == non_null_owner
    ));
    let capture = if before_take {
        loan(
            function
                .append_instruction(
                    non_null_block,
                    Operation::SharedReborrow { source: proof },
                    vec![EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: inner,
                    }],
                    at.clone(),
                )
                .expect("reborrow")
                .1[0],
        )
    } else {
        proof
    };
    let holder = append_value(
        function,
        non_null_block,
        Operation::ClosureConstruct {
            closure,
            thunk,
            captures: vec![ClosureCaptureOperand::Shared(capture)],
        },
        closure,
        &at,
    );
    if !before_take {
        function.blocks[non_null_block.index()]
            .instructions
            .extend(&old);
    }
    for operation in [
        Operation::CallableInvoke {
            callable: holder,
            arguments: vec![],
        },
        Operation::Drop { owner: holder },
    ] {
        function
            .append_instruction(non_null_block, operation, vec![], at.clone())
            .expect("closure use");
    }
    if before_take {
        function.blocks[non_null_block.index()]
            .instructions
            .extend(old);
    }
    function.blocks[non_null_block.index()].terminator = Some(terminator);
    (program, holder, take)
}

#[test]
fn nullable_take_respects_capture_loan_holders() {
    let (valid, _, _) = nullable_capture(true);
    verify_program(&valid)
        .expect("dropping a reborrowed capture leaves the original proof for take");
    let (invalid, value, take) = nullable_capture(false);
    let errors =
        verify_program(&invalid).expect_err("take cannot consume an active environment's proof");
    assert!(
        errors.errors.iter().any(|error| {
            error.kind == VerifyErrorKind::OwnerLoanConflict { value }
                && error.location == VerifyLocation::Instruction(take)
        }),
        "{errors:?}"
    );
}

fn copyable_proof(end_child_first: bool) -> (Program, LoanId, LoanId) {
    let at = origin();
    let mut program = Program::default();
    let module_id = program.add_module("copyable-nullable-proof");
    let module = program.module_mut(module_id).expect("module");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let nullable = module
        .add_inline_nullable_type(integer)
        .expect("inline nullable");
    let function_id = module
        .add_function("inspect", vec![], at.clone())
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let entry = function
        .add_block(vec![EntityType::Value(nullable)], at.clone())
        .expect("entry");
    let none = function.add_block(vec![], at.clone()).expect("null");
    let proof_type = EntityType::Loan {
        kind: LoanKind::Shared,
        target: integer,
    };
    let some = function
        .add_block(vec![EntityType::Value(nullable), proof_type], at.clone())
        .expect("some");
    let owner = value(function.block(entry).expect("entry").parameters[0]);
    let parameters = &function.block(some).expect("some").parameters;
    let proof = loan(parameters[1]);
    let taken = value(parameters[0]);
    function
        .set_terminator(
            entry,
            TerminatorKind::NullableBranch {
                owner,
                when_null: Edge {
                    target: none,
                    arguments: vec![],
                },
                when_non_null: Edge {
                    target: some,
                    arguments: vec![EntityId::Value(owner)],
                },
                view: proof,
            },
            at.clone(),
        )
        .expect("branch");
    let child = loan(
        function
            .append_instruction(
                some,
                Operation::SharedReborrow { source: proof },
                vec![proof_type],
                at.clone(),
            )
            .expect("child")
            .1[0],
    );
    let end = Operation::BorrowEnd { loan: child };
    if end_child_first {
        function
            .append_instruction(some, end.clone(), vec![], at.clone())
            .expect("end child");
    }
    append_value(
        function,
        some,
        Operation::NullableTake {
            owner: taken,
            proof,
        },
        integer,
        &at,
    );
    if !end_child_first {
        function
            .append_instruction(some, end, vec![], at.clone())
            .expect("end child");
    }
    for block in [none, some] {
        function
            .set_terminator(block, TerminatorKind::Return { values: vec![] }, at.clone())
            .expect("return");
    }
    (program, proof, child)
}

#[test]
fn copyable_nullable_take_rejects_a_live_proof_descendant() {
    verify_program(&copyable_proof(true).0).expect("take may end the proof after its child ends");
    let (invalid, parent, dependent) = copyable_proof(false);
    let errors = verify_program(&invalid).expect_err("take must keep the live child dependent");
    assert!(
        errors.errors.iter().any(|error| {
            error.kind == VerifyErrorKind::LoanDependencyActive { parent, dependent }
        }),
        "{errors:?}"
    );
}
