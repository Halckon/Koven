//! CI-only synthetic verifier negative contracts. Never select in ordinary-source local manifests.
use super::{
    model::{EntityId, EntityType, Operation, Origin, Program, SsaTypeKind, TerminatorKind},
    verify::{VerifyErrorKind, verify_program},
};
use lang_frontend::source::SourceMap;

fn fixture(source: SsaTypeKind, results: Vec<SsaTypeKind>) -> Program {
    let mut sources = SourceMap::new();
    let source_id = sources.add_source("contract.ko", "contract").unwrap();
    let origin = Origin::Source(sources.span(source_id, 0, 8).unwrap());
    let mut program = Program::default();
    let module_id = program.add_module("conversion-contract");
    let module = program.module_mut(module_id).unwrap();
    let source = module.intern_type(source);
    let result_types = results
        .into_iter()
        .map(|ty| EntityType::Value(module.intern_type(ty)))
        .collect();
    let function_id = module
        .add_function("conversion-contract", vec![], origin.clone())
        .unwrap();
    let function = module.function_mut(function_id).unwrap();
    let entry = function
        .add_block(vec![EntityType::Value(source)], origin.clone())
        .unwrap();
    let EntityId::Value(operand) = function.block(entry).unwrap().parameters[0] else {
        panic!("value");
    };
    function
        .append_instruction(
            entry,
            Operation::CheckedIntegerConvert { operand },
            result_types,
            origin.clone(),
        )
        .unwrap();
    function
        .set_terminator(entry, TerminatorKind::Return { values: vec![] }, origin)
        .unwrap();
    program
}

fn integer() -> SsaTypeKind {
    SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    }
}

fn assert_invalid(source: SsaTypeKind, results: Vec<SsaTypeKind>) {
    let errors =
        verify_program(&fixture(source, results)).expect_err("invalid conversion contract");
    assert!(
        errors
            .errors
            .iter()
            .any(|error| matches!(error.kind, VerifyErrorKind::OperationContract { .. })),
        "{errors:?}"
    );
}

#[test]
fn rejects_boolean_source() {
    assert_invalid(SsaTypeKind::Boolean, vec![integer(), SsaTypeKind::Boolean]);
}

#[test]
fn rejects_non_language_integer_source() {
    assert_invalid(
        SsaTypeKind::Integer {
            bits: 128,
            signed: true,
        },
        vec![integer(), SsaTypeKind::Boolean],
    );
}

#[test]
fn rejects_char_target() {
    assert_invalid(integer(), vec![SsaTypeKind::Char, SsaTypeKind::Boolean]);
}

#[test]
fn rejects_non_language_integer_target() {
    assert_invalid(
        integer(),
        vec![
            SsaTypeKind::Integer {
                bits: 7,
                signed: false,
            },
            SsaTypeKind::Boolean,
        ],
    );
}

#[test]
fn rejects_missing_failure_result() {
    assert_invalid(integer(), vec![integer()]);
}

#[test]
fn rejects_extra_result() {
    assert_invalid(
        integer(),
        vec![integer(), SsaTypeKind::Boolean, SsaTypeKind::Boolean],
    );
}

#[test]
fn rejects_non_boolean_failure_result() {
    assert_invalid(integer(), vec![integer(), integer()]);
}
