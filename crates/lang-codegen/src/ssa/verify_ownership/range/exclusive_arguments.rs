use lang_frontend::source::SourceMap;

use super::*;
use crate::ssa::{
    model::{Origin, Program, ScalarConstant, SequentialContainerKind, SsaTypeKind},
    verify::verify_program,
};

fn forwarding_range_call(
    auxiliary: SsaTypeKind,
    keep_child: bool,
) -> (Program, crate::ssa::model::FunctionId) {
    let mut sources = SourceMap::default();
    let file = sources.add_source("range-inout.ko", "fun carrier").unwrap();
    let origin = Origin::Source(sources.span(file, 0, 3).unwrap());
    let mut program = Program::default();
    let module_id = program.add_module("main");
    let module = program.module_mut(module_id).unwrap();
    let int = module.intern_type(SsaTypeKind::Integer {
        bits: 32,
        signed: true,
    });
    let auxiliary = module.intern_type(auxiliary);
    let list = module
        .add_sequential_container_type(SequentialContainerKind::List, int)
        .unwrap();
    let view = module.intern_type(SsaTypeKind::RangeView { source: list });
    let root_ty = EntityType::Loan {
        kind: LoanKind::Shared,
        target: list,
    };
    let auxiliary_ty = EntityType::Loan {
        kind: LoanKind::Exclusive,
        target: auxiliary,
    };
    let callee = module
        .add_function("carrier", vec![view], origin.clone())
        .unwrap();
    let caller = module
        .add_function("forward", vec![view], origin.clone())
        .unwrap();

    for id in [callee, caller] {
        let function = module.function_mut(id).unwrap();
        function.carrier_return = Some(0);
        let block = function
            .add_block(vec![root_ty, auxiliary_ty, auxiliary_ty], origin.clone())
            .unwrap();
        let arguments = function.block(block).unwrap().parameters.clone();
        let EntityId::Loan(source) = arguments[0] else {
            panic!("range source")
        };
        let child = if id == caller && keep_child {
            let EntityId::Loan(parent) = arguments[1] else {
                panic!("Inout parameter")
            };
            let (_, children) = function
                .append_instruction(
                    block,
                    Operation::SharedReborrow { source: parent },
                    vec![EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: auxiliary,
                    }],
                    origin.clone(),
                )
                .unwrap();
            let EntityId::Loan(child) = children[0] else {
                panic!("shared child")
            };
            Some(child)
        } else {
            None
        };
        let operation = if id == callee {
            let (_, values) = function
                .append_instruction(
                    block,
                    Operation::Constant(ScalarConstant::Integer(0)),
                    vec![EntityType::Value(int)],
                    origin.clone(),
                )
                .unwrap();
            let EntityId::Value(bound) = values[0] else {
                panic!("range bound")
            };
            Operation::RangeConstruct {
                view,
                source,
                begin: bound,
                end: bound,
            }
        } else {
            Operation::RangeCall {
                callee,
                arguments,
                source,
            }
        };
        let (_, results) = function
            .append_instruction(
                block,
                operation,
                vec![EntityType::Value(view), root_ty],
                origin.clone(),
            )
            .unwrap();
        let [EntityId::Value(view), EntityId::Loan(source)] = results.as_slice() else {
            panic!("range result and its source")
        };
        if let Some(loan) = child {
            function
                .append_instruction(block, Operation::BorrowEnd { loan }, vec![], origin.clone())
                .unwrap();
        }
        function
            .set_terminator(
                block,
                TerminatorKind::RangeReturn {
                    view: *view,
                    source: *source,
                },
                origin.clone(),
            )
            .unwrap();
    }
    (program, caller)
}

#[test]
fn range_call_rejects_the_same_exclusive_loan_in_two_auxiliary_slots() {
    for auxiliary in [SsaTypeKind::Boolean, SsaTypeKind::StringOwner] {
        let (mut program, caller) = forwarding_range_call(auxiliary, false);
        verify_program(&program).expect("two independent exclusive loans may be forwarded");
        crate::llvm::render_verified_program(&program).expect("valid RangeCall reaches LLVM");
        let function = program
            .module_mut(caller.module())
            .unwrap()
            .function_mut(caller)
            .unwrap();
        let call = &mut function.instructions[0];
        let Operation::RangeCall { arguments, .. } = &mut call.operation else {
            unreachable!()
        };
        assert_ne!(arguments[1], arguments[2]);
        arguments[2] = arguments[1];
        let location = VerifyLocation::Instruction(call.id);
        let errors =
            verify_program(&program).expect_err("a duplicated exclusive loan must be rejected");
        assert!(
            errors.errors.iter().any(|e| e.location == location
                && matches!(
                    e.kind,
                    VerifyErrorKind::OperationContract {
                        reason: "Inout arguments must be disjoint from other loans"
                    }
                )),
            "{errors:?}"
        );
    }
}

#[test]
fn range_call_rejects_an_exclusive_parent_with_a_live_shared_child() {
    let (program, caller) = forwarding_range_call(SsaTypeKind::Boolean, true);
    let function = program
        .module(caller.module())
        .unwrap()
        .function(caller)
        .unwrap();
    let call = &function.instructions[1];
    assert!(matches!(call.operation, Operation::RangeCall { .. }));
    let errors = verify_program(&program).expect_err("an active shared child protects its parent");
    assert!(
        errors
            .errors
            .iter()
            .any(|e| e.location == VerifyLocation::Instruction(call.id)
                && matches!(e.kind, VerifyErrorKind::LoanDependencyActive { .. })),
        "{errors:?}"
    );
}
