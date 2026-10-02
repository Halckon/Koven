//! Unit storage never erases the SSA operation's concrete type contract.

use lang_frontend::source::SourceMap;

use super::super::{
    model::{
        EntityId, EntityType, Operation, Origin, Program, ScalarConstant, SequentialContainerKind,
        SsaTypeKind, TerminatorKind,
    },
    verify::{VerifyErrorKind, VerifyLocation, verify_program},
};

#[test]
fn unit_constant_with_integer_result_fails_operation_contract() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("unit_constant_invalid.ko", "unit")
        .unwrap();
    let origin = Origin::Source(sources.span(source, 0, 4).unwrap());
    let mut program = Program::default();
    let module_id = program.add_module("unit_constant_invalid");
    let module = program.module_mut(module_id).unwrap();
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let function_id = module.add_function("main", vec![], origin.clone()).unwrap();
    let function = module.function_mut(function_id).unwrap();
    let entry = function.add_block(vec![], origin.clone()).unwrap();
    let (instruction, _) = function
        .append_instruction(
            entry,
            Operation::Constant(ScalarConstant::Unit),
            vec![EntityType::Value(integer)],
            origin.clone(),
        )
        .unwrap();
    function
        .set_terminator(entry, TerminatorKind::Return { values: vec![] }, origin)
        .unwrap();
    let errors = verify_program(&program).expect_err("Unit constant cannot masquerade as Int");
    assert!(
        errors.errors.iter().any(|error| {
            error.location == VerifyLocation::Instruction(instruction)
                && matches!(error.kind, VerifyErrorKind::OperationContract { .. })
        }),
        "{errors:?}"
    );
}

#[test]
fn unit_operand_for_integer_array_fails_operation_contract() {
    let mut sources = SourceMap::new();
    let source = sources
        .add_source("unit_operand_invalid.ko", "arrayOf")
        .unwrap();
    let origin = Origin::Source(sources.span(source, 0, 7).unwrap());
    let mut program = Program::default();
    let module_id = program.add_module("unit_operand_invalid");
    let module = program.module_mut(module_id).unwrap();
    let unit = module.intern_type(SsaTypeKind::Unit);
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let array = module
        .add_sequential_container_type(SequentialContainerKind::Array, integer)
        .unwrap();
    let function_id = module
        .add_function("source", vec![array], origin.clone())
        .unwrap();
    let function = module.function_mut(function_id).unwrap();
    let entry = function.add_block(vec![], origin.clone()).unwrap();
    let (_, operand) = function
        .append_instruction(
            entry,
            Operation::Constant(ScalarConstant::Unit),
            vec![EntityType::Value(unit)],
            origin.clone(),
        )
        .unwrap();
    let EntityId::Value(operand) = operand[0] else {
        panic!("Unit value")
    };
    let (instruction, container) = function
        .append_instruction(
            entry,
            Operation::ContainerConstruct {
                container: array,
                elements: vec![operand],
            },
            vec![EntityType::Value(array)],
            origin.clone(),
        )
        .unwrap();
    let EntityId::Value(container) = container[0] else {
        panic!("container value")
    };
    function
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: vec![container],
            },
            origin,
        )
        .unwrap();
    let errors = verify_program(&program).expect_err("Unit operand cannot inhabit Array<Int>");
    assert!(
        errors.errors.iter().any(|error| {
            error.location == VerifyLocation::Instruction(instruction)
                && matches!(error.kind, VerifyErrorKind::OperationContract { .. })
        }),
        "{errors:?}"
    );
}
