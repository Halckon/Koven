//! Query only the SSA produced by the two ordinary read-only nested sources.
use std::collections::BTreeSet;

use crate::ssa::model::{
    Block, Edge, EntityId, EntityType, LoanKind, Operation, Origin, Program, SsaTypeKind,
    TerminatorKind,
};

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
    // There is exactly one birth in the whole function: the outer guard.
    let outer = only(function.blocks.iter().filter(|block| {
        matches!(
            block.terminator.as_ref().map(|term| &term.kind),
            Some(TerminatorKind::NullableBranch { .. })
        )
    }));
    let TerminatorKind::NullableBranch {
        owner,
        when_non_null,
        view,
        ..
    } = &outer.terminator.as_ref().unwrap().kind
    else {
        unreachable!()
    };
    let outer_body = function.block(when_non_null.target).unwrap();
    let column = only(
        when_non_null
            .arguments
            .iter()
            .enumerate()
            .filter_map(|(index, entity)| (*entity == EntityId::Value(*owner)).then_some(index)),
    );
    let EntityId::Value(body_owner) = outer_body.parameters[column] else {
        panic!("true-edge owner")
    };
    assert_eq!(outer_body.parameters.last(), Some(&EntityId::Loan(*view)));
    let EntityType::Value(nullable) = function.entity(EntityId::Value(body_owner)).unwrap().ty
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

    // Follow the actual owner and loan columns together. A matching type alone
    // does not count as inheritance, and no metadata or synthetic fact is injected.
    let mut pairs = BTreeSet::from([(body_owner, *view)]);
    loop {
        let mut added = Vec::new();
        for block in &function.blocks {
            for edge in edges(block) {
                let target = function.block(edge.target).unwrap();
                for &(paired_owner, loan) in &pairs {
                    for (loan_column, argument) in edge.arguments.iter().enumerate() {
                        if *argument != EntityId::Loan(loan) {
                            continue;
                        }
                        let owner_column = only(edge.arguments.iter().enumerate().filter_map(
                            |(index, entity)| {
                                (*entity == EntityId::Value(paired_owner)).then_some(index)
                            },
                        ));
                        let EntityId::Value(next_owner) = target.parameters[owner_column] else {
                            panic!("owner column")
                        };
                        let EntityId::Loan(next_loan) = target.parameters[loan_column] else {
                            panic!("loan column")
                        };
                        added.push((next_owner, next_loan));
                    }
                }
            }
        }
        let before = pairs.len();
        pairs.extend(added);
        if pairs.len() == before {
            break;
        }
    }
    for &(paired_owner, loan) in &pairs {
        assert_eq!(
            function.entity(EntityId::Value(paired_owner)).unwrap().ty,
            EntityType::Value(nullable)
        );
        assert_eq!(
            function.entity(EntityId::Loan(loan)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: *inner
            }
        );
    }
    let inner_header = only(function.blocks.iter().filter(|block| {
        matches!(
            block.terminator.as_ref().map(|term| &term.kind),
            Some(TerminatorKind::Conditional { .. })
        )
    }));
    let header_pair = only(pairs.iter().copied().filter(|(owner, loan)| {
        inner_header.parameters.contains(&EntityId::Value(*owner))
            && inner_header.parameters.contains(&EntityId::Loan(*loan))
    }));
    for edge in edges(inner_header) {
        assert!(edge.arguments.contains(&EntityId::Value(header_pair.0)));
        assert!(edge.arguments.contains(&EntityId::Loan(header_pair.1)));
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
    for read in &reads {
        let Operation::HeapFieldRead { receiver, .. } = read.operation else {
            unreachable!()
        };
        assert!(pairs.iter().any(|(_, loan)| *loan == receiver));
    }
    let offset = if unit { "package p\n".len() } else { 0 };
    let final_read_start = source.rfind("current.value").unwrap() + offset;
    let final_read = only(reads.into_iter().filter(|read| {
        matches!(read.origin, Origin::Source(span) if span.start() == final_read_start && span.end() == final_read_start + "current.value".len())
    }));
    let Operation::HeapFieldRead { receiver, .. } = final_read.operation else {
        unreachable!()
    };
    assert_ne!(
        receiver, *view,
        "the outer read uses the real inner-exit rebinding"
    );
    let end = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::BorrowEnd { loan } if pairs.iter().any(|(_, inherited)| *inherited == loan))
    }));
    assert!(matches!(end.operation, Operation::BorrowEnd { loan } if loan == receiver));
    assert_eq!(end.block, final_read.block);
    assert_ne!(end.block, inner_header.id);
    for edge in edges(inner_header) {
        assert_ne!(end.block, edge.target);
    }
    let block = function.block(end.block).unwrap();
    let position = |id| {
        block
            .instructions
            .iter()
            .position(|candidate| *candidate == id)
            .unwrap()
    };
    assert!(position(final_read.id) < position(end.id));
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
