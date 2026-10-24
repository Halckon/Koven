//! Independent SPEC-0311 oracle over unmodified ordinary-source SSA.
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
                } => vec![when_true, when_false],
                TerminatorKind::NullableBranch {
                    when_null,
                    when_non_null,
                    ..
                } => vec![when_null, when_non_null],
                _ => Vec::new(),
            };
            outgoing.into_iter().map(move |edge| (block.id, edge))
        })
        .collect()
}

// Only block parameters are transparent. In particular, a previous MapPut result,
// another update of the same Map, a reborrow, or a same-typed owner is not equal.
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
        let incoming = edges(function)
            .into_iter()
            .filter(|(_, edge)| edge.target == block)
            .collect::<Vec<_>>();
        !incoming.is_empty()
            && incoming
                .iter()
                .all(|(_, edge)| visit(function, edge.arguments[index], expected, depth + 1))
    }
    visit(function, actual, expected, 0)
}

fn only(function: &Function, predicate: impl Fn(&Instruction) -> bool) -> &Instruction {
    let found = function
        .instructions
        .iter()
        .filter(|instruction| predicate(instruction))
        .collect::<Vec<_>>();
    assert_eq!(found.len(), 1, "expected one exact instruction: {found:?}");
    found[0]
}

fn definition(function: &Function, entity: EntityId) -> &Instruction {
    let Definition::InstructionResult { instruction, .. } =
        function.entity(entity).unwrap().definition
    else {
        panic!("expected actual operation result: {entity:?}")
    };
    function.instruction(instruction).unwrap()
}

fn slot(arguments: &[EntityId], entity: EntityId) -> usize {
    let slots = arguments
        .iter()
        .enumerate()
        .filter_map(|(index, value)| (*value == entity).then_some(index))
        .collect::<Vec<_>>();
    assert_eq!(slots.len(), 1, "one exact transport slot for {entity:?}");
    slots[0]
}

fn transport(function: &Function, edge: &Edge, entity: EntityId) -> EntityId {
    function.block(edge.target).unwrap().parameters[slot(&edge.arguments, entity)]
}

fn branch(block: &Block) -> &Edge {
    let TerminatorKind::Branch(edge) = &block.terminator.as_ref().unwrap().kind else {
        panic!("expected branch from {:?}", block.id)
    };
    edge
}

fn position(function: &Function, instruction: &Instruction) -> usize {
    function
        .block(instruction.block)
        .unwrap()
        .instructions
        .iter()
        .position(|id| *id == instruction.id)
        .unwrap()
}

fn before(function: &Function, first: &Instruction, second: &Instruction) {
    assert_eq!(
        first.block, second.block,
        "ordered cleanup in the actual block"
    );
    assert!(position(function, first) < position(function, second));
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

fn function_at_continue(program: &Program, span: Span) -> &Function {
    let functions = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| {
            function
                .blocks
                .iter()
                .any(|block| block.terminator.as_ref().unwrap().origin == Origin::Source(span))
        })
        .collect::<Vec<_>>();
    assert_eq!(functions.len(), 1, "continue selects its actual function");
    functions[0]
}

fn end_in(function: &Function, block: BlockId, expected: EntityId) -> &Instruction {
    only(function, |instruction| {
        instruction.block == block
            && matches!(instruction.operation, Operation::BorrowEnd { loan }
                if forwards(function, EntityId::Loan(loan), expected))
    })
}

fn update_owner(instruction: &Instruction) -> EntityId {
    match instruction.operation {
        Operation::MapPut { owner, .. } | Operation::MapRemove { owner, .. } => {
            EntityId::Value(owner)
        }
        _ => panic!("expected Map update"),
    }
}

pub(super) fn assert_continue_structure(
    program: &Program,
    continue_span: Span,
    cleanup_span: Span,
    source_span: Span,
    live: bool,
) {
    let function = function_at_continue(program, continue_span);
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
        panic!("source loan has the exact hidden owner root")
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
    let source_owner_slot = slot(&enter.arguments, source_owner);
    let source_loan_slot = slot(&enter.arguments, source_loan);
    let snapshot_slot = slot(&enter.arguments, length.results[0]);
    let header_map = header.parameters[map_slot];
    let header_source = header.parameters[source_owner_slot];
    let header_loan = header.parameters[source_loan_slot];
    let snapshot = header.parameters[snapshot_slot];
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("provider header guards body versus exhaustion")
    };
    let guard = definition(function, EntityId::Value(*condition));
    let Operation::Compare {
        operator: ComparisonOperator::LessThan,
        left: cursor,
        right: compared_length,
    } = guard.operation
    else {
        panic!("header compares current cursor with the entry length snapshot")
    };
    assert_eq!(guard.block, header.id);
    assert_eq!(EntityId::Value(compared_length), snapshot);
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
    let Operation::ContainerElementPlace { owner, index } = element.operation else {
        unreachable!()
    };
    assert_eq!(element.block, body.id);
    assert_eq!(owner, body_loan);
    assert_eq!(EntityId::Value(index), body_cursor);
    let element_borrow = only(function, |i| {
        matches!(i.operation, Operation::BorrowBegin { place, kind: LoanKind::Shared }
            if EntityId::Place(place) == element.results[0])
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
    assert_eq!(updates.len(), 2, "one pre-continue and one tail update");
    let head = updates
        .iter()
        .copied()
        .find(|i| i.origin.span().end() <= continue_span.start())
        .unwrap();
    let tail = updates
        .iter()
        .copied()
        .find(|i| i.origin.span().start() >= continue_span.end())
        .unwrap();
    assert!(matches!(head.operation, Operation::MapPut { .. }));
    assert_eq!(head.block, body.id);
    assert!(forwards(function, update_owner(head), body_map));
    assert!(forwards(function, update_owner(tail), head.results[0]));
    assert_ne!(head.results[0], tail.results[0]);

    let continue_blocks = function
        .blocks
        .iter()
        .filter(|block| block.terminator.as_ref().unwrap().origin == Origin::Source(continue_span))
        .collect::<Vec<_>>();
    assert_eq!(continue_blocks.len(), 1);
    let continued = continue_blocks[0];
    let continue_edge = branch(continued);
    assert_eq!(continue_edge.target, header.id);
    let backedges = all_edges
        .iter()
        .filter(|(block, edge)| edge.target == header.id && *block != preheader.id)
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(
        backedges.len(),
        2,
        "both actual Header backedges, no others"
    );
    assert_eq!(
        all_edges
            .iter()
            .filter(|(_, edge)| edge.target == header.id)
            .count(),
        3,
        "Header incoming set is exactly entry, continue, fallthrough"
    );
    let (fallthrough, fallthrough_edge) = backedges
        .iter()
        .copied()
        .find(|(block, _)| *block != continued.id)
        .unwrap();
    let TerminatorKind::Conditional {
        when_true: selected_continue,
        when_false: selected_tail,
        ..
    } = &body.terminator.as_ref().unwrap().kind
    else {
        panic!("ordinary source condition selects continue versus tail")
    };
    assert!(reaches(
        function,
        selected_continue.target,
        continued.id,
        header.id
    ));
    assert!(!reaches(
        function,
        selected_continue.target,
        tail.block,
        header.id
    ));
    assert!(reaches(
        function,
        selected_tail.target,
        tail.block,
        header.id
    ));
    assert!(reaches(function, tail.block, fallthrough, header.id));
    assert!(!reaches(
        function,
        selected_tail.target,
        continued.id,
        header.id
    ));

    for (block, edge, current) in [
        (continued.id, continue_edge, head.results[0]),
        (fallthrough, fallthrough_edge, tail.results[0]),
    ] {
        assert_eq!(branch(function.block(block).unwrap()), edge);
        assert!(forwards(function, edge.arguments[map_slot], current));
        assert!(forwards(
            function,
            edge.arguments[source_owner_slot],
            body_source
        ));
        assert!(forwards(
            function,
            edge.arguments[source_loan_slot],
            body_loan
        ));
        assert!(forwards(function, edge.arguments[snapshot_slot], snapshot));
        let advance = definition(function, edge.arguments[cursor_slot]);
        let Operation::Binary {
            operator: BinaryOperator::Add,
            left,
            right,
        } = advance.operation
        else {
            panic!("each backedge carries its own cursor+1 result")
        };
        assert_eq!(advance.block, block);
        assert!(forwards(function, EntityId::Value(left), body_cursor));
        let step = definition(function, EntityId::Value(right));
        assert_eq!(
            step.operation,
            Operation::Constant(ScalarConstant::Integer(1))
        );
        assert_eq!(step.block, preheader.id);
        let element_end = end_in(function, block, element_loan);
        before(function, element_end, advance);
        if block == continued.id {
            assert_eq!(element_end.origin, Origin::Source(cleanup_span));
        }
    }
    assert!(!forwards(
        function,
        continue_edge.arguments[map_slot],
        tail.results[0]
    ));
    assert!(!forwards(
        function,
        fallthrough_edge.arguments[map_slot],
        head.results[0]
    ));

    // Check all cleanup sites, not just the instructions already selected above.
    let element_ends = function
        .instructions
        .iter()
        .filter(|i| {
            matches!(i.operation, Operation::BorrowEnd { loan }
            if forwards(function, EntityId::Loan(loan), element_loan))
        })
        .collect::<Vec<_>>();
    assert_eq!(element_ends.len(), 2);
    let source_ends = function
        .instructions
        .iter()
        .filter(|i| {
            matches!(i.operation, Operation::BorrowEnd { loan }
            if [source_loan, header_loan, body_loan, exhausted_loan].iter()
                .any(|expected| forwards(function, EntityId::Loan(loan), *expected)))
        })
        .collect::<Vec<_>>();
    assert_eq!(source_ends.len(), 1, "continue never finishes the provider");
    assert_eq!(source_ends[0].block, exhausted.id);
    assert!(
        matches!(source_ends[0].operation, Operation::BorrowEnd { loan }
        if EntityId::Loan(loan) == exhausted_loan)
    );
    let source_drops = function
        .instructions
        .iter()
        .filter(|i| {
            matches!(i.operation, Operation::Drop { owner }
            if [source_owner, header_source, body_source, exhausted_source].iter()
                .any(|expected| forwards(function, EntityId::Value(owner), *expected)))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        source_drops.len(),
        1,
        "hidden source survives both backedges"
    );
    assert_eq!(source_drops[0].origin, Origin::Source(source_span));
    assert!(
        matches!(source_drops[0].operation, Operation::Drop { owner }
        if EntityId::Value(owner) == exhausted_source)
    );
    before(function, source_ends[0], source_drops[0]);

    // The type enumerates every Map drop in this single-seed fixture; identity is
    // then proved separately against the exact exhaustion parameter.
    let map_type = function.entity(seed.results[0]).unwrap().ty;
    let map_drop = only(function, |i| {
        matches!(i.operation, Operation::Drop { owner }
            if function.entity(EntityId::Value(owner)).unwrap().ty == map_type)
    });
    assert!(matches!(map_drop.operation, Operation::Drop { owner }
        if EntityId::Value(owner) == exhausted_map));
    assert_eq!(map_drop.block, exhausted.id);
    before(function, source_drops[0], map_drop);
    let queries = function
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::MapContains { .. }))
        .collect::<Vec<_>>();
    assert_eq!(queries.len(), usize::from(live));
    if live {
        assert!(
            matches!(queries[0].operation, Operation::MapContains { owner, .. }
            if owner == exhausted_map)
        );
        before(function, source_drops[0], queries[0]);
        before(function, queries[0], map_drop);
    }
}

pub(super) fn assert_distinct_owner_backedges(program: &Program, source: Span) {
    // A fixture label chooses the test subject only. Each owner is identified by
    // its real construction and operation results, never by the shared Map type.
    let functions = program
        .modules
        .iter()
        .flat_map(|module| &module.functions)
        .filter(|function| function.origin == Origin::Source(source))
        .collect::<Vec<_>>();
    let [function] = functions.as_slice() else {
        panic!("one original separateOwners function");
    };
    let mut seeds = function
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::MapConstruct { .. }))
        .collect::<Vec<_>>();
    assert_eq!(seeds.len(), 2);
    seeds.sort_by_key(|i| i.origin.span().start());
    assert_eq!(
        function.entity(seeds[0].results[0]).unwrap().ty,
        function.entity(seeds[1].results[0]).unwrap().ty
    );
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
    let incoming = edges(function)
        .into_iter()
        .filter(|(_, edge)| edge.target == header.id)
        .collect::<Vec<_>>();
    assert_eq!(incoming.len(), 3, "exact entry plus two real backedges");
    let backedges = incoming
        .iter()
        .copied()
        .filter(|(block, _)| *block != preheader.id)
        .collect::<Vec<_>>();
    assert_eq!(backedges.len(), 2);
    let is_continue = |block: BlockId| {
        let origin = &function
            .block(block)
            .unwrap()
            .terminator
            .as_ref()
            .unwrap()
            .origin;
        let span = origin.span();
        matches!(origin, Origin::Source(_))
            && super::DISTINCT.get(span.start()..span.end()) == Some("continue")
    };
    assert_eq!(
        backedges
            .iter()
            .filter(|(block, _)| is_continue(*block))
            .count(),
        1
    );
    for (block, edge) in backedges {
        let expected = if is_continue(block) {
            [head_first.results[0], head_second.results[0]]
        } else {
            assert!(reaches(function, tail_first.block, block, header.id));
            assert!(reaches(function, tail_second.block, block, header.id));
            [tail_first.results[0], tail_second.results[0]]
        };
        for (owner_slot, owner, other) in [
            (first_slot, expected[0], expected[1]),
            (second_slot, expected[1], expected[0]),
        ] {
            assert!(forwards(function, edge.arguments[owner_slot], owner));
            assert!(!forwards(function, edge.arguments[owner_slot], other));
        }
    }
    let after_first = only(function, |i| {
        matches!(i.operation, Operation::MapPut { owner, .. }
            if forwards(function, EntityId::Value(owner), exit_first))
    });
    let after_second = only(function, |i| {
        matches!(i.operation, Operation::MapRemove { owner, .. }
            if forwards(function, EntityId::Value(owner), exit_second))
    });
    // Both final drops must use their own final post-loop update. This also
    // catches aliasing that happens only after the two Header slots leave SSA.
    for updated in [after_first.results[0], after_second.results[0]] {
        only(function, |i| {
            matches!(i.operation, Operation::Drop { owner }
            if forwards(function, EntityId::Value(owner), updated))
        });
    }
    let map_type = function.entity(seeds[0].results[0]).unwrap().ty;
    assert_eq!(
        function
            .instructions
            .iter()
            .filter(|i| {
                matches!(i.operation, Operation::Drop { owner }
            if function.entity(EntityId::Value(owner)).unwrap().ty == map_type)
            })
            .count(),
        2
    );
}

#[test]
fn single_map_continue_cleanup_preserves_provider_and_reverses_body_locals() {
    let text = r#"
        value class Entry(val key: Int, val text: String)
        class Guard(val name: String) { deinit() { println(this.name) } }
        fun earlier(): Guard = Guard("earlier")
        fun later(): Guard = Guard("later")
        fun main(): Unit {
            var cache = mutableMapOf<Int, String>()
            for ((key, text) in arrayOf(Entry(1, "one"), Entry(2, "two"))) {
                val first = earlier()
                val second = later()
                cache.put(key, text.clone())
                if (key == 1) { continue }
                cache.put(9, text.clone())
            }
            cache.contains(9)
        }
    "#;
    let (program, analysis) = super::super::lower_map_unit(text, "main");
    let origins = super::super::unit_iteration::assert_facts(
        &analysis,
        super::super::unit_iteration::Control::Continue,
        true,
    );
    let continue_span = origins.control;
    crate::ssa::verify::verify_program(&program).unwrap();
    assert_continue_structure(
        &program,
        continue_span,
        origins.statement,
        origins.source,
        true,
    );

    let function = function_at_continue(&program, continue_span);
    let local = |name| {
        let callee = program
            .modules
            .iter()
            .flat_map(|module| &module.functions)
            .filter(|function| {
                let declaration = analysis
                    .declaration(name, lang_frontend::name_resolution::SymbolKind::Function);
                function.origin == Origin::Source(analysis.declaration_span(declaration))
            })
            .collect::<Vec<_>>();
        assert_eq!(callee.len(), 1, "one real source callee for {name}");
        let callee = callee[0];
        only(function, |i| {
            matches!(i.operation, Operation::DirectCall { callee: id, .. }
            if id == callee.id)
        })
    };
    let first = local("earlier");
    let second = local("later");
    before(function, first, second);
    let element = only(function, |i| {
        matches!(i.operation, Operation::ContainerElementPlace { .. })
    });
    let element_borrow = only(function, |i| {
        matches!(i.operation, Operation::BorrowBegin { place, kind: LoanKind::Shared }
            if EntityId::Place(place) == element.results[0])
    });
    let element_loan = element_borrow.results[0];
    let component = |expected_field| {
        only(function, |i| {
            matches!(i.operation, Operation::SharedFieldLoan { base, field }
            if EntityId::Loan(base) == element_loan && field == expected_field)
        })
        .results[0]
    };
    let key_loan = component(0);
    let text_loan = component(1);
    let source_length = only(function, |i| {
        matches!(i.operation, Operation::ContainerLength { .. })
    });
    let preheader = function.block(source_length.block).unwrap();
    let header = branch(preheader).target;
    let backedges = edges(function)
        .into_iter()
        .filter(|(block, edge)| edge.target == header && *block != preheader.id)
        .collect::<Vec<_>>();
    assert_eq!(backedges.len(), 2);
    for (block, _) in backedges {
        let drop_local = |expected| {
            only(function, |i| {
                i.block == block
                    && matches!(i.operation, Operation::Drop { owner }
                if forwards(function, EntityId::Value(owner), expected))
            })
        };
        let second_drop = drop_local(second.results[0]);
        let first_drop = drop_local(first.results[0]);
        let text_end = end_in(function, block, text_loan);
        let key_end = end_in(function, block, key_loan);
        let element_end = end_in(function, block, element_loan);
        before(function, second_drop, first_drop);
        before(function, first_drop, text_end);
        before(function, text_end, key_end);
        before(function, key_end, element_end);
    }
    for local in [first.results[0], second.results[0]] {
        assert_eq!(
            function
                .instructions
                .iter()
                .filter(|i| {
                    matches!(i.operation, Operation::Drop { owner }
                if forwards(function, EntityId::Value(owner), local))
                })
                .count(),
            2,
            "one local drop on each mutually exclusive iteration exit"
        );
    }
    for component in [text_loan, key_loan] {
        assert_eq!(
            function
                .instructions
                .iter()
                .filter(|i| {
                    matches!(i.operation, Operation::BorrowEnd { loan }
                if forwards(function, EntityId::Loan(loan), component))
                })
                .count(),
            2,
            "derived element loans end on each actual backedge"
        );
    }
}
