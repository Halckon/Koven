//! Success assertions for the two unchanged direct-call sources in the original whole.
//! Only inspect the real returned Program; no synthetic facts, IDs, or SSA are constructed.

use super::{dropped, is_direct_call, operation_count};
use crate::ssa::{
    model::{EntityId, FunctionId, Operation, Program, ScalarConstant, TerminatorKind},
    verify::verify_program,
};

pub(super) fn assert_cancelled_scalar_call(program: &Program, entry: FunctionId, returning: bool) {
    verify_program(program).expect("original direct-argument source now verifies");
    let function = program
        .module(entry.module())
        .unwrap()
        .function(entry)
        .unwrap();
    assert_eq!(
        operation_count(function, is_direct_call),
        0,
        "the cancelled callee is never invoked"
    );
    assert!(
        dropped(function).is_empty(),
        "the Copyable prefix creates no owner obligation"
    );
    let endings = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::BorrowEnd { .. }))
        .collect::<Vec<_>>();
    assert_eq!(
        endings.len(),
        1,
        "the sole new prefix loan ends exactly once"
    );
    let ending = endings[0];
    let begins = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::BorrowBegin { .. }))
        .collect::<Vec<_>>();
    assert_eq!(begins.len(), 1, "one actual prefix loan was created");
    let Operation::BorrowEnd { loan } = ending.operation else {
        unreachable!("selected the actual BorrowEnd operation")
    };
    assert_eq!(begins[0].results, [EntityId::Loan(loan)]);
    let block = function.block(ending.block).unwrap();
    assert_eq!(block.instructions.last(), Some(&ending.id));
    let term = block.terminator.as_ref().unwrap();
    assert_eq!(
        term.origin, ending.origin,
        "cleanup immediately precedes the exact source transfer"
    );
    match &term.kind {
        TerminatorKind::Return { values } if returning => {
            let [returned] = values.as_slice() else {
                panic!("the original return source returns exactly one scalar")
            };
            let constants = function
                .instructions
                .iter()
                .filter(|instruction| {
                    instruction.results == [EntityId::Value(*returned)]
                        && instruction.operation == Operation::Constant(ScalarConstant::Integer(7))
                })
                .count();
            assert_eq!(
                constants, 1,
                "the original source returns 7 without calling select"
            );
        }
        TerminatorKind::Branch(_) if !returning => {}
        _ => panic!("the original return/break source retains its exact transfer kind"),
    }
    crate::llvm::render_verified_program(program)
        .expect("original migrated source emits verified LLVM");
}
