mod borrowed_calls;

use crate::test_support::diagnostics::{assert_diagnostic, assert_eq_diagnostic};
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
                receiver: None,
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
fn shared_reborrow_blocks_parent_end_and_exclusive_call_until_child_end() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("shared-reborrow");
    let module = program.module_mut(module_id).expect("module must exist");
    let owner = module
        .declare_heap_owner("Owner")
        .expect("owner declaration must succeed");
    let payload = module
        .add_aggregate_type("Owner.payload", Vec::new())
        .expect("payload must be valid");
    module
        .define_heap_owner(owner, payload)
        .expect("owner definition must succeed");
    let exclusive = EntityType::Loan {
        kind: LoanKind::Exclusive,
        target: owner,
    };

    let callee = module
        .add_instance_function("mutate", exclusive, Vec::new(), origin.clone())
        .expect("callee must be valid");
    let callee_function = module.function_mut(callee).expect("callee must exist");
    let callee_entry = callee_function
        .add_block(vec![exclusive], origin.clone())
        .expect("callee entry must be valid");
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
    let caller_entry = caller_function
        .add_block(vec![exclusive], origin.clone())
        .expect("caller entry must be valid");
    let parent = loan(caller_function.blocks[0].parameters[0]);
    let reborrow_block = caller_function
        .add_block(vec![exclusive], origin.clone())
        .expect("reborrow block must be valid");
    caller_function
        .set_terminator(
            caller_entry,
            TerminatorKind::Branch(Edge {
                target: reborrow_block,
                arguments: vec![EntityId::Loan(parent)],
            }),
            origin.clone(),
        )
        .expect("parent loan must cross the first edge");
    let parent_rebound = loan(
        caller_function
            .block(reborrow_block)
            .expect("reborrow block exists")
            .parameters[0],
    );
    let (_, reborrowed) = caller_function
        .append_instruction(
            reborrow_block,
            Operation::SharedReborrow {
                source: parent_rebound,
            },
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: owner,
            }],
            origin.clone(),
        )
        .expect("shared reborrow must append");
    let child = loan(reborrowed[0]);
    let successor = caller_function
        .add_block(
            vec![
                exclusive,
                EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: owner,
                },
            ],
            origin.clone(),
        )
        .expect("successor must be valid");
    caller_function
        .set_terminator(
            reborrow_block,
            TerminatorKind::Branch(Edge {
                target: successor,
                arguments: vec![EntityId::Loan(parent_rebound), EntityId::Loan(child)],
            }),
            origin.clone(),
        )
        .expect("parent and child loans must cross the edge");
    let successor_parameters = caller_function
        .block(successor)
        .expect("successor exists")
        .parameters
        .clone();
    let parent_phi = loan(successor_parameters[0]);
    let child_phi = loan(successor_parameters[1]);
    caller_function
        .append_instruction(
            successor,
            Operation::DirectCall {
                callee,
                receiver: Some(EntityId::Loan(parent_phi)),
                arguments: Vec::new(),
            },
            Vec::new(),
            origin.clone(),
        )
        .expect("exclusive call shape must append");
    caller_function
        .append_instruction(
            successor,
            Operation::BorrowEnd { loan: parent_phi },
            Vec::new(),
            origin.clone(),
        )
        .expect("parent end shape must append");
    caller_function
        .set_terminator(
            successor,
            TerminatorKind::Return { values: Vec::new() },
            origin,
        )
        .expect("caller must return");

    let errors = verify_program(&program).expect_err("active child must suspend its parent loan");
    assert_eq!(
        errors
            .errors
            .iter()
            .filter(|error| matches!(
                error.kind,
                VerifyErrorKind::LoanDependencyActive {
                    parent: actual_parent,
                    dependent,
                } if actual_parent == parent_phi && dependent == child_phi
            ))
            .count(),
        2
    );
    assert!(errors.errors.iter().any(|error| matches!(
        error.kind,
        VerifyErrorKind::ActiveLoanAtExit { loan } if loan == child_phi
    )));
}

#[test]
fn entry_borrow_parameter_is_function_scoped_across_cfg_blocks() {
    let origin = origin();
    let mut program = Program::default();
    let module_id = program.add_module("borrow-parameter-cfg");
    let module = program.module_mut(module_id).expect("module must exist");
    let integer = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let function_id = module
        .add_function("read", vec![integer], origin.clone())
        .expect("function");
    let function = module.function_mut(function_id).expect("function");
    let entry = function
        .add_block(
            vec![EntityType::Loan {
                kind: LoanKind::Shared,
                target: integer,
            }],
            origin.clone(),
        )
        .expect("entry");
    let entry_loan = loan(function.block(entry).expect("entry").parameters[0]);
    let body = function
        .add_block(Vec::new(), origin.clone())
        .expect("body");
    function
        .set_terminator(
            entry,
            TerminatorKind::Branch(Edge {
                target: body,
                arguments: Vec::new(),
            }),
            origin.clone(),
        )
        .expect("entry branch");
    let (_, results) = function
        .append_instruction(
            body,
            Operation::Read {
                source: super::model::PlaceAccess::Loan(entry_loan),
            },
            vec![EntityType::Value(integer)],
            origin.clone(),
        )
        .expect("loan read");
    function
        .set_terminator(
            body,
            TerminatorKind::Return {
                values: vec![value(results[0])],
            },
            origin,
        )
        .expect("return");

    verify_program(&program).expect("entry Borrow parameter must dominate the callable CFG");
    let llvm = render_verified_program(&program).expect("entry Borrow parameter must lower");
    assert_diagnostic!(
        llvm.contains("define internal i32 @f0.read(ptr %l0)"),
        "{llvm}"
    );
    assert_diagnostic!(llvm.contains("load i32, ptr %l0"), "{llvm}");
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
                receiver: None,
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
