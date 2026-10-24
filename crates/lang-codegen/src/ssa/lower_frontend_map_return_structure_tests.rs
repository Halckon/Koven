//! Ordinary-source SPEC-0313 oracle: one Map, two puts, one conditional Unit return.
use crate::ssa::model::{
    BinaryOperator, Block, BlockId, ComparisonOperator, Definition, Edge, EntityId, Function,
    Instruction, LoanKind, Operation, Origin, Program, ScalarConstant, TerminatorKind,
};
use lang_frontend::source::Span;

pub(super) fn edges(function: &Function) -> Vec<(BlockId, &Edge)> {
    function
        .blocks
        .iter()
        .flat_map(|block| {
            let outgoing = match &block.terminator.as_ref().unwrap().kind {
                TerminatorKind::Branch(edge) => vec![edge],
                TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                }
                | TerminatorKind::NullableBranch {
                    when_null: when_true,
                    when_non_null: when_false,
                    ..
                } => vec![when_true, when_false],
                _ => Vec::new(),
            };
            outgoing.into_iter().map(move |edge| (block.id, edge))
        })
        .collect()
}

// Only block parameters forward identity; even an older update of this Map fails.
pub(super) fn forwards(function: &Function, actual: EntityId, expected: EntityId) -> bool {
    fn visit(function: &Function, actual: EntityId, expected: EntityId, depth: usize) -> bool {
        if actual == expected {
            return true;
        }
        if depth > function.blocks.len() {
            return false;
        }
        let Definition::BlockParameter { block, index } =
            function.entity(actual).unwrap().definition
        else {
            return false;
        };
        let inputs = edges(function)
            .into_iter()
            .filter(|(_, edge)| edge.target == block)
            .collect::<Vec<_>>();
        !inputs.is_empty()
            && inputs
                .iter()
                .all(|(_, edge)| visit(function, edge.arguments[index], expected, depth + 1))
    }
    visit(function, actual, expected, 0)
}

pub(super) fn only(function: &Function, predicate: impl Fn(&Instruction) -> bool) -> &Instruction {
    let found = function
        .instructions
        .iter()
        .filter(|i| predicate(i))
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "one exact instruction: {found:?}");
    found[0]
}

pub(super) fn definition(function: &Function, entity: EntityId) -> &Instruction {
    let Definition::InstructionResult { instruction, .. } =
        function.entity(entity).unwrap().definition
    else {
        panic!("expected operation result: {entity:?}")
    };
    function.instruction(instruction).unwrap()
}

pub(super) fn slot(arguments: &[EntityId], entity: EntityId) -> usize {
    let found = arguments
        .iter()
        .enumerate()
        .filter_map(|(i, value)| (*value == entity).then_some(i))
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "one exact transport slot for {entity:?}");
    found[0]
}

pub(super) fn transport(function: &Function, edge: &Edge, entity: EntityId) -> EntityId {
    function.block(edge.target).unwrap().parameters[slot(&edge.arguments, entity)]
}

pub(super) fn branch(block: &Block) -> &Edge {
    let TerminatorKind::Branch(edge) = &block.terminator.as_ref().unwrap().kind else {
        panic!("expected unconditional branch from {:?}", block.id)
    };
    edge
}

pub(super) fn before(function: &Function, first: &Instruction, second: &Instruction) {
    assert_eq!(
        first.block, second.block,
        "this fixture has straight-line cleanup"
    );
    let instructions = &function.block(first.block).unwrap().instructions;
    let index = |instruction: &Instruction| {
        instructions
            .iter()
            .position(|id| *id == instruction.id)
            .unwrap()
    };
    assert!(index(first) < index(second));
}

pub(super) fn reaches(
    function: &Function,
    from: BlockId,
    target: BlockId,
    header: BlockId,
) -> bool {
    let mut pending = vec![from];
    let mut seen = Vec::new();
    while let Some(block) = pending.pop() {
        if block == header || seen.contains(&block) {
            continue;
        }
        if block == target {
            return true;
        }
        seen.push(block);
        pending.extend(
            edges(function)
                .into_iter()
                .filter(|(source, _)| *source == block)
                .map(|(_, edge)| edge.target),
        );
    }
    false
}

pub(super) fn assert_return_structure(
    program: &Program,
    return_span: Span,
    cleanup_span: Span,
    source_span: Span,
    live: bool,
) {
    let functions = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| {
            function.blocks.iter().any(|block| {
                block.terminator.as_ref().unwrap().origin == Origin::Source(return_span)
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(functions.len(), 1, "return selects its actual function");
    let function = functions[0];
    let seed = only(function, |i| {
        matches!(i.operation, Operation::MapConstruct { .. })
    });
    let length = only(function, |i| {
        matches!(i.operation, Operation::ContainerLength { .. })
    });
    let Operation::ContainerLength { owner: source_loan } = length.operation else {
        unreachable!()
    };
    let source_borrow = definition(function, source_loan);
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = source_borrow.operation
    else {
        panic!("provider borrows its hidden source owner")
    };
    let root = definition(function, EntityId::Place(place));
    let Operation::RootPlace { owner } = root.operation else {
        panic!("exact source root")
    };
    let source_owner = EntityId::Value(owner);
    let source = only(function, |i| {
        matches!(i.operation, Operation::ContainerConstruct { .. })
    });
    assert_eq!(source.results.as_slice(), &[source_owner]);
    assert_eq!(source.origin, Origin::Source(source_span));
    let preheader = function.block(length.block).unwrap();
    let all_edges = edges(function);
    assert!(
        all_edges
            .iter()
            .all(|(_, edge)| edge.target != preheader.id)
    );
    for instruction in [seed, source, root, source_borrow, length] {
        assert_eq!(instruction.block, preheader.id);
    }
    before(function, source, root);
    before(function, root, source_borrow);
    before(function, source_borrow, length);
    let enter = branch(preheader);
    let header = function.block(enter.target).unwrap();
    let map_slot = slot(&enter.arguments, seed.results[0]);
    let source_slot = slot(&enter.arguments, source_owner);
    let loan_slot = slot(&enter.arguments, source_loan);
    let snapshot_slot = slot(&enter.arguments, length.results[0]);
    let [header_map, header_source, header_loan, snapshot] =
        [map_slot, source_slot, loan_slot, snapshot_slot].map(|slot| header.parameters[slot]);
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("provider header guard")
    };
    let guard = definition(function, EntityId::Value(*condition));
    let Operation::Compare {
        operator: ComparisonOperator::LessThan,
        left: cursor,
        right,
    } = guard.operation
    else {
        panic!("cursor is compared with the entry length snapshot")
    };
    assert_eq!(guard.block, header.id);
    assert_eq!(EntityId::Value(right), snapshot);
    let cursor = EntityId::Value(cursor);
    let cursor_slot = slot(&header.parameters, cursor);
    assert_eq!(
        definition(function, enter.arguments[cursor_slot]).operation,
        Operation::Constant(ScalarConstant::Integer(0))
    );
    let body = function.block(when_true.target).unwrap();
    let exhausted = function.block(when_false.target).unwrap();
    assert_ne!(body.id, exhausted.id);
    for target in [body.id, exhausted.id] {
        let incoming = all_edges
            .iter()
            .filter(|(_, edge)| edge.target == target)
            .collect::<Vec<_>>();
        assert_eq!(incoming.len(), 1);
        assert_eq!(incoming[0].0, header.id);
    }
    let body_map = transport(function, when_true, header_map);
    let exhausted_map = transport(function, when_false, header_map);
    let body_source = transport(function, when_true, header_source);
    let exhausted_source = transport(function, when_false, header_source);
    let body_loan = transport(function, when_true, header_loan);
    let exhausted_loan = transport(function, when_false, header_loan);
    let body_cursor = transport(function, when_true, cursor);
    let element = only(function, |i| {
        matches!(i.operation, Operation::ContainerElementPlace { .. })
    });
    assert!(
        matches!(element.operation, Operation::ContainerElementPlace { owner, index }
        if owner == body_loan && EntityId::Value(index) == body_cursor)
    );
    assert_eq!(element.block, body.id);
    let element_borrow = only(function, |i| {
        matches!(i.operation,
        Operation::BorrowBegin { place, kind: LoanKind::Shared } if EntityId::Place(place) == element.results[0])
    });
    let element_loan = element_borrow.results[0];
    before(function, element, element_borrow);
    let updates = function
        .instructions
        .iter()
        .filter(|i| {
            matches!(
                i.operation,
                Operation::MapPut { .. } | Operation::MapRemove { .. }
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(updates.len(), 2);
    let head = *updates
        .iter()
        .find(|i| i.origin.span().end() <= return_span.start())
        .unwrap();
    let tail = *updates
        .iter()
        .find(|i| i.origin.span().start() >= return_span.end())
        .unwrap();
    assert_eq!(head.block, body.id);
    assert!(matches!(head.operation, Operation::MapPut { owner, .. }
        if forwards(function, EntityId::Value(owner), body_map)));
    assert!(matches!(tail.operation, Operation::MapPut { owner, .. }
        if forwards(function, EntityId::Value(owner), head.results[0])));
    assert_ne!(head.results[0], tail.results[0]);
    let returns = function
        .blocks
        .iter()
        .filter(|block| block.terminator.as_ref().unwrap().origin == Origin::Source(return_span))
        .collect::<Vec<_>>();
    assert_eq!(returns.len(), 1);
    let returned = returns[0];
    assert!(matches!(&returned.terminator.as_ref().unwrap().kind,
        TerminatorKind::Return { values } if values.is_empty()));
    assert!(all_edges.iter().all(|(block, _)| *block != returned.id));
    let inputs = all_edges
        .iter()
        .copied()
        .filter(|(_, edge)| edge.target == header.id)
        .collect::<Vec<_>>();
    assert_eq!(
        inputs.len(),
        2,
        "exactly Entry and Fallthrough, never Return"
    );
    let (fallthrough, backedge) = *inputs
        .iter()
        .find(|(block, _)| *block != preheader.id)
        .unwrap();
    assert_eq!(branch(function.block(fallthrough).unwrap()), backedge);
    for (slot, expected) in [
        (map_slot, tail.results[0]),
        (source_slot, body_source),
        (loan_slot, body_loan),
        (snapshot_slot, snapshot),
    ] {
        assert!(forwards(function, backedge.arguments[slot], expected));
    }
    assert!(!forwards(
        function,
        backedge.arguments[map_slot],
        head.results[0]
    ));
    let advance = only(function, |i| {
        matches!(
            i.operation,
            Operation::Binary {
                operator: BinaryOperator::Add,
                ..
            }
        )
    });
    assert_eq!(
        advance.results.as_slice(),
        &[backedge.arguments[cursor_slot]]
    );
    assert_eq!(advance.block, fallthrough);
    let Operation::Binary { left, right, .. } = advance.operation else {
        unreachable!()
    };
    assert!(forwards(function, EntityId::Value(left), body_cursor));
    let step = definition(function, EntityId::Value(right));
    assert_eq!(
        step.operation,
        Operation::Constant(ScalarConstant::Integer(1))
    );
    assert_eq!(step.block, preheader.id);
    let TerminatorKind::Conditional {
        when_true: return_edge,
        when_false: tail_edge,
        ..
    } = &function
        .block(head.block)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
        .kind
    else {
        panic!("ordinary condition selects return versus tail")
    };
    for (from, target, expected) in [
        (return_edge.target, returned.id, true),
        (return_edge.target, tail.block, false),
        (return_edge.target, advance.block, false),
        (return_edge.target, exhausted.id, false),
        (tail_edge.target, tail.block, true),
        (tail_edge.target, returned.id, false),
        (tail.block, fallthrough, true),
    ] {
        assert_eq!(reaches(function, from, target, header.id), expected);
    }
    let element_ends = function
        .instructions
        .iter()
        .filter(|i| {
            matches!(i.operation,
        Operation::BorrowEnd { loan } if forwards(function, EntityId::Loan(loan), element_loan))
        })
        .collect::<Vec<_>>();
    assert_eq!(element_ends.len(), 2);
    let source_ends = function
        .instructions
        .iter()
        .filter(|i| {
            matches!(i.operation,
        Operation::BorrowEnd { loan } if [source_loan, header_loan, body_loan, exhausted_loan]
            .iter().any(|expected| forwards(function, EntityId::Loan(loan), *expected)))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        source_ends.len(),
        2,
        "Return and exhaustion each end their source loan"
    );
    let source_drops = function
        .instructions
        .iter()
        .filter(|i| {
            matches!(i.operation,
        Operation::Drop { owner } if [source_owner, header_source, body_source, exhausted_source]
            .iter().any(|expected| forwards(function, EntityId::Value(owner), *expected)))
        })
        .collect::<Vec<_>>();
    assert_eq!(source_drops.len(), 2);
    let map_type = function.entity(seed.results[0]).unwrap().ty;
    let map_drops = function.instructions.iter().filter(|i| matches!(i.operation,
        Operation::Drop { owner } if function.entity(EntityId::Value(owner)).unwrap().ty == map_type)).collect::<Vec<_>>();
    assert_eq!(
        map_drops.len(),
        2,
        "enumerate by type, then prove each exact owner separately"
    );
    for (block, expected_map, expected_source, expected_loan) in [
        (returned.id, head.results[0], body_source, body_loan),
        (
            exhausted.id,
            exhausted_map,
            exhausted_source,
            exhausted_loan,
        ),
    ] {
        let end = *source_ends.iter().find(|i| i.block == block).unwrap();
        let source_drop = *source_drops.iter().find(|i| i.block == block).unwrap();
        let map_drop = *map_drops.iter().find(|i| i.block == block).unwrap();
        assert!(matches!(end.operation, Operation::BorrowEnd { loan }
            if forwards(function, EntityId::Loan(loan), expected_loan)));
        assert!(matches!(source_drop.operation, Operation::Drop { owner }
            if forwards(function, EntityId::Value(owner), expected_source)));
        assert!(matches!(map_drop.operation, Operation::Drop { owner }
            if forwards(function, EntityId::Value(owner), expected_map)));
        assert_eq!(source_drop.origin, Origin::Source(source_span));
        before(function, end, source_drop);
        before(function, source_drop, map_drop);
        if block == returned.id {
            let element_end = *element_ends.iter().find(|i| i.block == block).unwrap();
            before(function, element_end, end);
            assert_eq!(element_end.origin, Origin::Source(cleanup_span));
        }
    }
    let fallthrough_end = *element_ends
        .iter()
        .find(|i| i.block == fallthrough)
        .unwrap();
    before(function, tail, fallthrough_end);
    before(function, fallthrough_end, advance);
    let queries = function
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::MapContains { .. }))
        .collect::<Vec<_>>();
    assert_eq!(queries.len(), usize::from(live));
    if live {
        assert!(
            matches!(queries[0].operation, Operation::MapContains { owner, .. } if owner == exhausted_map)
        );
        before(
            function,
            source_drops
                .iter()
                .find(|i| i.block == exhausted.id)
                .unwrap(),
            queries[0],
        );
        before(
            function,
            queries[0],
            map_drops.iter().find(|i| i.block == exhausted.id).unwrap(),
        );
    }
}

// Declaration spans bind fixture labels to actual functions without depending
// on single-file names or reproducing the Unit generated-name convention.
pub(super) fn source_function(
    program: &Program,
    source: Span,
) -> (&crate::ssa::model::Module, &Function) {
    let functions = program
        .modules
        .iter()
        .flat_map(|module| {
            module.functions.iter().filter_map(move |function| {
                (function.origin == Origin::Source(source)).then_some((module, function))
            })
        })
        .collect::<Vec<_>>();
    let [function] = functions.as_slice() else {
        panic!("one exact source function for declaration {source:?}");
    };
    *function
}

// Append beside the existing oracle helpers; inspect ordinary, unmodified fixture SSA only.
pub(super) fn assert_distinct_return_owners(program: &Program, source: Span) {
    let (_, function) = source_function(program, source);
    let mut seeds = function
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::MapConstruct { .. }))
        .collect::<Vec<_>>();
    assert_eq!(seeds.len(), 2);
    seeds.sort_by_key(|i| i.origin.span().start());
    let map_type = function.entity(seeds[0].results[0]).unwrap().ty;
    assert_eq!(map_type, function.entity(seeds[1].results[0]).unwrap().ty);
    assert_ne!(seeds[0].results[0], seeds[1].results[0]);
    let initial_second = only(function, |i| {
        matches!(i.operation, Operation::MapPut { owner, .. }
        if forwards(function, EntityId::Value(owner), seeds[1].results[0]))
    });
    let length = only(function, |i| {
        matches!(i.operation, Operation::ContainerLength { .. })
    });
    let preheader = function.block(length.block).unwrap();
    let enter = branch(preheader);
    let header = function.block(enter.target).unwrap();
    assert_eq!(initial_second.block, preheader.id);
    let first_slot = slot(&enter.arguments, seeds[0].results[0]);
    let second_slot = slot(&enter.arguments, initial_second.results[0]);
    assert_ne!(first_slot, second_slot);
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("real provider header")
    };
    let body_first = transport(function, when_true, header.parameters[first_slot]);
    let body_second = transport(function, when_true, header.parameters[second_slot]);
    let exit_first = transport(function, when_false, header.parameters[first_slot]);
    let exit_second = transport(function, when_false, header.parameters[second_slot]);
    assert_ne!(body_first, body_second);
    assert_ne!(exit_first, exit_second);
    let head_first = only(function, |i| {
        matches!(i.operation, Operation::MapPut { owner, .. }
        if forwards(function, EntityId::Value(owner), body_first))
    });
    let head_second = only(function, |i| {
        matches!(i.operation, Operation::MapRemove { owner, .. }
        if forwards(function, EntityId::Value(owner), body_second))
    });
    let tail_first = only(function, |i| {
        matches!(i.operation, Operation::MapPut { owner, .. }
        if forwards(function, EntityId::Value(owner), head_first.results[0]))
    });
    let tail_second = only(function, |i| {
        matches!(i.operation, Operation::MapPut { owner, .. }
        if forwards(function, EntityId::Value(owner), head_second.results[0]))
    });
    let return_first = only(function, |i| {
        matches!(i.operation, Operation::Drop { owner }
        if forwards(function, EntityId::Value(owner), head_first.results[0]))
    });
    let return_second = only(function, |i| {
        matches!(i.operation, Operation::Drop { owner }
        if forwards(function, EntityId::Value(owner), head_second.results[0]))
    });
    assert_ne!(return_first.id, return_second.id);
    assert_eq!(return_first.block, return_second.block);
    let returned = function.block(return_first.block).unwrap();
    assert!(matches!(&returned.terminator.as_ref().unwrap().kind,
        TerminatorKind::Return { values } if values.is_empty()));
    for (drop, current, other, stale) in [
        (
            return_first,
            head_first.results[0],
            head_second.results[0],
            body_first,
        ),
        (
            return_second,
            head_second.results[0],
            head_first.results[0],
            body_second,
        ),
    ] {
        let Operation::Drop { owner } = drop.operation else {
            unreachable!()
        };
        let owner = EntityId::Value(owner);
        assert!(forwards(function, owner, current));
        assert!(!forwards(function, owner, other));
        assert!(!forwards(function, owner, stale));
    }
    let all_edges = edges(function);
    assert!(all_edges.iter().all(|(block, _)| *block != returned.id));
    let incoming = all_edges
        .iter()
        .copied()
        .filter(|(_, edge)| edge.target == header.id)
        .collect::<Vec<_>>();
    assert_eq!(
        incoming.len(),
        2,
        "exactly entry plus the real fallthrough backedge"
    );
    let (fallthrough, edge) = *incoming
        .iter()
        .find(|(block, _)| *block != preheader.id)
        .unwrap();
    assert_eq!(branch(function.block(fallthrough).unwrap()), edge);
    for (owner_slot, current, other, stale) in [
        (
            first_slot,
            tail_first.results[0],
            tail_second.results[0],
            head_first.results[0],
        ),
        (
            second_slot,
            tail_second.results[0],
            tail_first.results[0],
            head_second.results[0],
        ),
    ] {
        assert!(forwards(function, edge.arguments[owner_slot], current));
        assert!(!forwards(function, edge.arguments[owner_slot], other));
        assert!(!forwards(function, edge.arguments[owner_slot], stale));
    }
    for tail in [tail_first, tail_second] {
        assert!(reaches(function, tail.block, fallthrough, header.id));
        assert!(!reaches(function, returned.id, tail.block, header.id));
    }
    let after_first = only(function, |i| {
        matches!(i.operation, Operation::MapPut { owner, .. }
        if forwards(function, EntityId::Value(owner), exit_first))
    });
    let after_second = only(function, |i| {
        matches!(i.operation, Operation::MapRemove { owner, .. }
        if forwards(function, EntityId::Value(owner), exit_second))
    });
    for (current, other) in [
        (after_first.results[0], after_second.results[0]),
        (after_second.results[0], after_first.results[0]),
    ] {
        let drop = only(function, |i| {
            matches!(i.operation, Operation::Drop { owner }
            if forwards(function, EntityId::Value(owner), current))
        });
        assert_ne!(drop.block, returned.id);
        assert!(matches!(drop.operation, Operation::Drop { owner }
            if !forwards(function, EntityId::Value(owner), other)));
    }
    assert_eq!(function.instructions.iter().filter(|i| matches!(i.operation,
        Operation::Drop { owner } if function.entity(EntityId::Value(owner)).unwrap().ty == map_type))
        .count(), 4, "two distinct Map owners on each terminal path");
}

// The real operand call returns a Copyable value before the two body Resources
// are destroyed in reverse declaration order; it is the value handed to Return.
pub(super) fn assert_operand_before_body_cleanup(
    program: &Program,
    source: &str,
    cleanup: Span,
    operand: Span,
) {
    operand_before_body_cleanup(program, source, cleanup, operand, false);
}

pub(super) fn assert_owned_operand_before_body_cleanup(
    program: &Program,
    source: &str,
    cleanup: Span,
    operand: Span,
) {
    operand_before_body_cleanup(program, source, cleanup, operand, true);
}

fn operand_before_body_cleanup(
    program: &Program,
    source: &str,
    cleanup: Span,
    operand: Span,
    named_source: bool,
) {
    let (module, function) = source_function(program, cleanup);
    let (operand_module, operand) = source_function(program, operand);
    assert_eq!(module.id, operand_module.id);
    let call = only(
        function,
        |i| matches!(i.operation, Operation::DirectCall { callee, .. } if callee == operand.id),
    );
    let returned = function.block(call.block).unwrap();
    let TerminatorKind::Return { values } = &returned.terminator.as_ref().unwrap().kind else {
        panic!("the evaluated operand reaches the actual Return")
    };
    assert_eq!(values.len(), 1);
    assert_eq!(call.results, [EntityId::Value(values[0])]);

    let length = only(function, |i| {
        matches!(i.operation, Operation::ContainerLength { .. })
    });
    let Operation::ContainerLength { owner: source_loan } = length.operation else {
        unreachable!()
    };
    let source_borrow = definition(function, source_loan);
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = source_borrow.operation
    else {
        panic!("the provider holds a shared source loan")
    };
    let Operation::RootPlace { owner } = definition(function, EntityId::Place(place)).operation
    else {
        panic!("the source loan is rooted in the hidden source owner")
    };
    let source_owner = EntityId::Value(owner);
    let source_construct = definition(function, source_owner);
    assert!(matches!(
        source_construct.operation,
        Operation::ContainerConstruct { .. }
    ));
    let enter = branch(function.block(length.block).unwrap());
    let header = function.block(enter.target).unwrap();
    let source_slot = slot(&enter.arguments, source_owner);
    let loan_slot = slot(&enter.arguments, source_loan);
    let TerminatorKind::Conditional { when_true, .. } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("the provider's true edge enters the body")
    };
    let body_source = transport(function, when_true, header.parameters[source_slot]);
    let body_loan = transport(function, when_true, header.parameters[loan_slot]);
    let element = only(
        function,
        |i| matches!(i.operation, Operation::ContainerElementPlace { owner, .. } if owner == body_loan),
    );
    assert_eq!(element.block, when_true.target);
    let element_borrow = only(function, |i| {
        matches!(i.operation, Operation::BorrowBegin { place, kind: LoanKind::Shared }
            if EntityId::Place(place) == element.results[0])
    });
    before(function, element, element_borrow);
    let element_loan = element_borrow.results[0];
    assert!(matches!(
        module.type_kind(function.entity(element_loan).unwrap().ty.semantic_type()),
        Some(crate::ssa::model::SsaTypeKind::StringOwner)
    ));
    let Operation::DirectCall { arguments, .. } = &call.operation else {
        unreachable!()
    };
    assert_eq!(arguments.len(), 1);
    assert!(forwards(function, arguments[0], element_loan));

    // Source text selects each construction; the Drop must consume that exact owner.
    let guard_drop = |construction: &str| {
        let allocation = only(function, |i| {
            matches!(i.operation, Operation::HeapAllocate { .. })
                && source.get(i.origin.span().start()..i.origin.span().end()) == Some(construction)
        });
        only(function, |i| {
            i.block == returned.id
                && matches!(i.operation, Operation::Drop { owner }
                    if forwards(function, EntityId::Value(owner), allocation.results[0]))
        })
    };
    let second = guard_drop("Guard(\"later\")");
    let first = guard_drop("Guard(\"earlier\")");
    assert_ne!(first.id, second.id);
    let element_end = only(function, |i| {
        i.block == returned.id
            && matches!(i.operation, Operation::BorrowEnd { loan }
                if forwards(function, EntityId::Loan(loan), element_loan))
    });
    let source_end = only(function, |i| {
        i.block == returned.id
            && matches!(i.operation, Operation::BorrowEnd { loan }
                if forwards(function, EntityId::Loan(loan), body_loan))
    });
    assert_ne!(element_end.id, source_end.id);
    let source_drop = only(function, |i| {
        i.block == returned.id
            && matches!(i.operation, Operation::Drop { owner }
                if forwards(function, EntityId::Value(owner), body_source))
    });
    if named_source {
        let span = source_drop.origin.span();
        assert_eq!(source.get(span.start()..span.end()), Some("keys"));
    } else {
        assert_eq!(source_drop.origin, source_construct.origin);
    }
    let map_seed = only(function, |i| {
        matches!(i.operation, Operation::MapConstruct { .. })
    });
    let map_slot = slot(&enter.arguments, map_seed.results[0]);
    let body_map = transport(function, when_true, header.parameters[map_slot]);
    let head = only(function, |i| {
        matches!(i.operation, Operation::MapPut { owner, .. }
            if forwards(function, EntityId::Value(owner), body_map))
    });
    let map_drop = only(function, |i| {
        i.block == returned.id
            && matches!(i.operation, Operation::Drop { owner }
                if forwards(function, EntityId::Value(owner), head.results[0]))
    });

    let ordered = [
        call,
        second,
        first,
        element_end,
        source_end,
        source_drop,
        map_drop,
    ];
    for pair in ordered.windows(2) {
        before(function, pair[0], pair[1]);
    }
    assert!(
        !function
            .instructions
            .iter()
            .any(|i| { matches!(i.operation, Operation::Drop { owner } if owner == values[0]) })
    );
}
