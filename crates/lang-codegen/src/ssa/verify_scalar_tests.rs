use lang_frontend::source::SourceMap;

use super::{
    model::{
        CheckedArithmeticOperator, ComparisonOperator, Edge, EntityId, EntityType, FunctionId,
        Operation, Origin, Program, SsaTypeKind, TerminatorKind, ValueId,
    },
    render::render_program,
    verify::{VerifyErrorKind, verify_program},
};

fn origin() -> Origin {
    let mut sources = SourceMap::default();
    let source = sources
        .add_source("scalar-contract.ko", "fun scalar")
        .expect("test source must be unique");
    Origin::Source(
        sources
            .span(source, 0, 10)
            .expect("test span must be valid"),
    )
}

fn value(entity: EntityId) -> ValueId {
    let EntityId::Value(value) = entity else {
        panic!("expected value, got {entity:?}");
    };
    value
}

#[test]
fn checked_arithmetic_exposes_failure_and_routes_it_to_abort() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("checked");
    let module = program.module_mut(module_id).expect("module must exist");
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 64,
        signed: true,
    });

    for operator in [
        CheckedArithmeticOperator::Add,
        CheckedArithmeticOperator::Subtract,
        CheckedArithmeticOperator::Multiply,
        CheckedArithmeticOperator::Divide,
        CheckedArithmeticOperator::Remainder,
    ] {
        let function_id = module
            .add_function(format!("{operator:?}"), vec![integer], origin.clone())
            .expect("signature must be valid");
        let function = module
            .function_mut(function_id)
            .expect("function must exist");
        let entry = function
            .add_block(
                vec![EntityType::Value(integer), EntityType::Value(integer)],
                origin.clone(),
            )
            .expect("entry must be valid");
        let success = function
            .add_block(vec![EntityType::Value(integer)], origin.clone())
            .expect("success block must be valid");
        let failure = function
            .add_block(Vec::new(), origin.clone())
            .expect("failure block must be valid");
        let parameters = function
            .block(entry)
            .expect("entry must exist")
            .parameters
            .clone();
        let (_, results) = function
            .append_instruction(
                entry,
                Operation::CheckedArithmetic {
                    operator,
                    left: value(parameters[0]),
                    right: value(parameters[1]),
                },
                vec![EntityType::Value(integer), EntityType::Value(boolean)],
                origin.clone(),
            )
            .expect("checked operation must be appendable");
        function
            .set_terminator(
                entry,
                TerminatorKind::Conditional {
                    condition: value(results[1]),
                    when_true: Edge {
                        target: failure,
                        arguments: Vec::new(),
                    },
                    when_false: Edge {
                        target: success,
                        arguments: vec![results[0]],
                    },
                },
                origin.clone(),
            )
            .expect("failure flag must form CFG");
        function
            .set_terminator(failure, TerminatorKind::Abort, origin.clone())
            .expect("failure must abort");
        let result = value(
            function
                .block(success)
                .expect("success must exist")
                .parameters[0],
        );
        function
            .set_terminator(
                success,
                TerminatorKind::Return {
                    values: vec![result],
                },
                origin.clone(),
            )
            .expect("success must return");
    }

    verify_program(&program).expect("all checked scalar contracts must verify");
    let rendered = render_program(&program);
    for operation in ["add", "sub", "mul", "div", "rem"] {
        assert!(rendered.contains(&format!("checked.{operation}")));
    }
    assert_eq!(rendered.matches("abort @source").count(), 5);
}

#[test]
fn comparisons_and_direct_calls_have_exact_scalar_signatures() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("calls");
    let module = program.module_mut(module_id).expect("module must exist");
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: false,
    });

    let callee = module
        .add_function("identity", vec![integer], origin.clone())
        .expect("callee signature must be valid");
    let callee_function = module.function_mut(callee).expect("callee must exist");
    let callee_entry = callee_function
        .add_block(vec![EntityType::Value(integer)], origin.clone())
        .expect("callee entry must be valid");
    let callee_result = value(
        callee_function
            .block(callee_entry)
            .expect("callee entry must exist")
            .parameters[0],
    );
    callee_function
        .set_terminator(
            callee_entry,
            TerminatorKind::Return {
                values: vec![callee_result],
            },
            origin.clone(),
        )
        .expect("callee return must be valid");

    let caller = module
        .add_function("caller", vec![boolean], origin.clone())
        .expect("caller signature must be valid");
    let caller_function = module.function_mut(caller).expect("caller must exist");
    let caller_entry = caller_function
        .add_block(
            vec![EntityType::Value(integer), EntityType::Value(integer)],
            origin.clone(),
        )
        .expect("caller entry must be valid");
    let arguments = caller_function
        .block(caller_entry)
        .expect("caller entry must exist")
        .parameters
        .clone();
    let (_, call_result) = caller_function
        .append_instruction(
            caller_entry,
            Operation::DirectCall {
                callee,
                arguments: vec![value(arguments[0])],
            },
            vec![EntityType::Value(integer)],
            origin.clone(),
        )
        .expect("call must be appendable");

    let mut last = None;
    for operator in [
        ComparisonOperator::Equal,
        ComparisonOperator::NotEqual,
        ComparisonOperator::LessThan,
        ComparisonOperator::LessThanOrEqual,
        ComparisonOperator::GreaterThan,
        ComparisonOperator::GreaterThanOrEqual,
    ] {
        let (_, result) = caller_function
            .append_instruction(
                caller_entry,
                Operation::Compare {
                    operator,
                    left: value(call_result[0]),
                    right: value(arguments[1]),
                },
                vec![EntityType::Value(boolean)],
                origin.clone(),
            )
            .expect("comparison must be appendable");
        last = Some(value(result[0]));
    }
    let (_, negated) = caller_function
        .append_instruction(
            caller_entry,
            Operation::BooleanNot {
                operand: last.expect("comparison matrix must be non-empty"),
            },
            vec![EntityType::Value(boolean)],
            origin.clone(),
        )
        .expect("Boolean not must be appendable");
    caller_function
        .set_terminator(
            caller_entry,
            TerminatorKind::Return {
                values: vec![value(negated[0])],
            },
            origin,
        )
        .expect("caller return must be valid");

    verify_program(&program).expect("scalar call and comparison contracts must verify");
    let rendered = render_program(&program);
    assert!(rendered.contains("call @f0("));
    assert!(rendered.contains("not %v"));
    for comparison in ["eq", "ne", "lt", "le", "gt", "ge"] {
        assert!(rendered.contains(&format!("cmp.{comparison}")));
    }
}

#[test]
fn malformed_checked_results_and_call_signatures_are_rejected() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("invalid");
    let module = program.module_mut(module_id).expect("module must exist");
    let boolean = module.intern_type(SsaTypeKind::Boolean);
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });

    let callee = module
        .add_function("callee", vec![integer], origin.clone())
        .expect("callee signature must be valid");
    let callee_function = module.function_mut(callee).expect("callee must exist");
    let callee_entry = callee_function
        .add_block(vec![EntityType::Value(integer)], origin.clone())
        .expect("callee entry must be valid");
    let callee_value = value(
        callee_function
            .block(callee_entry)
            .expect("callee entry must exist")
            .parameters[0],
    );
    callee_function
        .set_terminator(
            callee_entry,
            TerminatorKind::Return {
                values: vec![callee_value],
            },
            origin.clone(),
        )
        .expect("callee must return");

    let caller = module
        .add_function("caller", Vec::new(), origin.clone())
        .expect("caller signature must be valid");
    let caller_function = module.function_mut(caller).expect("caller must exist");
    let entry = caller_function
        .add_block(
            vec![EntityType::Value(integer), EntityType::Value(integer)],
            origin.clone(),
        )
        .expect("caller entry must be valid");
    let parameters = caller_function
        .block(entry)
        .expect("entry must exist")
        .parameters
        .clone();
    caller_function
        .append_instruction(
            entry,
            Operation::CheckedArithmetic {
                operator: CheckedArithmeticOperator::Add,
                left: value(parameters[0]),
                right: value(parameters[1]),
            },
            vec![EntityType::Value(integer), EntityType::Value(integer)],
            origin.clone(),
        )
        .expect("model accepts types before semantic verification");
    caller_function
        .append_instruction(
            entry,
            Operation::DirectCall {
                callee,
                arguments: Vec::new(),
            },
            vec![EntityType::Value(boolean)],
            origin.clone(),
        )
        .expect("model accepts call shape before semantic verification");
    caller_function
        .append_instruction(
            entry,
            Operation::BooleanNot {
                operand: value(parameters[0]),
            },
            vec![EntityType::Value(boolean)],
            origin.clone(),
        )
        .expect("model accepts prefix shape before semantic verification");
    caller_function
        .set_terminator(entry, TerminatorKind::Abort, origin)
        .expect("invalid fixture still needs structural terminator");

    let errors = verify_program(&program).expect_err("all malformed operation contracts must fail");
    assert_eq!(
        errors
            .errors
            .iter()
            .filter(|error| matches!(error.kind, VerifyErrorKind::OperationContract { .. }))
            .count(),
        3
    );

    let mut dangling = program;
    let caller_function = dangling
        .module_mut(module_id)
        .expect("module must remain available")
        .function_mut(caller)
        .expect("caller must remain available");
    let call = caller_function.blocks[entry.index()].instructions[1];
    let Operation::DirectCall { callee, .. } =
        &mut caller_function.instructions[call.index()].operation
    else {
        panic!("second instruction must be a direct call");
    };
    *callee = FunctionId {
        module: module_id,
        index: 999,
    };
    assert!(
        verify_program(&dangling)
            .expect_err("unknown callee must fail")
            .errors
            .iter()
            .any(|error| matches!(error.kind, VerifyErrorKind::OperationContract { .. }))
    );
}
