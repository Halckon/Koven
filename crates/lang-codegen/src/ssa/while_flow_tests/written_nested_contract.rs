//! Query-only identities for the four ordinary written same-root nested sources.
use std::collections::BTreeSet;

use crate::ssa::model::{
    Block, BlockId, Definition, Edge, EntityId, EntityType, Function, InstructionId, LoanId,
    LoanKind, Operation, Origin, Program, SsaTypeKind, TerminatorKind, ValueId,
};

pub(super) fn assert_reborn_proof(program: &Program, source: &str, unit: bool) {
    let offset = if unit { "package p\n".len() } else { 0 };
    let starts = source
        .match_indices("while (")
        .map(|(start, _)| start)
        .collect::<Vec<_>>();
    assert_eq!(starts.len(), 2, "only the two-while written fixtures");
    let outer_end = source.rfind("\nreturn total").unwrap();
    let inner_end = source.find("\nreturn total\n}").unwrap();
    let module = only(program.modules.iter());
    let function = only(module.functions.iter().filter(|function| {
        function.blocks.iter().any(|block| {
            matches!(
                block.terminator.as_ref().map(|term| &term.kind),
                Some(TerminatorKind::NullableBranch { .. })
            )
        })
    }));
    let births = function
        .blocks
        .iter()
        .filter(|block| {
            matches!(
                block.terminator.as_ref().map(|term| &term.kind),
                Some(TerminatorKind::NullableBranch { .. })
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        births.len(),
        2,
        "the outer proof is retired and the inner guard births anew"
    );
    // While terminators carry the whole statement span, including grouped guards.
    let header = |start, end| {
        only(births.iter().copied().filter(|block| {
            matches!(block.terminator.as_ref().unwrap().origin, Origin::Source(span)
            if span.start() == start + offset && span.end() == end + offset)
        }))
    };
    let outer = header(starts[0], outer_end);
    let inner = header(starts[1], inner_end);
    assert_ne!(outer.id, inner.id);
    let (outer_owner, outer_pair) = birth_pair(function, outer);
    let (inner_owner, inner_pair) = birth_pair(function, inner);
    let TerminatorKind::NullableBranch {
        when_non_null: outer_true,
        ..
    } = &outer.terminator.as_ref().unwrap().kind
    else {
        unreachable!()
    };
    let TerminatorKind::NullableBranch {
        when_non_null: inner_true,
        when_null: inner_false,
        ..
    } = &inner.terminator.as_ref().unwrap().kind
    else {
        unreachable!()
    };
    assert_ne!(outer_owner, inner_owner);
    assert_ne!(outer_pair.0, inner_pair.0);
    assert_ne!(
        outer_pair.1, inner_pair.1,
        "the inner view is a distinct birth"
    );
    let old_pairs = trace_pairs(function, outer_pair);
    let new_pairs = trace_pairs(function, inner_pair);
    assert!(
        old_pairs
            .iter()
            .all(|(_, old)| new_pairs.iter().all(|(_, new)| old != new))
    );
    let EntityType::Value(nullable) = function.entity(EntityId::Value(outer_owner)).unwrap().ty
    else {
        panic!("owned nullable storage")
    };
    let Some(SsaTypeKind::NullableHandle { inner: payload }) = module.type_kind(nullable) else {
        panic!("nullable storage type")
    };
    assert!(matches!(
        module.type_kind(*payload),
        Some(SsaTypeKind::HeapOwner { .. })
    ));
    for &(owner, loan) in old_pairs.iter().chain(&new_pairs) {
        assert_eq!(
            function.entity(EntityId::Value(owner)).unwrap().ty,
            EntityType::Value(nullable)
        );
        assert_eq!(
            function.entity(EntityId::Loan(loan)).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: *payload
            }
        );
    }
    assert_eq!(
        function.entity(EntityId::Value(inner_owner)).unwrap().ty,
        EntityType::Value(nullable)
    );

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
    assert_eq!(reads.len(), 2);
    let read_at = |start| {
        only(reads.iter().copied().filter(|instruction| {
        matches!(instruction.origin, Origin::Source(span)
            if span.start() == start + offset && span.end() == start + offset + "current.value".len())
    }))
    };
    let outer_read = read_at(source.find("current.value").unwrap());
    let inner_read = read_at(source.rfind("current.value").unwrap());
    assert!(
        matches!(outer_read.operation, Operation::HeapFieldRead { receiver, .. }
        if old_pairs.iter().any(|(_, loan)| *loan == receiver))
    );
    assert!(
        matches!(inner_read.operation, Operation::HeapFieldRead { receiver, .. }
        if new_pairs.iter().any(|(_, loan)| *loan == receiver))
    );
    assert_eq!(inner_read.block, inner_true.target);
    let old_end = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::BorrowEnd { loan }
            if old_pairs.iter().any(|(_, old)| *old == loan))
    }));

    // Follow the actual outer true path to the inner header, carrying the owner
    // column even after its old proof ends. No type-only correspondence suffices.
    let mut path = Vec::new();
    let mut current = outer_true.target;
    let mut current_owner = outer_pair.0;
    while current != inner.id {
        assert!(!path.contains(&current), "acyclic inner-entry path");
        path.push(current);
        let block = function.block(current).unwrap();
        let TerminatorKind::Branch(edge) = &block.terminator.as_ref().unwrap().kind else {
            panic!("ordinary outer-body path enters the inner header")
        };
        let column = only(
            edge.arguments
                .iter()
                .enumerate()
                .filter_map(|(index, entity)| {
                    (*entity == EntityId::Value(current_owner)).then_some(index)
                }),
        );
        let EntityId::Value(next_owner) = function.block(edge.target).unwrap().parameters[column]
        else {
            panic!("same-root owner column")
        };
        current_owner = next_owner;
        current = edge.target;
    }
    assert_eq!(
        current_owner, inner_owner,
        "the new test uses the actual old root storage"
    );
    let read_position = path
        .iter()
        .position(|block| *block == outer_read.block)
        .unwrap();
    let end_position = path
        .iter()
        .position(|block| *block == old_end.block)
        .unwrap();
    assert!(read_position <= end_position);
    if outer_read.block == old_end.block {
        assert_before(function, outer_read.block, outer_read.id, old_end.id);
    }
    for block in &path[end_position..] {
        for edge in edges(function.block(*block).unwrap()) {
            assert_no_proof(&edge.arguments, &old_pairs);
        }
    }
    assert!(!path.is_empty());
    let entry_predecessor = *path.last().unwrap();
    assert_ne!(
        old_end.block, inner.id,
        "the old proof ends before entering the inner header"
    );

    // Absence is checked in the real CFG as well as in the traced edge columns:
    // no old loan is used or resurrected anywhere reachable from the inner guard.
    let mut reachable = BTreeSet::from([inner.id]);
    loop {
        let successors = reachable
            .iter()
            .flat_map(|id| {
                edges(function.block(*id).unwrap())
                    .into_iter()
                    .map(|edge| edge.target)
            })
            .collect::<Vec<_>>();
        let before = reachable.len();
        reachable.extend(successors);
        if reachable.len() == before {
            break;
        }
    }
    for id in reachable {
        let block = function.block(id).unwrap();
        assert_no_proof(&block.parameters, &old_pairs);
        assert_no_proof(
            &block.terminator.as_ref().unwrap().kind.entities(),
            &old_pairs,
        );
        for instruction in &block.instructions {
            let instruction = function.instruction(*instruction).unwrap();
            assert_no_proof(&instruction.operation.entities(), &old_pairs);
            assert_no_proof(&instruction.results, &old_pairs);
        }
    }
    for pairs in [&old_pairs, &new_pairs] {
        assert_no_proof(&inner.parameters, pairs);
        assert_no_proof(&inner_false.arguments, pairs);
        assert_no_proof(
            &function.block(inner_false.target).unwrap().parameters,
            pairs,
        );
        assert_no_proof(&inner_true.arguments, pairs);
    }

    // Checked addition can rebind both members before assignment. Find the ended
    // pair by its actual loan, then require Drop of precisely its paired owner.
    let new_end = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::BorrowEnd { loan }
            if new_pairs.iter().any(|(_, new)| *new == loan))
    }));
    let Operation::BorrowEnd { loan: ended_loan } = new_end.operation else {
        unreachable!()
    };
    let ended_pair = only(
        new_pairs
            .iter()
            .copied()
            .filter(|(_, loan)| *loan == ended_loan),
    );
    let assignment_block = function.block(new_end.block).unwrap();
    assert!(
        assignment_block
            .parameters
            .contains(&EntityId::Value(ended_pair.0))
    );
    assert!(
        assignment_block
            .parameters
            .contains(&EntityId::Loan(ended_pair.1))
    );
    let drop = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::Drop { owner } if owner == ended_pair.0)
    }));
    assert_eq!(drop.block, new_end.block);
    assert_before(function, new_end.block, new_end.id, drop.id);
    let assignment = source.find("current = null").unwrap();
    let null_start = assignment + "current = ".len() + offset;
    let null = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::NullableNull { nullable: actual } if actual == nullable)
            && matches!(instruction.origin, Origin::Source(span)
                if span.start() == null_start && span.end() == null_start + "null".len())
    }));
    // The last field read retires the old owner before checked arithmetic; the
    // null assignment executes only on that arithmetic's real success edge.
    assert_eq!(inner_read.block, new_end.block);
    assert_before(function, new_end.block, inner_read.id, new_end.id);
    let EntityId::Value(read_value) = only(inner_read.results.iter().copied()) else {
        panic!("field read value")
    };
    let checked = only(function.instructions.iter().filter(|instruction| {
        instruction.block == new_end.block
            && matches!(instruction.operation,
                Operation::CheckedArithmetic { operator: crate::ssa::model::CheckedArithmeticOperator::Add, right, .. }
                if right == read_value)
    }));
    assert_before(function, new_end.block, drop.id, checked.id);
    let [EntityId::Value(_), EntityId::Value(failed)] = checked.results.as_slice() else {
        panic!("checked arithmetic value and failure flag")
    };
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &assignment_block.terminator.as_ref().unwrap().kind
    else {
        panic!("actual checked arithmetic branch")
    };
    assert_eq!(condition, failed);
    assert_eq!(when_false.target, null.block);
    assert!(matches!(
        function
            .block(when_true.target)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .kind,
        TerminatorKind::Abort
    ));
    let incoming_success = function
        .blocks
        .iter()
        .flat_map(edges)
        .filter(|edge| edge.target == null.block)
        .collect::<Vec<_>>();
    assert_eq!(incoming_success.len(), 1);
    assert_eq!(incoming_success[0], when_false);
    let replacement = only(null.results.iter().copied());
    assert!(matches!(replacement, EntityId::Value(_)));
    assert_ne!(replacement, EntityId::Value(ended_pair.0));
    let Definition::BlockParameter {
        block,
        index: owner_column,
    } = function
        .entity(EntityId::Value(inner_owner))
        .unwrap()
        .definition
    else {
        panic!("the inner guard tests its actual header owner parameter")
    };
    assert_eq!(block, inner.id);
    let incoming = function
        .blocks
        .iter()
        .flat_map(|block| {
            edges(block)
                .into_iter()
                .filter(|edge| edge.target == inner.id)
                .map(move |edge| (block, edge))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        incoming.len(),
        2,
        "one initial edge and one normal/continue backedge"
    );
    let (backedge_block, backedge) = only(
        incoming
            .into_iter()
            .filter(|(block, _)| block.id != entry_predecessor),
    );
    assert_eq!(backedge_block.id, null.block);
    assert!(matches!(
        backedge_block.terminator.as_ref().unwrap().kind,
        TerminatorKind::Branch(_)
    ));
    assert_eq!(
        backedge.arguments[owner_column], replacement,
        "the actual NullableNull result replaces the consumed owner on recurrence"
    );
    for pairs in [&old_pairs, &new_pairs] {
        assert_no_proof(&backedge.arguments, pairs);
    }
}

fn birth_pair(function: &Function, header: &Block) -> (ValueId, (ValueId, LoanId)) {
    let TerminatorKind::NullableBranch {
        owner,
        when_null,
        when_non_null,
        view,
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("real nullable birth")
    };
    let body = function.block(when_non_null.target).unwrap();
    let column = only(
        when_non_null
            .arguments
            .iter()
            .enumerate()
            .filter_map(|(index, entity)| (*entity == EntityId::Value(*owner)).then_some(index)),
    );
    let EntityId::Value(body_owner) = body.parameters[column] else {
        panic!("true-edge owner column")
    };
    assert_eq!(body.parameters.len(), when_non_null.arguments.len() + 1);
    assert_eq!(body.parameters.last(), Some(&EntityId::Loan(*view)));
    assert!(!when_non_null.arguments.contains(&EntityId::Loan(*view)));
    assert!(!when_null.arguments.contains(&EntityId::Loan(*view)));
    assert_eq!(
        function.block(when_null.target).unwrap().parameters.len(),
        when_null.arguments.len()
    );
    (*owner, (body_owner, *view))
}

fn trace_pairs(function: &Function, initial: (ValueId, LoanId)) -> BTreeSet<(ValueId, LoanId)> {
    let mut pairs = BTreeSet::from([initial]);
    loop {
        let mut added = Vec::new();
        for block in &function.blocks {
            for edge in edges(block) {
                let target = function.block(edge.target).unwrap();
                for &(owner, loan) in &pairs {
                    for (loan_column, argument) in edge.arguments.iter().enumerate() {
                        if *argument != EntityId::Loan(loan) {
                            continue;
                        }
                        let owner_column = only(edge.arguments.iter().enumerate().filter_map(
                            |(index, entity)| (*entity == EntityId::Value(owner)).then_some(index),
                        ));
                        let EntityId::Value(next_owner) = target.parameters[owner_column] else {
                            panic!("carried owner column")
                        };
                        let EntityId::Loan(next_loan) = target.parameters[loan_column] else {
                            panic!("carried proof column")
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
    pairs
}

fn assert_no_proof(entities: &[EntityId], pairs: &BTreeSet<(ValueId, LoanId)>) {
    assert!(
        pairs
            .iter()
            .all(|(_, loan)| !entities.contains(&EntityId::Loan(*loan))),
        "retired proof must not be carried or resurrected"
    );
}

fn assert_before(function: &Function, block: BlockId, first: InstructionId, second: InstructionId) {
    let instructions = &function.block(block).unwrap().instructions;
    let position = |id| {
        instructions
            .iter()
            .position(|candidate| *candidate == id)
            .unwrap()
    };
    assert!(position(first) < position(second));
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
