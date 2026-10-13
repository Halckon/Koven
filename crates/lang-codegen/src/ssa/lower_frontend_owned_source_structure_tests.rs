//! Exact ordinary-source SSA lineage; only transparent block parameters are followed.
use crate::ssa::model::{
    BinaryOperator, BlockId, Definition, Edge, EntityId, Function, Operation, Origin, Program,
    ScalarConstant, TerminatorKind,
};
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
    let incoming = edges(function)
        .into_iter()
        .filter(|(_, edge)| edge.target == block)
        .collect::<Vec<_>>();
    !incoming.is_empty()
        && incoming
            .iter()
            .all(|(_, edge)| forwards(function, edge.arguments[index], expected, depth + 1))
}
pub(super) fn check(program: &Program, live: bool, seed_span: lang_frontend::source::Span) {
    let f = program.modules[0]
        .functions
        .iter()
        .find(|f| {
            f.instructions.iter().any(|i| {
                i.origin == Origin::Source(seed_span)
                    && matches!(i.operation, Operation::ContainerConstruct { .. })
            })
        })
        .unwrap();
    let seeds = f
        .instructions
        .iter()
        .filter(|i| {
            i.origin == Origin::Source(seed_span)
                && matches!(i.operation, Operation::ContainerConstruct { .. })
        })
        .collect::<Vec<_>>();
    assert_eq!(seeds.len(), 1);
    let seed = seeds[0].results[0];
    let all_edges = edges(f);
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
    let (preheader, entry, owner_slot) = entries[0];
    let header = f.block(entry.target).unwrap();
    let lengths = f
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::ContainerLength { .. }))
        .collect::<Vec<_>>();
    let snapshot = lengths.iter().find(|i| i.block == preheader).unwrap();
    let Operation::ContainerLength { owner: source } = snapshot.operation else {
        unreachable!()
    };
    let source_slot = entry
        .arguments
        .iter()
        .position(|arg| *arg == source)
        .unwrap();
    let length_slot = entry
        .arguments
        .iter()
        .position(|arg| *arg == snapshot.results[0])
        .unwrap();
    let zero = f
        .instructions
        .iter()
        .find(|i| {
            i.block == preheader
                && matches!(i.operation, Operation::Constant(ScalarConstant::Integer(0)))
        })
        .unwrap()
        .results[0];
    let cursor_slot = entry.arguments.iter().position(|arg| *arg == zero).unwrap();
    let one = f
        .instructions
        .iter()
        .find(|i| {
            i.block == preheader
                && i.id.index() > snapshot.id.index()
                && matches!(i.operation, Operation::Constant(ScalarConstant::Integer(1)))
        })
        .unwrap()
        .results[0];
    let root_place = f.instructions.iter().find(|i| matches!(i.operation, Operation::RootPlace { owner } if EntityId::Value(owner) == seed)).unwrap();
    let source_begin = f
        .instructions
        .iter()
        .find(|i| i.results.contains(&source))
        .unwrap();
    assert!(
        matches!(source_begin.operation, Operation::BorrowBegin { place, .. } if EntityId::Place(place) == root_place.results[0])
    );
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("provider guard")
    };
    let body = f.block(when_true.target).unwrap();
    let exhausted = f.block(when_false.target).unwrap();
    let body_slot = |slot| {
        when_true
            .arguments
            .iter()
            .position(|arg| *arg == header.parameters[slot])
            .unwrap()
    };
    let exhausted_slot = |slot| {
        when_false
            .arguments
            .iter()
            .position(|arg| *arg == header.parameters[slot])
            .unwrap()
    };
    let body_owner = body.parameters[body_slot(owner_slot)];
    let body_source = body.parameters[body_slot(source_slot)];
    let body_cursor = body.parameters[body_slot(cursor_slot)];
    let exhausted_owner = exhausted.parameters[exhausted_slot(owner_slot)];
    let exhausted_source = exhausted.parameters[exhausted_slot(source_slot)];
    let place = f
        .instructions
        .iter()
        .find(|i| matches!(i.operation, Operation::ContainerElementPlace { .. }))
        .unwrap();
    assert!(
        matches!(place.operation, Operation::ContainerElementPlace { owner, index } if owner == body_source && EntityId::Value(index) == body_cursor)
    );
    let element = f.instructions.iter().find(|i| matches!(i.operation, Operation::BorrowBegin { place: p, .. } if EntityId::Place(p) == place.results[0])).unwrap().results[0];
    let back = all_edges
        .iter()
        .filter(|(block, edge)| edge.target == header.id && *block != preheader)
        .collect::<Vec<_>>();
    assert_eq!(
        back.len(),
        2,
        "continue and normal fallthrough both transport"
    );
    for (block, edge) in back {
        for (slot, expected) in [(owner_slot, body_owner), (source_slot, body_source)] {
            assert!(forwards(f, edge.arguments[slot], expected, 0));
        }
        assert_eq!(
            edge.arguments[length_slot], header.parameters[length_slot],
            "same immutable length snapshot"
        );
        let advance = f
            .instructions
            .iter()
            .find(|i| i.results.contains(&edge.arguments[cursor_slot]))
            .unwrap();
        assert_eq!(advance.block, *block);
        assert!(
            matches!(advance.operation, Operation::Binary { operator: BinaryOperator::Add, left, right }
            if forwards(f, EntityId::Value(left), body_cursor, 0) && EntityId::Value(right) == one)
        );
        let ends = f
            .instructions
            .iter()
            .filter(|i| i.block == *block && matches!(i.operation, Operation::BorrowEnd { .. }))
            .collect::<Vec<_>>();
        assert!(!ends.iter().any(|i| matches!(i.operation, Operation::BorrowEnd { loan } if forwards(f, EntityId::Loan(loan), body_source, 0))), "source loan survives both backedges");
        let ends = ends.into_iter().filter(|i| matches!(i.operation, Operation::BorrowEnd { loan } if forwards(f, EntityId::Loan(loan), element, 0))).collect::<Vec<_>>();
        assert_eq!(
            ends.len(),
            1,
            "the actual element ends once on each iteration path"
        );
        let ids = &f.block(*block).unwrap().instructions;
        assert!(
            ids.iter().position(|id| *id == ends[0].id).unwrap()
                < ids.iter().position(|id| *id == advance.id).unwrap()
        );
    }
    let source_end = f.instructions.iter().filter(|i| i.block == exhausted.id && matches!(i.operation, Operation::BorrowEnd { loan } if EntityId::Loan(loan) == exhausted_source)).collect::<Vec<_>>();
    assert_eq!(source_end.len(), 1);
    let drops = f.instructions.iter().filter(|i| matches!(i.operation, Operation::Drop { owner } if forwards(f, EntityId::Value(owner), exhausted_owner, 0))).collect::<Vec<_>>();
    assert_eq!(drops.len(), 1, "exact exhausted source owner consumed once");
    if live {
        let post = lengths.iter().find(|i| i.block == drops[0].block).unwrap();
        let Operation::ContainerLength { owner: post_loan } = post.operation else {
            unreachable!()
        };
        let begin = f
            .instructions
            .iter()
            .find(|i| i.results.contains(&post_loan))
            .unwrap();
        let Operation::BorrowBegin { place, .. } = begin.operation else {
            panic!("post-use source loan")
        };
        let root = f
            .instructions
            .iter()
            .find(|i| i.results.contains(&EntityId::Place(place)))
            .unwrap();
        assert!(
            matches!(root.operation, Operation::RootPlace { owner } if forwards(f, EntityId::Value(owner), exhausted_owner, 0))
        );
        assert_eq!(post.block, drops[0].block);
        let ids = &f.block(post.block).unwrap().instructions;
        assert!(
            ids.iter().position(|id| *id == post.id).unwrap()
                < ids.iter().position(|id| *id == drops[0].id).unwrap()
        );
    } else {
        assert_eq!(drops[0].block, exhausted.id);
        let ids = &exhausted.instructions;
        assert!(
            ids.iter().position(|id| *id == source_end[0].id).unwrap()
                < ids.iter().position(|id| *id == drops[0].id).unwrap()
        );
    }
}
