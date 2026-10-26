//! Query only the SSA produced by the two ordinary read-only nested sources.
use std::collections::BTreeSet;

use crate::ssa::model::{
    Block, Edge, EntityId, EntityType, LoanId, LoanKind, Operation, Origin, Program, SsaTypeKind,
    TerminatorKind,
};

/// SPEC-0356: both layouts lower `entry` through the refinement layer. The inner `while`
/// re-tests the same root with its own `NullableBranch`; the outer proof view is carried
/// across the inner loop on every exit and backs the final read.
pub(super) fn assert_inherited_proof(program: &Program, source: &str, unit: bool) {
    let module = only(program.modules.iter());
    let function = only(module.functions.iter().filter(|function| {
        function.blocks.iter().any(|block| {
            matches!(
                block.terminator.as_ref().map(|term| &term.kind),
                Some(TerminatorKind::NullableBranch { .. })
            )
        })
    }));
    let mut branches = function
        .blocks
        .iter()
        .filter_map(|block| {
            let terminator = block.terminator.as_ref()?;
            let TerminatorKind::NullableBranch { owner, view, .. } = &terminator.kind else {
                return None;
            };
            let Origin::Source(span) = terminator.origin else {
                return None;
            };
            Some((span.start(), *owner, *view))
        })
        .collect::<Vec<_>>();
    branches.sort_unstable();
    let [(_, outer_owner, outer_view), (_, inner_owner, inner_view)] = branches[..] else {
        panic!("the outer guard and the re-tested inner condition: {branches:?}")
    };
    let EntityType::Value(nullable) = function.entity(EntityId::Value(outer_owner)).unwrap().ty
    else {
        panic!("nullable owner")
    };
    let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(nullable) else {
        panic!("nullable storage")
    };
    assert!(matches!(
        module.type_kind(*inner),
        Some(SsaTypeKind::HeapOwner { .. })
    ));
    assert_eq!(
        function.entity(EntityId::Value(inner_owner)).unwrap().ty,
        EntityType::Value(nullable)
    );
    let outer = traced(function, outer_view);
    let inner_loans = traced(function, inner_view);
    assert!(
        outer.is_disjoint(&inner_loans),
        "the inner test mints a fresh view"
    );
    for loan in outer.iter().chain(&inner_loans) {
        assert_eq!(
            function.entity(EntityId::Loan(*loan)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: *inner
            }
        );
    }
    let reads = function
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(
                instruction.operation,
                Operation::HeapFieldRead { field: 0, .. }
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(reads.len(), 3);
    let offset = if unit { "package p\n".len() } else { 0 };
    let receiver_at = |start: usize| {
        let read = only(reads.iter().filter(
            |read| matches!(read.origin, Origin::Source(span) if span.start() == start + offset),
        ));
        let Operation::HeapFieldRead { receiver, .. } = read.operation else {
            unreachable!()
        };
        (receiver, read)
    };
    let first = source.find("current.value").unwrap();
    let middle = first + 1 + source[first + 1..].find("current.value").unwrap();
    let last = source.rfind("current.value").unwrap();
    assert!(outer.contains(&receiver_at(first).0));
    assert!(inner_loans.contains(&receiver_at(middle).0));
    let (final_receiver, final_read) = receiver_at(last);
    assert!(
        outer.contains(&final_receiver) && final_receiver != outer_view,
        "the final read uses the outer proof carried across the inner loop"
    );
    let ends = |loans: &BTreeSet<_>| {
        function
            .instructions
            .iter()
            .filter(|instruction| {
                matches!(instruction.operation, Operation::BorrowEnd { loan } if loans.contains(&loan))
            })
            .collect::<Vec<_>>()
    };
    assert!(!ends(&inner_loans).is_empty(), "the inner view ends");
    let outer_end = only(ends(&outer).into_iter());
    assert_eq!(outer_end.block, final_read.block);
    let block = function.block(final_read.block).unwrap();
    let position = |id| {
        block
            .instructions
            .iter()
            .position(|candidate| *candidate == id)
            .unwrap()
    };
    assert!(position(final_read.id) < position(outer_end.id));
}

/// The loan and every block parameter it is passed into.
fn traced(function: &crate::ssa::model::Function, view: LoanId) -> BTreeSet<LoanId> {
    let mut loans = BTreeSet::from([view]);
    loop {
        let before = loans.len();
        for block in &function.blocks {
            for edge in edges(block) {
                let target = function.block(edge.target).unwrap();
                for (column, argument) in edge.arguments.iter().enumerate() {
                    if let EntityId::Loan(loan) = argument
                        && loans.contains(loan)
                        && let EntityId::Loan(next) = target.parameters[column]
                    {
                        loans.insert(next);
                    }
                }
            }
        }
        if loans.len() == before {
            return loans;
        }
    }
}

fn edges(block: &Block) -> Vec<&Edge> {
    match &block.terminator.as_ref().unwrap().kind {
        TerminatorKind::Branch(edge) => vec![edge],
        TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } => vec![when_true, when_false],
        TerminatorKind::NullableBranch {
            when_null,
            when_non_null,
            ..
        } => vec![when_null, when_non_null],
        TerminatorKind::Return { .. } | TerminatorKind::Abort => Vec::new(),
        _ => panic!("ordinary owned nullable fixture has no borrowed return"),
    }
}

fn only<T>(mut values: impl Iterator<Item = T>) -> T {
    let value = values.next().expect("one actual identity");
    assert!(values.next().is_none(), "identity must be unique");
    value
}
