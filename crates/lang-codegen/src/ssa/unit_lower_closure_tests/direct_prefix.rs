//! Success assertions for direct-call prefixes cancelled by source control flow.
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
            assert!(
                values.is_empty(),
                "the original loop naturally returns Unit"
            );
        }
        _ => panic!("the original return/break source retains its exact transfer kind"),
    }
    crate::llvm::render_verified_program(program)
        .expect("original migrated source emits verified LLVM");
}

pub(super) fn assert_cancelled_nested_break(
    program: &Program,
    entry: FunctionId,
    sources: &SourceMap,
) {
    verify_program(program).expect("nested break source verifies");
    let function = program
        .module(entry.module())
        .unwrap()
        .function(entry)
        .unwrap();
    let endings: Vec<_> = function
        .instructions
        .iter()
        .filter_map(
            |instruction| match (&instruction.operation, &instruction.origin) {
                (Operation::BorrowEnd { loan }, Origin::Source(span))
                    if sources.slice(*span) == Ok("break") =>
                {
                    Some((instruction, *loan))
                }
                _ => None,
            },
        )
        .collect();
    let [(ending, loan)] = endings.as_slice() else {
        panic!(
            "expected one borrow end on the nested break edge: {}",
            super::render_program(program)
        );
    };
    let begins: Vec<_> = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::BorrowBegin { .. }))
        .collect();
    let [begin] = begins.as_slice() else {
        panic!("the nested break creates only the scalar prefix loan");
    };
    // The verifier checks the provenance of the loan transported through
    // block parameters before the actual break cleanup.
    let ended = EntityId::Loan(*loan);
    assert!(
        begin.results == [ended]
            || function
                .blocks
                .iter()
                .any(|block| { block.parameters.contains(&ended) })
    );
    let block = function.block(ending.block).unwrap();
    assert_eq!(block.instructions.last(), Some(&ending.id));
    assert!(!function.instructions.iter().any(|instruction| {
        instruction.block == ending.block && is_direct_call(&instruction.operation)
    }));
    crate::llvm::render_verified_program(program).expect("nested break emits verified LLVM");
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
        form:
            FunctionForm::Explicit {
                body: FunctionBody::Block(body),
                ..
            },
        ..
    } = entry.payload()
    else {
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
    let Item::Variable { initializer, .. } = ast.items().get(*declaration).unwrap().payload()
    else {
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

pub(in crate::ssa) mod borrow_storage {
//! 通用 Borrow storage 参数取代旧 String-only closure 布局限制。
use super::super::*;
use crate::ssa::verify::verify_program;

#[test]
fn move_only_borrow_parameter_uses_shared_storage_without_payload_owner() {
    // Preserve the exact former unsupported fixture as a positive ABI contract.
    let source = "package test\n\
        class Host {}\n\
        fun entry(): Unit {\n\
            val action: move (borrow Host) -> Unit = move { item -> }\n\
        }";
    let mut sources = SourceMap::new();
    let (source_id, file) = parsed(&mut sources, "test/move-only-borrow-parameter.ko", source);
    let inputs = [SourceUnitInput::new(
        "root",
        "test/move-only-borrow-parameter.ko",
        source_id,
        &file,
    )];
    let (environment, types) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &environment, &types);
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &types,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    )
    .unwrap();
    verify_program(&program).unwrap();
    let module = &program.modules[0];
    let thunk = module
        .functions
        .iter()
        .find(|f| f.name.contains("lambda"))
        .unwrap();
    let [EntityId::Loan(parameter)] = thunk
        .block(thunk.entry_block().unwrap())
        .unwrap()
        .parameters
        .as_slice()
    else {
        panic!("one shared storage parameter")
    };
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target,
    } = thunk.entity(EntityId::Loan(*parameter)).unwrap().ty
    else {
        panic!("borrow ABI")
    };
    assert!(matches!(
        module.type_kind(target),
        Some(SsaTypeKind::HeapOwner { .. })
    ));
    assert!(!thunk.instructions.iter().any(|i| matches!(
        i.operation,
        Operation::Drop { .. } | Operation::Copy { .. } | Operation::SharedRetain { .. }
    )));
    // Entry Borrow is a call-scoped parameter; Return ends this callee view.
    // The caller remains responsible for its actual source loan continuation.
    assert_eq!(
        operation_count(thunk, |i| matches!(i, Operation::BorrowEnd { .. })),
        0
    );
    let llvm = crate::llvm::render_verified_program(&program).unwrap();
    let header = llvm
        .lines()
        .find(|line| line.starts_with("define internal void") && line.contains("lambda"))
        .unwrap();
    assert!(header.contains("ptr %l"));
}
}

pub(in crate::ssa) mod shared_capture {
//! A nonescaping borrowed closure now consumes the published shared-capture contract.
use super::super::*;
use lang_frontend::{
    ownership_checking::{ClosureCaptureEffect, ClosureCaptureMode as FrontendCaptureMode},
    parser::Expression,
    type_checking::UnitExpressionId,
};

#[test]
fn borrowed_closure_keeps_its_source_owner_until_the_closure_drop() {
    let mut sources = SourceMap::new();
    let (source, parsed) = parsed(
        &mut sources,
        "test/borrowed.ko",
        "package test\n\
         fun inspect(message: String): Unit {}\n\
         fun entry(): Unit {\n\
             val message = \"borrowed\"\n\
             val action: () -> Unit = { -> val read = inspect(message) }\n\
             val invoked = action()\n\
         }",
    );
    let inputs = [SourceUnitInput::new(
        "root",
        "test/borrowed.ko",
        source,
        &parsed,
    )];
    let (name_environment, type_environment) = standard_environments();
    let (names, typed, owned) = analyze(&sources, &inputs, &name_environment, &type_environment);
    let lambda = parsed
        .ast()
        .expressions()
        .iter()
        .find(|(_, node)| matches!(node.payload(), Expression::Lambda { .. }))
        .expect("real source lambda")
        .0;
    let lambda = UnitExpressionId::new(names.names().source_units()[0].source_unit(), lambda);
    let captures = owned.ownership().captures_of(lambda).collect::<Vec<_>>();
    assert_eq!(captures.len(), 1);
    assert_eq!(captures[0].mode(), FrontendCaptureMode::Shared);
    assert_eq!(captures[0].effect(), ClosureCaptureEffect::Borrow);
    let arena = typed.types().types().len();
    let (program, _) = lower_scalar_unit_with_entry(
        &sources,
        &inputs,
        &names,
        &type_environment,
        &typed,
        &owned,
        declaration(&names, "test", "entry"),
    )
    .expect("guide permits a borrowed closure's synchronous call in its defining callable");
    let module = &program.modules[0];
    let entry = function(module, "test.entry");
    let owner = entry
        .instructions
        .iter()
        .find(|instruction| matches!(instruction.operation, Operation::StringLiteral { .. }))
        .expect("captured source owner")
        .results[0];
    let closure = entry
        .instructions
        .iter()
        .find(|instruction| matches!(instruction.operation, Operation::ClosureConstruct { .. }))
        .expect("shared capture environment")
        .results[0];
    let drop_index = |entity| {
        entry
            .instructions
            .iter()
            .position(|instruction| {
                matches!(instruction.operation, Operation::Drop { owner }
                if EntityId::Value(owner) == entity)
            })
            .expect("one real owner drop")
    };
    assert_eq!(operation_count(entry, is_callable_invoke), 1);
    assert_eq!(operation_count(entry, is_drop), 2);
    assert!(drop_index(closure) < drop_index(owner));
    let thunk = module
        .functions
        .iter()
        .find(|function| function.name.contains(".thunk"))
        .expect("borrowed closure thunk");
    assert!(thunk.instructions.iter().any(|instruction| matches!(
        instruction.operation,
        Operation::SharedReferenceFollow { .. }
    )));
    crate::llvm::render_verified_program(&program)
        .expect("shared capture lifetime reaches verified LLVM without consuming the source");
    assert_eq!(typed.types().types().len(), arena);
}
}
