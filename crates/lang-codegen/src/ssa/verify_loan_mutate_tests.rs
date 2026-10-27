//! SPEC-0358 `LoanMutate`: replacing the Copyable target of an `Inout` loan parameter.
use lang_frontend::source::SourceMap;

use super::{
    model::{
        EntityId, EntityType, LoanKind, Operation, Origin, Ownership, Program, SsaTypeKind,
        TerminatorKind,
    },
    verify::{VerifyErrorKind, verify_program},
};

fn origin() -> Origin {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("loan-mutate.ko", "fun bump")
        .expect("test source must be unique");
    Origin::Source(sources.span(source, 0, 3).expect("test span must be valid"))
}

/// `fun f(<loan> x: T, own v: U) { mutate x, v }` where the loan is `kind` of `target`.
fn program(kind: LoanKind, target: SsaTypeKind, value: SsaTypeKind) -> Program {
    let origin = origin();
    let mut program = Program::default();
    let module = program.add_module("main");
    let module = program.module_mut(module).expect("module exists");
    let target = module.intern_type(target);
    let value = module.intern_type(value);
    let function = module
        .add_function("bump", Vec::new(), origin.clone())
        .expect("signature is valid");
    let function = module.function_mut(function).expect("function exists");
    let entry = function
        .add_block(
            vec![EntityType::Loan { kind, target }, EntityType::Value(value)],
            origin.clone(),
        )
        .expect("entry is valid");
    let parameters = function.block(entry).expect("entry").parameters.clone();
    let (EntityId::Loan(loan), EntityId::Value(value)) = (parameters[0], parameters[1]) else {
        panic!("loan and value parameters");
    };
    function
        .append_instruction(
            entry,
            Operation::LoanMutate { loan, value },
            Vec::new(),
            origin.clone(),
        )
        .expect("instruction is well formed");
    function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("terminator is valid");
    program
}

const INT: SsaTypeKind = SsaTypeKind::Integer {
    bits: 32,
    signed: true,
};

fn contract_error(program: &Program) -> bool {
    verify_program(program).is_err_and(|failure| {
        failure
            .errors
            .iter()
            .any(|error| matches!(error.kind, VerifyErrorKind::OperationContract { .. }))
    })
}

#[test]
fn loan_mutate_replaces_a_copyable_target_of_an_exclusive_loan() {
    assert_eq!(
        verify_program(&program(LoanKind::Exclusive, INT, INT)),
        Ok(())
    );
}

#[test]
fn loan_mutate_rejects_shared_loans_mismatched_and_move_only_targets() {
    assert!(contract_error(&program(LoanKind::Shared, INT, INT)));
    assert!(contract_error(&program(
        LoanKind::Exclusive,
        INT,
        SsaTypeKind::Boolean
    )));
    let move_only = || SsaTypeKind::Opaque {
        name: "Endpoint".to_owned(),
        ownership: Ownership::MoveOnly,
    };
    assert!(contract_error(&program(
        LoanKind::Exclusive,
        move_only(),
        move_only()
    )));
}
