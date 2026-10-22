//! Success assertions for the two unchanged direct-call sources in the original whole.
//! Only inspect the real returned Program; no synthetic facts, IDs, or SSA are constructed.

use lang_frontend::{
    parser::{Expression, FunctionBody, FunctionForm, Item, NameMarker, ParsedFile, Statement},
    source::SourceMap,
};

use super::{dropped, is_direct_call, operation_count};
use crate::ssa::{
    model::{
        EntityId, Function, FunctionId, Instruction, Operation, Origin, Program, ScalarConstant,
        TerminatorKind,
    },
    verify::verify_program,
};

pub(super) fn assert_cancelled_scalar_call(
    program: &Program,
    entry: FunctionId,
    returning: bool,
    sources: &SourceMap,
    parsed: &ParsedFile,
) {
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
    if returning {
        assert_eq!(
            term.origin, ending.origin,
            "cleanup immediately precedes the exact source transfer"
        );
    } else {
        assert_single_break_tail(sources, parsed, function, ending);
    }
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
        TerminatorKind::Return { values } if !returning => {
            assert!(values.is_empty(), "the original loop naturally returns Unit");
        }
        _ => panic!("the original return/break source retains its exact transfer kind"),
    }
    crate::llvm::render_verified_program(program)
        .expect("original migrated source emits verified LLVM");
}

// The original single-exit loop reuses its break block, then returns Unit naturally.
fn assert_single_break_tail(
    sources: &SourceMap,
    parsed: &ParsedFile,
    function: &Function,
    ending: &Instruction,
) {
    let ast = parsed.ast();
    let entries = ast
        .items()
        .iter()
        .filter(|(_, node)| {
            matches!(node.payload(), Item::Function { name: NameMarker::Present(name), .. }
                if sources.slice(*name) == Ok("entry"))
        })
        .collect::<Vec<_>>();
    let [(_, entry)] = entries.as_slice() else {
        panic!("the original source has one actual entry declaration")
    };
    let Item::Function {
        form: FunctionForm::Explicit { body: FunctionBody::Block(body), .. },
        ..
    } = entry.payload() else {
        panic!("the original entry has its explicit Unit block body")
    };
    let Statement::Block { elements } = ast.statements().get(*body).unwrap().payload() else {
        panic!("the actual entry body is a block")
    };
    let [_, loop_id] = elements.as_slice() else {
        panic!("the original entry has only its scalar declaration and tail loop")
    };
    let loop_node = ast.statements().get(*loop_id).unwrap();
    let Statement::Loop { body, .. } = loop_node.payload() else {
        panic!("the actual tail statement is an unconditional loop")
    };
    let Statement::Block { elements } = ast.statements().get(*body).unwrap().payload() else {
        panic!("the actual loop body is a block")
    };
    let [local] = elements.as_slice() else {
        panic!("the original loop contains only the cancelling call declaration")
    };
    let Statement::LocalVariable { declaration } = ast.statements().get(*local).unwrap().payload()
    else {
        panic!("the original call is the local initializer")
    };
    let Item::Variable { initializer, .. } = ast.items().get(*declaration).unwrap().payload() else {
        panic!("the actual local declaration has its initializer")
    };
    let Expression::Call { arguments, .. } = ast.expressions().get(*initializer).unwrap().payload()
    else {
        panic!("the actual initializer is the direct call")
    };
    let [_, exit] = arguments.as_slice() else {
        panic!("the original direct call has exactly two arguments")
    };
    let exit = ast.expressions().get(exit.value).unwrap();
    assert!(matches!(exit.payload(), Expression::Break { .. }));
    assert_eq!(ending.origin, Origin::Source(exit.span()));
    assert_eq!(entry.span().source_id(), parsed.source_id());
    assert_eq!(loop_node.span().source_id(), parsed.source_id());
    assert_eq!(exit.span().source_id(), parsed.source_id());
    assert_eq!(function.origin, Origin::Source(entry.span()));
    assert!(function.return_types.is_empty());
    let [entry_block, loop_block] = function.blocks.as_slice() else {
        panic!("the single-exit source has only entry and the reused loop block")
    };
    assert_eq!(function.entry_block(), Some(entry_block.id));
    assert_eq!(entry_block.origin, Origin::Source(entry.span()));
    assert_eq!(loop_block.origin, Origin::Source(loop_node.span()));
    assert_eq!(ending.block, loop_block.id);
    let transfer = entry_block.terminator.as_ref().unwrap();
    assert_eq!(transfer.origin, Origin::Source(loop_node.span()));
    let TerminatorKind::Branch(edge) = &transfer.kind else {
        panic!("entry transfers directly to the actual loop header")
    };
    assert_eq!(edge.target, loop_block.id);
    assert_ne!(edge.target, entry_block.id);
    let tail = loop_block.terminator.as_ref().unwrap();
    assert_eq!(tail.origin, Origin::Source(entry.span()));
    let TerminatorKind::Return { values } = &tail.kind else {
        panic!("the reused single break block becomes the natural Unit return")
    };
    assert!(values.is_empty());
}
