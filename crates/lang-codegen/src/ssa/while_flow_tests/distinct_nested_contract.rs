//! Query-only identities for the ordinary different-root nested sources.
use std::collections::BTreeSet;

use crate::ssa::model::{
    Block, BlockId, CheckedArithmeticOperator, Edge, EntityId, EntityType, Function, InstructionId,
    LoanId, LoanKind, Module, Operation, Origin, Program, SsaTypeId, SsaTypeKind, TerminatorKind,
    ValueId,
};

type Pair = (ValueId, LoanId);
type Pairs = BTreeSet<Pair>;

pub(super) fn assert_distinct_scopes(program: &Program, source: &str, unit: bool) {
    let offset = if unit { "package p\n".len() } else { 0 };
    let outer_start = source.find("while (").unwrap();
    let inner_start = source.rfind("while (").unwrap();
    assert_ne!(outer_start, inner_start);
    let module = only(program.modules.iter());
    let is_birth = |block: &Block| matches!(term(block), TerminatorKind::NullableBranch { .. });
    let function = only(
        module
            .functions
            .iter()
            .filter(|function| function.blocks.iter().any(is_birth)),
    );
    let births = function
        .blocks
        .iter()
        .filter(|block| is_birth(block))
        .collect::<Vec<_>>();
    assert_eq!(births.len(), 2, "two real proof births");
    // Grouped guards retain the complete while statement's source identity.
    let header = |start, end| {
        only(births.iter().copied().filter(|block| {
            matches!(block.terminator.as_ref().unwrap().origin, Origin::Source(span)
                if span.start() == start + offset && span.end() == end + offset)
        }))
    };
    let outer = header(outer_start, source.rfind("\n    return total").unwrap());
    let inner = header(
        inner_start,
        source
            .find("\n        return total + current.value")
            .unwrap(),
    );
    assert_ne!(outer.id, inner.id);
    let (outer_owner, outer_birth) = birth_pair(function, outer);
    let (inner_owner, inner_birth) = birth_pair(function, inner);
    let outer_pairs = trace_pairs(function, outer_birth);
    let inner_pairs = trace_pairs(function, inner_birth);
    assert_ne!(outer_owner, inner_owner);
    assert!(
        outer_pairs.iter().all(|(owner, loan)| {
            inner_pairs
                .iter()
                .all(|(other_owner, other_loan)| owner != other_owner && loan != other_loan)
        }),
        "different roots never share an actual owner or loan identity"
    );
    // Resolve each scope's nullable and payload independently of the other.
    let (_, outer_payload) = assert_scope_types(module, function, outer_owner, &outer_pairs);
    let (inner_nullable, inner_payload) =
        assert_scope_types(module, function, inner_owner, &inner_pairs);
    assert_eq!(
        outer_payload != inner_payload,
        source.contains("class Other(")
    );
    // edges orders nullable successors as null, then non-null.
    let outer_true = edges(outer)[1];
    let (inner_false, inner_true) = (edges(inner)[0], edges(inner)[1]);

    // This fixture's outer body branches directly into the inner header.
    let entry = function.block(outer_true.target).unwrap();
    let TerminatorKind::Branch(initial) = term(entry) else {
        panic!("actual inner entry branch")
    };
    assert_eq!(initial.target, inner.id);
    let outer_header_pair = carry_pair(function, initial, outer_birth, &outer_pairs);
    assert_ne!(outer_header_pair.0, inner_owner);
    for edge in [inner_true, inner_false] {
        carry_pair(function, edge, outer_header_pair, &outer_pairs);
        assert_no_proof(&edge.arguments, &inner_pairs);
    }
    assert_no_proof(&inner.parameters, &inner_pairs);
    assert_no_proof(
        &function.block(inner_false.target).unwrap().parameters,
        &inner_pairs,
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
    let read_at = |text: &str| {
        let start = source.find(text).unwrap() + offset;
        only(reads.iter().copied().filter(|instruction| {
            matches!(instruction.origin, Origin::Source(span)
                if span.start() == start && span.end() == start + text.len())
        }))
    };
    let inner_read = read_at("other.value");
    let outer_read = read_at("current.value");
    assert_eq!(inner_read.block, inner_true.target);
    let inner_end = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::BorrowEnd { loan }
            if inner_pairs.iter().any(|(_, inner)| *inner == loan))
    }));
    let Operation::BorrowEnd { loan: ended_loan } = inner_end.operation else {
        unreachable!()
    };
    let ended_pair = only(
        inner_pairs
            .iter()
            .copied()
            .filter(|(_, loan)| *loan == ended_loan),
    );
    assert_eq!(inner_read.block, inner_end.block);
    assert!(
        matches!(inner_read.operation, Operation::HeapFieldRead { receiver, .. }
        if receiver == ended_loan)
    );
    assert_before(function, inner_end.block, inner_read.id, inner_end.id);
    let inner_drop = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::Drop { owner } if owner == ended_pair.0)
    }));
    assert_eq!(inner_drop.block, inner_end.block);
    assert_before(function, inner_end.block, inner_end.id, inner_drop.id);

    // ASAP retirement precedes checked addition; select its actual failed flag.
    let EntityId::Value(read_value) = only(inner_read.results.iter().copied()) else {
        panic!("inner field read value")
    };
    let checked = only(function.instructions.iter().filter(|instruction| {
        instruction.block == inner_read.block
            && matches!(instruction.operation, Operation::CheckedArithmetic {
                operator: CheckedArithmeticOperator::Add, right, .. } if right == read_value)
    }));
    assert_before(function, checked.block, inner_drop.id, checked.id);
    let [EntityId::Value(_), EntityId::Value(failed)] = checked.results.as_slice() else {
        panic!("checked addition value and failure flag")
    };
    let checked_block = function.block(checked.block).unwrap();
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = term(checked_block)
    else {
        panic!("checked arithmetic failure branch")
    };
    assert_eq!(condition, failed);
    assert!(matches!(
        term(function.block(when_true.target).unwrap()),
        TerminatorKind::Abort
    ));
    // rfind selects the body assignment even in the explicit-null variant;
    // this checks compilation paths, without claiming runtime zero iterations.
    let start = source.rfind("other = null").unwrap() + "other = ".len() + offset;
    let null = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::NullableNull { nullable } if nullable == inner_nullable)
            && matches!(instruction.origin, Origin::Source(span)
                if span.start() == start && span.end() == start + "null".len())
    }));
    assert_eq!(when_false.target, null.block);
    let success_pair = carry_pair(
        function,
        when_false,
        pair_in(checked_block, &outer_pairs),
        &outer_pairs,
    );
    for edge in edges(checked_block) {
        assert_no_proof(&edge.arguments, &inner_pairs);
    }
    let replacement = only(null.results.iter().copied());
    assert!(matches!(replacement, EntityId::Value(_)));
    assert_ne!(replacement, EntityId::Value(ended_pair.0));
    let inner_owner_column = column(&inner.parameters, EntityId::Value(inner_owner));
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
    assert_eq!(incoming.len(), 2, "entry and normal/continue backedge");
    let (backedge_block, backedge) = only(
        incoming
            .into_iter()
            .filter(|(block, _)| block.id != entry.id),
    );
    assert_eq!(backedge_block.id, null.block);
    assert!(matches!(*term(backedge_block), TerminatorKind::Branch(_)));
    assert_eq!(
        backedge.arguments[inner_owner_column], replacement,
        "actual null replaces only the inner owner on recurrence"
    );
    assert_no_proof(&backedge.arguments, &inner_pairs);
    assert_no_proof(&backedge_block.parameters, &inner_pairs);
    assert_eq!(
        carry_pair(function, backedge, success_pair, &outer_pairs),
        outer_header_pair
    );

    // Require the actual false-exit parameter pair and its incoming columns.
    let exit = function.block(outer_read.block).unwrap();
    assert_eq!(exit.id, inner_false.target);
    let exit_pair = pair_in(exit, &outer_pairs);
    assert!(
        matches!(outer_read.operation, Operation::HeapFieldRead { receiver, .. }
        if receiver == exit_pair.1)
    );
    assert_ne!(exit_pair.1, outer_birth.1);
    let incoming = only(
        function
            .blocks
            .iter()
            .flat_map(edges)
            .filter(|edge| edge.target == exit.id),
    );
    assert_eq!(incoming, inner_false, "the real inner false exit");
    assert_eq!(
        carry_pair(function, incoming, outer_header_pair, &outer_pairs),
        exit_pair
    );
    let outer_end = only(function.instructions.iter().filter(|instruction| {
        matches!(instruction.operation, Operation::BorrowEnd { loan }
            if outer_pairs.iter().any(|(_, outer)| *outer == loan))
    }));
    assert!(matches!(outer_end.operation, Operation::BorrowEnd { loan } if loan == exit_pair.1));
    assert_eq!(outer_end.block, outer_read.block);
    assert_before(function, outer_end.block, outer_read.id, outer_end.id);
    for block in [inner.id, inner_true.target, backedge_block.id] {
        assert_ne!(outer_end.block, block, "only the outer exit ends its proof");
    }
}

fn birth_pair(function: &Function, header: &Block) -> (ValueId, Pair) {
    let TerminatorKind::NullableBranch {
        owner,
        when_null,
        when_non_null,
        view,
    } = term(header)
    else {
        panic!("real nullable birth")
    };
    let body = function.block(when_non_null.target).unwrap();
    let owner_column = column(&when_non_null.arguments, EntityId::Value(*owner));
    let EntityId::Value(body_owner) = body.parameters[owner_column] else {
        panic!("true owner column")
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

fn assert_scope_types(
    module: &Module,
    function: &Function,
    owner: ValueId,
    pairs: &Pairs,
) -> (SsaTypeId, SsaTypeId) {
    let EntityType::Value(nullable) = function.entity(EntityId::Value(owner)).unwrap().ty else {
        panic!("owned nullable storage")
    };
    let Some(SsaTypeKind::NullableHandle { inner }) = module.type_kind(nullable) else {
        panic!("nullable storage type")
    };
    assert!(matches!(
        module.type_kind(*inner),
        Some(SsaTypeKind::HeapOwner { .. })
    ));
    for &(owner, loan) in pairs {
        assert_eq!(
            function.entity(EntityId::Value(owner)).unwrap().ty,
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
    (nullable, *inner)
}

fn trace_pairs(function: &Function, initial: Pair) -> Pairs {
    let mut pairs = BTreeSet::from([initial]);
    loop {
        let mut added = Vec::new();
        for edge in function.blocks.iter().flat_map(edges) {
            for &(owner, loan) in &pairs {
                if edge.arguments.contains(&EntityId::Loan(loan)) {
                    let target = function.block(edge.target).unwrap();
                    let owner_column = column(&edge.arguments, EntityId::Value(owner));
                    let loan_column = column(&edge.arguments, EntityId::Loan(loan));
                    let (EntityId::Value(next_owner), EntityId::Loan(next_loan)) = (
                        target.parameters[owner_column],
                        target.parameters[loan_column],
                    ) else {
                        panic!("actual paired owner and loan edge columns")
                    };
                    added.push((next_owner, next_loan));
                }
            }
        }
        let before = pairs.len();
        pairs.extend(added);
        if pairs.len() == before {
            return pairs;
        }
    }
}

fn pair_in(block: &Block, pairs: &Pairs) -> Pair {
    only(pairs.iter().copied().filter(|(owner, loan)| {
        block.parameters.contains(&EntityId::Value(*owner))
            && block.parameters.contains(&EntityId::Loan(*loan))
    }))
}

fn carry_pair(function: &Function, edge: &Edge, pair: Pair, pairs: &Pairs) -> Pair {
    let owner_column = column(&edge.arguments, EntityId::Value(pair.0));
    let loan_column = column(&edge.arguments, EntityId::Loan(pair.1));
    let target = function.block(edge.target).unwrap();
    let next = pair_in(target, pairs);
    assert_eq!(target.parameters[owner_column], EntityId::Value(next.0));
    assert_eq!(target.parameters[loan_column], EntityId::Loan(next.1));
    next
}

fn column(entities: &[EntityId], entity: EntityId) -> usize {
    only(
        entities
            .iter()
            .enumerate()
            .filter_map(|(index, candidate)| (*candidate == entity).then_some(index)),
    )
}

fn assert_no_proof(entities: &[EntityId], pairs: &Pairs) {
    assert!(
        pairs
            .iter()
            .all(|(_, loan)| !entities.contains(&EntityId::Loan(*loan))),
        "inner proof must not be carried or resurrected"
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

fn term(block: &Block) -> &TerminatorKind {
    &block.terminator.as_ref().unwrap().kind
}

fn edges(block: &Block) -> Vec<&Edge> {
    match term(block) {
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
