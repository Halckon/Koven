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

#[test]
fn checked_conversion_rejects_invalid_integer_and_result_contracts() {
    let integer = SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    };
    let invalid = [
        (
            SsaTypeKind::Boolean,
            vec![integer.clone(), SsaTypeKind::Boolean],
        ),
        (
            SsaTypeKind::Integer {
                bits: 128,
                signed: true,
            },
            vec![integer.clone(), SsaTypeKind::Boolean],
        ),
        (
            integer.clone(),
            vec![SsaTypeKind::Char, SsaTypeKind::Boolean],
        ),
        (
            integer.clone(),
            vec![
                SsaTypeKind::Integer {
                    bits: 7,
                    signed: false,
                },
                SsaTypeKind::Boolean,
            ],
        ),
        (integer.clone(), vec![integer.clone()]),
        (
            integer.clone(),
            vec![integer.clone(), SsaTypeKind::Boolean, SsaTypeKind::Boolean],
        ),
        (integer.clone(), vec![integer.clone(), integer]),
    ];
    for (source, results) in invalid {
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
}
