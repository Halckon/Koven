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
fn forwards(function: &Function, value: EntityId, expected: EntityId, _depth: usize) -> bool {
    fn check(
        function: &Function,
        value: EntityId,
        expected: EntityId,
        visited: &mut std::collections::BTreeSet<EntityId>,
        found: &mut bool,
    ) -> bool {
        if value == expected {
            *found = true;
            return true;
        }
        if !visited.insert(value) {
            return true;
        }
        let Some(ed) = function.entity(value) else {
            return false;
        };
        let Definition::BlockParameter { block, index } = ed.definition else {
            return false;
        };
        let incoming = edges(function)
            .into_iter()
            .filter(|(_, edge)| edge.target == block)
            .collect::<Vec<_>>();
        !incoming.is_empty()
            && incoming
                .iter()
                .all(|(_, edge)| check(function, edge.arguments[index], expected, visited, found))
    }
    let mut visited = std::collections::BTreeSet::new();
    let mut found = false;
    check(function, value, expected, &mut visited, &mut found) && found
}
pub(super) fn check(
    program: &Program,
    live: bool,
    seed_span: Option<lang_frontend::source::Span>,
    return_span: lang_frontend::source::Span,
    map: bool,
) {
    let functions = program
        .modules
        .iter()
        .flat_map(|m| &m.functions)
        .filter(|f| {
            f.blocks
                .iter()
                .any(|b| b.terminator.as_ref().unwrap().origin == Origin::Source(return_span))
        })
        .collect::<Vec<_>>();
    assert_eq!(functions.len(), 1);
    let f = functions[0];
    let seed = if let Some(span) = seed_span {
        let seeds = f
            .instructions
            .iter()
            .filter(|i| {
                i.origin == Origin::Source(span)
                    && matches!(i.operation, Operation::ContainerConstruct { .. })
            })
            .collect::<Vec<_>>();
        assert_eq!(seeds.len(), 1, "the exact frontend declaration initializer");
        seeds[0].results[0]
    } else {
        // These ordinary-source fixtures declare own keys as the first parameter.
        let entry = &f.blocks[0];
        assert!(!entry.parameters.is_empty());
        let seed = entry.parameters[0];
        assert!(
            matches!(f.entity(seed).unwrap().definition, Definition::BlockParameter { block, index: 0 } if block == entry.id)
        );
        seed
    };
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
    let root_place = f.instructions.iter().find(|i| matches!(i.operation, Operation::RootPlace { owner } if EntityId::Value(owner) == seed)).unwrap();
    let source_begin = f.instructions.iter().find(|i| matches!(i.operation, Operation::BorrowBegin { place, .. } if EntityId::Place(place) == root_place.results[0])).unwrap();
    let source = source_begin.results[0];
    let snapshots = lengths
        .iter()
        .filter(|i| matches!(i.operation, Operation::ContainerLength { owner } if owner == source))
        .collect::<Vec<_>>();
    assert_eq!(
        snapshots.len(),
        1,
        "one length snapshot for the actual initial source loan"
    );
    let snapshot = snapshots[0];
    assert_eq!(snapshot.block, preheader);
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
        .find(|i| matches!(i.operation, Operation::ContainerElementPlace { owner, .. } if owner == body_source))
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
        1,
        "only Fallthrough transports; Return has no incoming"
    );
    for (block, edge) in back {
        for (slot, expected) in [(owner_slot, body_owner), (source_slot, body_source)] {
            assert!(forwards(f, edge.arguments[slot], expected, 0));
        }
        assert!(
            forwards(
                f,
                edge.arguments[length_slot],
                header.parameters[length_slot],
                0
            ),
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
            if forwards(f, EntityId::Value(left), body_cursor, 0) && forwards(f, EntityId::Value(right), one, 0))
        );
        let ends = f
            .instructions
            .iter()
            .filter(|i| i.block == *block && matches!(i.operation, Operation::BorrowEnd { .. }))
            .collect::<Vec<_>>();
        assert!(!ends.iter().any(|i| matches!(i.operation, Operation::BorrowEnd { loan } if forwards(f, EntityId::Loan(loan), body_source, 0))), "source loan survives the sole backedge");
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
    let returned = f
        .blocks
        .iter()
        .find(|b| b.terminator.as_ref().unwrap().origin == Origin::Source(return_span))
        .unwrap();
    assert!(matches!(
        returned.terminator.as_ref().unwrap().kind,
        TerminatorKind::Return { .. }
    ));
    assert!(
        !all_edges.iter().any(|(block, _)| *block == returned.id),
        "Return cannot reach Header or Exit"
    );
    let return_end = f.instructions.iter().filter(|i| i.block == returned.id && matches!(i.operation, Operation::BorrowEnd { loan } if forwards(f, EntityId::Loan(loan), body_source, 0))).collect::<Vec<_>>();
    let element_end = f.instructions.iter().filter(|i| i.block == returned.id && matches!(i.operation, Operation::BorrowEnd { loan } if forwards(f, EntityId::Loan(loan), element, 0))).collect::<Vec<_>>();
    let return_drop = f.instructions.iter().filter(|i| i.block == returned.id && matches!(i.operation, Operation::Drop { owner } if forwards(f, EntityId::Value(owner), body_owner, 0))).collect::<Vec<_>>();
    assert_eq!(return_end.len(), 1);
    assert_eq!(element_end.len(), 1);
    assert_eq!(
        return_drop.len(),
        1,
        "the actual source owner, never a same-type decoy"
    );
    let index = |i: &crate::ssa::model::Instruction| {
        returned
            .instructions
            .iter()
            .position(|id| *id == i.id)
            .unwrap()
    };
    assert!(index(element_end[0]) < index(return_end[0]));
    assert!(index(return_end[0]) < index(return_drop[0]));
    if map {
        let put = f
            .instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::MapPut { .. }))
            .collect::<Vec<_>>();
        assert_eq!(put.len(), 1);
        let drops = f.instructions.iter().filter(|i| i.block == returned.id && matches!(i.operation, Operation::Drop { owner } if forwards(f, EntityId::Value(owner), put[0].results[0], 0))).collect::<Vec<_>>();
        assert_eq!(drops.len(), 1, "Return consumes the latest MapPut result");
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
