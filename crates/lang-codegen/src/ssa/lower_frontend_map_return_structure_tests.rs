//! Ordinary-source SPEC-0313 oracle: one Map, two puts, one conditional Unit return.
use crate::ssa::model::{
    BinaryOperator, Block, BlockId, ComparisonOperator, Definition, Edge, EntityId, Function,
    Instruction, LoanKind, Operation, Origin, Program, ScalarConstant, TerminatorKind,
};
use lang_frontend::source::Span;

fn edges(function: &Function) -> Vec<(BlockId, &Edge)> {
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
fn forwards(function: &Function, actual: EntityId, expected: EntityId) -> bool {
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

fn only(function: &Function, predicate: impl Fn(&Instruction) -> bool) -> &Instruction {
    let found = function
        .instructions
        .iter()
        .filter(|i| predicate(i))
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "one exact instruction: {found:?}");
    found[0]
}

fn definition(function: &Function, entity: EntityId) -> &Instruction {
    let Definition::InstructionResult { instruction, .. } =
        function.entity(entity).unwrap().definition
    else {
        panic!("expected operation result: {entity:?}")
    };
    function.instruction(instruction).unwrap()
}

fn slot(arguments: &[EntityId], entity: EntityId) -> usize {
    let found = arguments
        .iter()
        .enumerate()
        .filter_map(|(i, value)| (*value == entity).then_some(i))
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "one exact transport slot for {entity:?}");
    found[0]
}

fn transport(function: &Function, edge: &Edge, entity: EntityId) -> EntityId {
    function.block(edge.target).unwrap().parameters[slot(&edge.arguments, entity)]
}

fn branch(block: &Block) -> &Edge {
    let TerminatorKind::Branch(edge) = &block.terminator.as_ref().unwrap().kind else {
        panic!("expected unconditional branch from {:?}", block.id)
    };
    edge
}

fn before(function: &Function, first: &Instruction, second: &Instruction) {
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

fn reaches(function: &Function, from: BlockId, target: BlockId, header: BlockId) -> bool {
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
            assert_eq!(element_end.origin, Origin::Source(return_span));
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
