use lang_frontend::source::SourceMap;

use super::{
    model::{
        CheckedArithmeticOperator, ComparisonOperator, Edge, EntityId, EntityType, FunctionId,
        LoanId, LoanKind, Operation, Origin, PlaceId, Program, SsaTypeKind, TerminatorKind,
        ValueId,
    },
    render::render_program,
    verify::{VerifyErrorKind, verify_program},
};
use crate::llvm::render_verified_program;

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

fn place(entity: EntityId) -> PlaceId {
    let EntityId::Place(place) = entity else {
        panic!("expected place, got {entity:?}");
    };
    place
}

fn loan(entity: EntityId) -> LoanId {
    let EntityId::Loan(loan) = entity else {
        panic!("expected loan, got {entity:?}");
    };
    loan
}

#[test]
fn print_literal_requires_no_results_and_one_trailing_line_feed() {
    let origin = origin();
    let mut valid = Program::default();
    let module_id = valid.add_module("print-valid");
    let module = valid.module_mut(module_id).expect("module must exist");
    let function_id = module
        .add_function("print", Vec::new(), origin.clone())
        .expect("function must exist");
    let function = module
        .function_mut(function_id)
        .expect("function must exist");
    let entry = function
        .add_block(Vec::new(), origin.clone())
        .expect("entry must exist");
    function
        .append_instruction(
            entry,
            Operation::PrintLiteral {
                bytes: b"hello\n".to_vec(),
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("print operation must append");
    function
        .set_terminator(
            entry,
            TerminatorKind::Return { values: Vec::new() },
            origin.clone(),
        )
        .expect("function must return");
    verify_program(&valid).expect("newline-terminated print literal must verify");

    let mut invalid = valid;
    for bytes in [Vec::new(), b"missing-newline".to_vec()] {
        invalid
            .module_mut(module_id)
            .expect("module remains available")
            .function_mut(function_id)
            .expect("function remains available")
            .instructions[0]
            .operation = Operation::PrintLiteral { bytes };
        assert!(
            verify_program(&invalid)
                .expect_err("invalid stdout bytes must fail")
                .errors
                .iter()
                .any(|error| matches!(error.kind, VerifyErrorKind::OperationContract { .. }))
        );
    }
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
                arguments: vec![EntityId::Value(value(arguments[0]))],
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
fn direct_borrow_call_preserves_move_only_owner_and_uses_pointer_abi() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("borrow-call");
    let module = program.module_mut(module_id).expect("module must exist");
    let payload = module
        .add_aggregate_type("Resource.payload", Vec::new())
        .expect("payload must be valid");
    let owner = module
        .declare_heap_owner("Resource")
        .expect("owner must be valid");
    module
        .define_heap_owner(owner, payload)
        .expect("owner definition must be valid");

    let callee = module
        .add_function("inspect", Vec::new(), origin.clone())
        .expect("callee must be valid");
    let callee_function = module.function_mut(callee).expect("callee must exist");
    let callee_entry = callee_function
        .add_block(
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: owner,
            }],
            origin.clone(),
        )
        .expect("borrow parameter must be valid");
    callee_function
        .set_terminator(
            callee_entry,
            TerminatorKind::Return { values: Vec::new() },
            origin.clone(),
        )
        .expect("callee must return");

    let caller = module
        .add_function("caller", Vec::new(), origin.clone())
        .expect("caller must be valid");
    let caller_function = module.function_mut(caller).expect("caller must exist");
    let entry = caller_function
        .add_block(vec![EntityType::Value(owner)], origin.clone())
        .expect("caller entry must be valid");
    let owner_value = value(caller_function.block(entry).expect("entry").parameters[0]);
    let (_, places) = caller_function
        .append_instruction(
            entry,
            Operation::RootPlace { owner: owner_value },
            vec![EntityType::Place(owner)],
            origin.clone(),
        )
        .expect("root place must append");
    let (_, loans) = caller_function
        .append_instruction(
            entry,
            Operation::BorrowBegin {
                place: place(places[0]),
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: owner,
            }],
            origin.clone(),
        )
        .expect("borrow must append");
    let loan = loan(loans[0]);
    caller_function
        .append_instruction(
            entry,
            Operation::DirectCall {
                callee,
                arguments: vec![EntityId::Loan(loan)],
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("borrow call must append");
    caller_function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan },
            Vec::new(),
            origin.clone(),
        )
        .expect("borrow end must append");
    caller_function
        .append_instruction(
            entry,
            Operation::Drop { owner: owner_value },
            Vec::new(),
            origin.clone(),
        )
        .expect("owner must remain available after borrow call");
    caller_function
        .set_terminator(entry, TerminatorKind::Return { values: Vec::new() }, origin)
        .expect("caller must return");

    verify_program(&program).expect("borrow call must not consume its MoveOnly owner");
    let rendered = render_program(&program);
    assert!(rendered.contains("call @f0(%l0)"), "{rendered}");
    let llvm = render_verified_program(&program).expect("borrow call must lower to LLVM");
    assert!(
        llvm.contains("define internal void @f0.inspect(ptr %l0)"),
        "{llvm}"
    );
    assert!(llvm.contains("call void @f0.inspect(ptr %p0)"), "{llvm}");
    assert_eq!(llvm.matches("call void @free").count(), 1, "{llvm}");
}

#[test]
fn direct_call_rejects_value_for_borrow_and_wrong_loan_kind() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("invalid-borrow-call");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let shared = module
        .add_function("shared", Vec::new(), origin.clone())
        .expect("shared callee");
    let shared_function = module.function_mut(shared).expect("shared function");
    let shared_entry = shared_function
        .add_block(
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: integer,
            }],
            origin.clone(),
        )
        .expect("shared entry");
    shared_function
        .set_terminator(
            shared_entry,
            TerminatorKind::Return { values: Vec::new() },
            origin.clone(),
        )
        .expect("shared return");
    let exclusive = module
        .add_function("exclusive", Vec::new(), origin.clone())
        .expect("exclusive callee");
    let exclusive_function = module.function_mut(exclusive).expect("exclusive function");
    let exclusive_entry = exclusive_function
        .add_block(
            vec![EntityType::Loan {
                kind: LoanKind::Exclusive,
                target: integer,
            }],
            origin.clone(),
        )
        .expect("exclusive entry");
    exclusive_function
        .set_terminator(
            exclusive_entry,
            TerminatorKind::Return { values: Vec::new() },
            origin.clone(),
        )
        .expect("exclusive return");

    let caller = module
        .add_function("caller", Vec::new(), origin.clone())
        .expect("caller");
    let caller_function = module.function_mut(caller).expect("caller function");
    let entry = caller_function
        .add_block(vec![EntityType::Value(integer)], origin.clone())
        .expect("caller entry");
    let input = value(caller_function.block(entry).expect("entry").parameters[0]);
    let (_, places) = caller_function
        .append_instruction(
            entry,
            Operation::RootPlace { owner: input },
            vec![EntityType::Place(integer)],
            origin.clone(),
        )
        .expect("root place");
    let (_, loans) = caller_function
        .append_instruction(
            entry,
            Operation::BorrowBegin {
                place: place(places[0]),
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: integer,
            }],
            origin.clone(),
        )
        .expect("shared loan");
    let shared_loan = loan(loans[0]);
    for (callee, argument) in [
        (shared, EntityId::Value(input)),
        (exclusive, EntityId::Loan(shared_loan)),
    ] {
        caller_function
            .append_instruction(
                entry,
                Operation::DirectCall {
                    callee,
                    arguments: vec![argument],
                },
                Vec::new(),
                origin.clone(),
            )
            .expect("invalid call remains structurally appendable");
    }
    caller_function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan: shared_loan },
            Vec::new(),
            origin.clone(),
        )
        .expect("loan end");
    caller_function
        .set_terminator(entry, TerminatorKind::Abort, origin)
        .expect("caller abort");

    let errors = verify_program(&program).expect_err("delivery mode mismatches must fail");
    assert_eq!(
        errors
            .errors
            .iter()
            .filter(|error| matches!(error.kind, VerifyErrorKind::OperationContract { .. }))
            .count(),
        2,
        "{errors:#?}"
    );
}

#[test]
fn direct_call_rejects_inactive_borrow_argument() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("inactive-borrow-call");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let callee = module
        .add_function("inspect", Vec::new(), origin.clone())
        .expect("callee");
    let callee_function = module.function_mut(callee).expect("callee function");
    let callee_entry = callee_function
        .add_block(
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: integer,
            }],
            origin.clone(),
        )
        .expect("callee entry");
    callee_function
        .set_terminator(
            callee_entry,
            TerminatorKind::Return { values: Vec::new() },
            origin.clone(),
        )
        .expect("callee return");

    let caller = module
        .add_function("caller", Vec::new(), origin.clone())
        .expect("caller");
    let caller_function = module.function_mut(caller).expect("caller function");
    let entry = caller_function
        .add_block(vec![EntityType::Value(integer)], origin.clone())
        .expect("caller entry");
    let input = value(caller_function.block(entry).expect("entry").parameters[0]);
    let (_, places) = caller_function
        .append_instruction(
            entry,
            Operation::RootPlace { owner: input },
            vec![EntityType::Place(integer)],
            origin.clone(),
        )
        .expect("root place");
    let (_, loans) = caller_function
        .append_instruction(
            entry,
            Operation::BorrowBegin {
                place: place(places[0]),
                kind: LoanKind::Shared,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: integer,
            }],
            origin.clone(),
        )
        .expect("shared loan");
    let shared_loan = loan(loans[0]);
    caller_function
        .append_instruction(
            entry,
            Operation::BorrowEnd { loan: shared_loan },
            Vec::new(),
            origin.clone(),
        )
        .expect("loan end");
    caller_function
        .append_instruction(
            entry,
            Operation::DirectCall {
                callee,
                arguments: vec![EntityId::Loan(shared_loan)],
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("inactive call remains structurally appendable");
    caller_function
        .set_terminator(entry, TerminatorKind::Abort, origin)
        .expect("caller abort");

    let errors = verify_program(&program).expect_err("inactive loan call must fail");
    assert!(
        errors
            .errors
            .iter()
            .any(|error| matches!(error.kind, VerifyErrorKind::LoanInactive { loan } if loan == shared_loan)),
        "{errors:#?}"
    );
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
