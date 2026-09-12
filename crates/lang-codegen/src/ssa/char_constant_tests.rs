//! SPEC-0209: Char remains a distinct Unicode scalar through SSA and LLVM.
use lang_frontend::source::SourceMap;

use super::{
    model::{
        EntityId, EntityType, Operation, Origin, Program, ScalarConstant, SsaTypeKind,
        TerminatorKind,
    },
    render::render_program,
    verify::{VerifyErrorKind, verify_program},
};
use crate::llvm::render_verified_program;

fn constant_program(constant: ScalarConstant, result: SsaTypeKind) -> Program {
    let mut sources = SourceMap::new();
    let source = sources.add_source("char.ko", "fun charValue").unwrap();
    let origin = Origin::Source(sources.span(source, 0, 13).unwrap());
    let mut program = Program::default();
    let module_id = program.add_module("char");
    let module = program.module_mut(module_id).unwrap();
    let char_type = module.intern_type(SsaTypeKind::Char);
    let uint_type = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: false,
    });
    assert_ne!(char_type, uint_type, "Char must not be erased to UInt32");
    let result = module.intern_type(result);
    let function_id = module
        .add_function("char_value", vec![result], origin.clone())
        .unwrap();
    let function = module.function_mut(function_id).unwrap();
    let entry = function.add_block(Vec::new(), origin.clone()).unwrap();
    let (_, values) = function
        .append_instruction(
            entry,
            Operation::Constant(constant),
            vec![EntityType::Value(result)],
            origin.clone(),
        )
        .unwrap();
    let EntityId::Value(value) = values[0] else {
        panic!("constant must produce value")
    };
    function
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: vec![value],
            },
            origin,
        )
        .unwrap();
    program
}

#[test]
fn char_constants_preserve_unicode_boundaries_and_llvm_i32() {
    for codepoint in [0, 0xd7ff, 0xe000, 0x10ffff] {
        let program = constant_program(ScalarConstant::Char(codepoint), SsaTypeKind::Char);
        verify_program(&program).expect("Unicode scalar must verify");
        let ssa = render_program(&program);
        assert!(ssa.contains("!t0 = char"), "{ssa}");
        assert!(ssa.contains(&format!("U+{codepoint:04X}")), "{ssa}");
        let llvm = render_verified_program(&program).expect("Char must lower to LLVM");
        assert!(llvm.contains(&format!("ret i32 {codepoint}")), "{llvm}");
        assert_eq!(llvm, render_verified_program(&program).unwrap());
    }
}

#[test]
fn char_constants_reject_surrogates_out_of_range_and_integer_type_erasure() {
    for (constant, ty) in [
        (ScalarConstant::Char(0xd800), SsaTypeKind::Char),
        (ScalarConstant::Char(0xdfff), SsaTypeKind::Char),
        (ScalarConstant::Char(0x110000), SsaTypeKind::Char),
        (ScalarConstant::Char(u32::MAX), SsaTypeKind::Char),
        (ScalarConstant::Integer(65), SsaTypeKind::Char),
        (
            ScalarConstant::Char(65),
            SsaTypeKind::Integer {
                bits: 32,
                signed: false,
            },
        ),
    ] {
        let program = constant_program(constant, ty);
        let failure = verify_program(&program).expect_err("invalid Char contract");
        assert!(
            failure
                .errors
                .iter()
                .any(|error| matches!(error.kind, VerifyErrorKind::OperationContract { .. })),
            "{failure:?}"
        );
        assert!(render_verified_program(&program).is_err());
    }
}

fn parameter_program(
    operation: impl FnOnce(super::model::ValueId, super::model::ValueId) -> Operation,
    boolean_result: bool,
) -> Program {
    let mut program = constant_program(ScalarConstant::Char(65), SsaTypeKind::Char);
    let module = &mut program.modules[0];
    let char_type = module.intern_type(SsaTypeKind::Char);
    let result_type = if boolean_result {
        module.intern_type(SsaTypeKind::Boolean)
    } else {
        char_type
    };
    let origin = module.functions[0].origin.clone();
    let identity = module.functions[0].id;
    let function_id = module
        .add_function("char_compare", vec![result_type], origin.clone())
        .unwrap();
    let function = module.function_mut(function_id).unwrap();
    let entry = function
        .add_block(
            vec![EntityType::Value(char_type), EntityType::Value(char_type)],
            origin.clone(),
        )
        .unwrap();
    let parameters = function.block(entry).unwrap().parameters.clone();
    let EntityId::Value(left) = parameters[0] else {
        panic!("parameter")
    };
    let EntityId::Value(right) = parameters[1] else {
        panic!("parameter")
    };
    // Copy and direct-call results retain the distinct Char type through first-class values.
    let (_, copy) = function
        .append_instruction(
            entry,
            Operation::Copy { source: left },
            vec![EntityType::Value(char_type)],
            origin.clone(),
        )
        .unwrap();
    let EntityId::Value(copy) = copy[0] else {
        panic!("copy")
    };
    function
        .append_instruction(
            entry,
            Operation::DirectCall {
                callee: identity,
                receiver: None,
                arguments: Vec::new(),
            },
            vec![EntityType::Value(char_type)],
            origin.clone(),
        )
        .unwrap();
    let (_, result) = function
        .append_instruction(
            entry,
            operation(copy, right),
            vec![EntityType::Value(result_type)],
            origin.clone(),
        )
        .unwrap();
    let EntityId::Value(result) = result[0] else {
        panic!("operation")
    };
    function
        .set_terminator(
            entry,
            TerminatorKind::Return {
                values: vec![result],
            },
            origin,
        )
        .unwrap();
    program
}

#[test]
fn char_parameters_copy_call_and_compare_without_integer_coercion() {
    use super::model::ComparisonOperator;
    for (operator, llvm_operator) in [
        (ComparisonOperator::Equal, "eq"),
        (ComparisonOperator::NotEqual, "ne"),
    ] {
        let program = parameter_program(
            |left, right| Operation::Compare {
                operator,
                left,
                right,
            },
            true,
        );
        verify_program(&program).unwrap();
        let llvm = render_verified_program(&program).unwrap();
        assert!(
            llvm.contains(&format!("icmp {llvm_operator} i32")),
            "{llvm}"
        );
    }
}

#[test]
fn char_parameters_reject_integer_arithmetic_and_ordering() {
    use super::model::{BinaryOperator, ComparisonOperator};
    for operator in [
        BinaryOperator::Add,
        BinaryOperator::Subtract,
        BinaryOperator::Multiply,
    ] {
        let program = parameter_program(
            |left, right| Operation::Binary {
                operator,
                left,
                right,
            },
            false,
        );
        assert!(
            verify_program(&program)
                .unwrap_err()
                .errors
                .iter()
                .any(|error| matches!(error.kind, VerifyErrorKind::OperationContract { .. }))
        );
    }
    for operator in [
        ComparisonOperator::LessThan,
        ComparisonOperator::LessThanOrEqual,
        ComparisonOperator::GreaterThan,
        ComparisonOperator::GreaterThanOrEqual,
    ] {
        let program = parameter_program(
            |left, right| Operation::Compare {
                operator,
                left,
                right,
            },
            true,
        );
        assert!(
            verify_program(&program)
                .unwrap_err()
                .errors
                .iter()
                .any(|error| matches!(error.kind, VerifyErrorKind::OperationContract { .. }))
        );
    }
}
