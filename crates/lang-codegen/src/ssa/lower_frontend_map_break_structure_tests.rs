//! Inspect only real-source SSA. No IR mutation or synthetic fixtures.
use crate::ssa::model::{
    BlockId, Definition, Edge, EntityId, Function, Operation, Origin, Program, TerminatorKind,
};
use lang_frontend::source::Span;

fn edges(function: &Function) -> Vec<(BlockId, &Edge)> {
    function
        .blocks
        .iter()
        .flat_map(|block| {
            let edges = match &block.terminator.as_ref().unwrap().kind {
                TerminatorKind::Branch(edge) => vec![edge],
                TerminatorKind::Conditional {
                    when_true,
                    when_false,
                    ..
                } => vec![when_true, when_false],
                _ => Vec::new(),
            };
            edges.into_iter().map(move |edge| (block.id, edge))
        })
        .collect()
}

// The selected fixture has no intervening Map update: forwarding must terminate
// at the exact operation result, not merely at any owner of the same type.
fn forwards(function: &Function, value: EntityId, expected: EntityId, depth: usize) -> bool {
    if value == expected {
        return true;
    }
    if depth > function.blocks.len() {
        return false;
    }
    let Definition::BlockParameter { block, index } = function.entity(value).unwrap().definition
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
            .all(|(_, edge)| forwards(function, edge.arguments[index], expected, depth + 1))
}

pub(super) fn assert_exit_structure(
    program: &Program,
    break_span: Span,
    source_span: Span,
    live: bool,
) {
    let function = &program.modules[0].functions[0];
    let seed = function
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::MapConstruct { .. }))
        .collect::<Vec<_>>();
    assert_eq!(seed.len(), 1);
    let seed = seed[0].results[0];
    let all_edges = edges(function);
    let entries = all_edges
        .iter()
        .filter_map(|(block, edge)| {
            edge.arguments
                .iter()
                .position(|arg| *arg == seed)
                .map(|slot| (*block, *edge, slot))
        })
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 1);
    let (preheader, entry, map_slot) = entries[0];
    let header = function.block(entry.target).unwrap();
    let header_owner = header.parameters[map_slot];
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("real provider header")
    };
    assert_eq!(when_true.arguments[map_slot], header_owner);
    assert_eq!(when_false.arguments[map_slot], header_owner);
    let body = function.block(when_true.target).unwrap();
    let exhausted = function.block(when_false.target).unwrap();
    let body_owner = body.parameters[map_slot];
    let exhausted_owner = exhausted.parameters[map_slot];
    let puts = function
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::MapPut { .. }))
        .collect::<Vec<_>>();
    assert_eq!(puts.len(), 1);
    let Operation::MapPut { owner, .. } = puts[0].operation else {
        unreachable!()
    };
    assert!(forwards(function, EntityId::Value(owner), body_owner, 0));
    let updated = puts[0].results[0];
    let back = all_edges
        .iter()
        .filter(|(block, edge)| edge.target == header.id && *block != preheader)
        .collect::<Vec<_>>();
    assert_eq!(back.len(), 1);
    assert!(forwards(
        function,
        back[0].1.arguments[map_slot],
        updated,
        0
    ));

    let ends = function
        .instructions
        .iter()
        .filter(|i| {
            i.origin == Origin::Source(break_span)
                && matches!(i.operation, Operation::BorrowEnd { .. })
        })
        .collect::<Vec<_>>();
    assert_eq!(ends.len(), 2, "break ends element then provider source");
    assert_eq!(ends[0].block, ends[1].block);
    let break_block = function.block(ends[0].block).unwrap();
    let element_place = function
        .instructions
        .iter()
        .find(|i| matches!(i.operation, Operation::ContainerElementPlace { .. }))
        .unwrap();
    let Operation::ContainerElementPlace {
        owner: source_loan, ..
    } = element_place.operation
    else {
        unreachable!()
    };
    let element_loan = function.instructions.iter().find(|i| matches!(i.operation, Operation::BorrowBegin { place, .. } if EntityId::Place(place) == element_place.results[0])).unwrap().results[0];
    for (end, expected) in [(ends[0], element_loan), (ends[1], source_loan)] {
        let Operation::BorrowEnd { loan } = end.operation else {
            unreachable!()
        };
        assert!(forwards(function, EntityId::Loan(loan), expected, 0));
    }
    let position = |block: BlockId, id| {
        function
            .block(block)
            .unwrap()
            .instructions
            .iter()
            .position(|candidate| *candidate == id)
            .unwrap()
    };
    assert!(position(break_block.id, ends[0].id) < position(break_block.id, ends[1].id));

    let source_drops = function
        .instructions
        .iter()
        .filter(|i| {
            i.origin == Origin::Source(source_span) && matches!(i.operation, Operation::Drop { .. })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        source_drops.len(),
        2,
        "temporary source is consumed on each actual exit"
    );
    for block in [break_block, exhausted] {
        let source_drop = source_drops
            .iter()
            .find(|drop| drop.block == block.id)
            .unwrap();
        let source_ends = block
            .instructions
            .iter()
            .filter_map(|id| function.instruction(*id))
            .filter(|i| matches!(i.operation, Operation::BorrowEnd { .. }))
            .collect::<Vec<_>>();
        assert_eq!(
            source_ends.len(),
            if block.id == break_block.id { 2 } else { 1 }
        );
        assert!(
            source_ends
                .iter()
                .all(|end| position(block.id, end.id) < position(block.id, source_drop.id))
        );
    }
    let map_drops = function
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::Drop { .. }) && !source_drops.contains(i))
        .collect::<Vec<_>>();
    // The String literal also has a drop after clone. Select the Map by actual
    // forwarding from the corresponding exit owner, never by SsaTypeId.
    let TerminatorKind::Branch(break_edge) = &break_block.terminator.as_ref().unwrap().kind else {
        panic!("break reaches exit merge")
    };
    let TerminatorKind::Branch(exhausted_edge) = &exhausted.terminator.as_ref().unwrap().kind
    else {
        panic!("exhaustion reaches exit merge")
    };
    assert_eq!(break_edge.target, exhausted_edge.target);
    assert_ne!(break_block.id, exhausted.id);
    let joined = function.block(break_edge.target).unwrap();
    if live {
        let slots = break_edge
            .arguments
            .iter()
            .zip(&exhausted_edge.arguments)
            .enumerate()
            .filter(|(_, (a, b))| forwards(function, **a, updated, 0) && **b == exhausted_owner)
            .collect::<Vec<_>>();
        assert_eq!(slots.len(), 1, "one exact current owner from each exit");
        let merged = joined.parameters[slots[0].0];
        let final_drops = map_drops.iter().filter(|i| matches!(i.operation, Operation::Drop { owner } if EntityId::Value(owner) == merged)).collect::<Vec<_>>();
        assert_eq!(final_drops.len(), 1);
        assert_eq!(final_drops[0].block, joined.id);
        assert!(
            !map_drops
                .iter()
                .any(|i| [break_block.id, exhausted.id].contains(&i.block))
        );
    } else {
        for (block, expected) in [(break_block, updated), (exhausted, exhausted_owner)] {
            let drops = map_drops.iter().filter(|i| i.block == block.id && matches!(i.operation, Operation::Drop { owner } if forwards(function, EntityId::Value(owner), expected, 0))).collect::<Vec<_>>();
            assert_eq!(drops.len(), 1);
            let source_drop = source_drops.iter().find(|i| i.block == block.id).unwrap();
            assert!(position(block.id, source_drop.id) < position(block.id, drops[0].id));
        }
        assert!(
            joined.parameters.is_empty(),
            "dead Map must not reappear in exit"
        );
        assert!(!map_drops.iter().any(|i| i.block == joined.id));
    }
}
