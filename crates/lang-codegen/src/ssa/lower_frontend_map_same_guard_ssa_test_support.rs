//! Six exact SSA blocks and directed edge slots for the original same-guard source.
use std::collections::BTreeSet;

use crate::{
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::model::{
        BinaryOperator, BlockId, ComparisonOperator, Definition, Edge, EntityId, EntityType,
        Function, Instruction, LoanKind, Operation, Origin, PlaceAccess, ScalarConstant,
        TerminatorKind,
    },
};
use lang_frontend::type_checking::UnitExpressionId;

use super::super::super::structure::{before, branch, definition, edges, only, slot};
use super::{Source, one, span, symbol_span};

// Follow only this directed edge and its unique source slot. Updates are new owners,
// never aliases, and Header's seed/backedge are checked independently.
fn transport(function: &Function, edge: &Edge, entity: EntityId) -> EntityId {
    let target = function.block(edge.target).unwrap();
    assert_eq!(edge.arguments.len(), target.parameters.len());
    let index = slot(&edge.arguments, entity);
    let result = target.parameters[index];
    assert_eq!(
        function.entity(result).unwrap().definition,
        Definition::BlockParameter {
            block: target.id,
            index
        }
    );
    assert_eq!(
        function.entity(entity).unwrap().ty,
        function.entity(result).unwrap().ty
    );
    result
}

fn at<'a>(
    unit: &UnitSource,
    function: &'a Function,
    expression: UnitExpressionId,
    predicate: impl Fn(&Operation) -> bool,
) -> &'a Instruction {
    only(function, |instruction| {
        instruction.origin == Origin::Source(span(unit, expression))
            && predicate(&instruction.operation)
    })
}

fn read<'a>(
    unit: &UnitSource,
    function: &'a Function,
    expression: UnitExpressionId,
    loan: EntityId,
) -> &'a Instruction {
    let EntityId::Loan(loan) = loan else {
        panic!("a Read consumes the Shared capability, not an earlier copied Value");
    };
    let instruction = at(unit, function, expression, |operation| {
        *operation
            == Operation::Read {
                source: PlaceAccess::Loan(loan),
            }
    });
    let EntityId::Value(result) = *one(&instruction.results) else {
        panic!("Read produces a copied Int Value");
    };
    assert_eq!(
        function.entity(EntityId::Value(result)).unwrap().definition,
        Definition::InstructionResult {
            instruction: instruction.id,
            index: 0
        }
    );
    assert_eq!(
        function
            .entity(EntityId::Value(result))
            .unwrap()
            .ty
            .semantic_type(),
        function
            .entity(EntityId::Loan(loan))
            .unwrap()
            .ty
            .semantic_type()
    );
    instruction
}

fn drop(function: &Function, entity: EntityId) -> &Instruction {
    only(
        function,
        |instruction| matches!(instruction.operation, Operation::Drop { owner } if EntityId::Value(owner) == entity),
    )
}

fn end(function: &Function, entity: EntityId) -> &Instruction {
    only(
        function,
        |instruction| matches!(instruction.operation, Operation::BorrowEnd { loan } if EntityId::Loan(loan) == entity),
    )
}

fn uses(
    function: &Function,
    entity: EntityId,
    operations: &[&Instruction],
    expected_edges: &[(BlockId, BlockId)],
) {
    let actual = function
        .instructions
        .iter()
        .filter(|instruction| instruction.operation.entities().contains(&entity))
        .map(|instruction| instruction.id)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual,
        operations
            .iter()
            .map(|instruction| instruction.id)
            .collect()
    );
    let actual = edges(function)
        .into_iter()
        .filter(|(_, edge)| edge.arguments.contains(&entity))
        .map(|(source, edge)| (source, edge.target))
        .collect::<BTreeSet<_>>();
    assert_eq!(actual, expected_edges.iter().copied().collect());
}

pub(super) fn assert_ssa(unit: &UnitSource, source: &Source, function: &Function) {
    let seed = at(unit, function, source.construction, |operation| {
        matches!(operation, Operation::MapConstruct { .. })
    });
    let source_owner = at(unit, function, source.provider, |operation| {
        matches!(operation, Operation::ContainerConstruct { .. })
    });
    let length = only(function, |instruction| {
        matches!(instruction.operation, Operation::ContainerLength { .. })
    });
    let Operation::ContainerLength { owner: source_loan } = length.operation else {
        unreachable!();
    };
    let source_borrow = definition(function, source_loan);
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = source_borrow.operation
    else {
        panic!("the provider borrows its actual hidden Array owner");
    };
    let source_root = definition(function, EntityId::Place(place));
    assert!(
        matches!(source_root.operation, Operation::RootPlace { owner }
        if source_owner.results.as_slice() == [EntityId::Value(owner)])
    );
    let preheader = function.block(function.entry_block().unwrap()).unwrap();
    assert_eq!(preheader.id, length.block);
    for instruction in [seed, source_owner, source_root, source_borrow, length] {
        assert_eq!(instruction.block, preheader.id);
    }
    for pair in [source_owner, source_root, source_borrow, length].windows(2) {
        before(function, pair[0], pair[1]);
    }
    let stop = *one(&preheader.parameters);
    assert!(matches!(stop, EntityId::Loan(_)));
    assert_eq!(
        function.entity(stop).unwrap().definition,
        Definition::BlockParameter {
            block: preheader.id,
            index: 0
        }
    );
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target: int,
    } = function.entity(stop).unwrap().ty
    else {
        panic!("the original Borrow stop parameter is a Shared Int Loan");
    };
    assert_eq!(function.receiver(), None);
    let enter = branch(preheader);
    let header = function.block(enter.target).unwrap();
    let h_map = transport(function, enter, seed.results[0]);
    let h_owner = transport(function, enter, source_owner.results[0]);
    let h_loan = transport(function, enter, source_loan);
    let h_length = transport(function, enter, length.results[0]);
    let h_stop = transport(function, enter, stop);
    let TerminatorKind::Conditional {
        condition,
        when_true: body_edge,
        when_false: exit_edge,
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("the real cursor guard selects iteration versus exhaustion");
    };
    let guard = definition(function, EntityId::Value(*condition));
    let Operation::Compare {
        operator: ComparisonOperator::LessThan,
        left: cursor,
        right,
    } = guard.operation
    else {
        panic!("Header compares this cursor against the entry length snapshot");
    };
    assert_eq!(guard.block, header.id);
    assert_eq!(EntityId::Value(right), h_length);
    let cursor = EntityId::Value(cursor);
    let cursor_slot = slot(&header.parameters, cursor);
    assert_eq!(
        transport(function, enter, enter.arguments[cursor_slot]),
        cursor
    );
    assert_eq!(
        definition(function, enter.arguments[cursor_slot]).operation,
        Operation::Constant(ScalarConstant::Integer(0))
    );
    let body = function.block(body_edge.target).unwrap();
    let exhausted = function.block(exit_edge.target).unwrap();
    let b_map = transport(function, body_edge, h_map);
    let b_owner = transport(function, body_edge, h_owner);
    let b_loan = transport(function, body_edge, h_loan);
    let b_cursor = transport(function, body_edge, cursor);
    let b_length = transport(function, body_edge, h_length);
    let b_stop = transport(function, body_edge, h_stop);
    let e_map = transport(function, exit_edge, h_map);
    let e_owner = transport(function, exit_edge, h_owner);
    let e_loan = transport(function, exit_edge, h_loan);
    let e_stop = transport(function, exit_edge, h_stop);
    let element = only(function, |instruction| {
        matches!(instruction.operation, Operation::ContainerElementPlace { owner, index }
            if owner == b_loan && EntityId::Value(index) == b_cursor)
    });
    assert_eq!(element.block, body.id);
    assert!(matches!(one(&element.results), EntityId::Place(_)));
    let element_borrow = only(function, |instruction| {
        matches!(instruction.operation, Operation::BorrowBegin { place, kind: LoanKind::Shared }
            if EntityId::Place(place) == element.results[0])
    });
    before(function, element, element_borrow);
    let item = *one(&element_borrow.results);
    assert!(matches!(item, EntityId::Loan(_)));
    assert_ne!(
        item, b_stop,
        "same target type cannot identify different source loans"
    );
    assert_ne!(item, b_loan);
    let head = at(unit, function, source.head, |operation| {
        matches!(operation, Operation::MapPut { .. })
    });
    let tail = at(unit, function, source.tail, |operation| {
        matches!(operation, Operation::MapPut { .. })
    });
    assert_eq!(head.block, body.id);
    assert!(matches!(head.operation, Operation::MapPut { owner, .. }
        if EntityId::Value(owner) == b_map));
    for (expression, instruction) in [(source.head, head), (source.tail, tail)] {
        let descriptor = unit.types().map_put(expression).unwrap();
        let key = if expression == source.head {
            read(unit, function, descriptor.key(), item)
        } else {
            at(unit, function, descriptor.key(), |operation| {
                *operation == Operation::Constant(ScalarConstant::Integer(9))
            })
        };
        let value = at(unit, function, descriptor.value(), |operation| {
            matches!(operation, Operation::StringClone { .. })
        });
        let Operation::MapPut {
            key: actual_key,
            value: actual_value,
            ..
        } = instruction.operation
        else {
            unreachable!();
        };
        assert_eq!(key.results.as_slice(), [EntityId::Value(actual_key)]);
        assert_eq!(value.results.as_slice(), [EntityId::Value(actual_value)]);
        before(function, key, value);
        before(function, value, instruction);
    }
    let item_read = read(unit, function, source.left, item);
    let stop_read = read(unit, function, source.right, b_stop);
    let compare = at(unit, function, source.condition, |operation| {
        matches!(operation, Operation::Compare { operator: ComparisonOperator::Equal, left, right }
            if EntityId::Value(*left) == item_read.results[0]
                && EntityId::Value(*right) == stop_read.results[0])
    });
    before(function, head, item_read);
    before(function, item_read, stop_read);
    before(function, stop_read, compare);
    let terminator = body.terminator.as_ref().unwrap();
    assert_eq!(
        terminator.origin,
        Origin::Source(span(unit, source.conditional))
    );
    let TerminatorKind::Conditional {
        condition,
        when_true: return_edge,
        when_false: tail_edge,
    } = &terminator.kind
    else {
        panic!("one real complementary branch choice, not independent reachability");
    };
    assert_eq!(compare.results.as_slice(), [EntityId::Value(*condition)]);
    let returned = function.block(return_edge.target).unwrap();
    let fallthrough = function.block(tail_edge.target).unwrap();
    let backedge = branch(fallthrough);
    assert_eq!(backedge.target, header.id);
    let blocks = [
        preheader.id,
        header.id,
        body.id,
        returned.id,
        fallthrough.id,
        exhausted.id,
    ];
    assert_eq!(blocks.into_iter().collect::<BTreeSet<_>>().len(), 6);
    assert_eq!(
        function
            .blocks
            .iter()
            .map(|block| block.id)
            .collect::<BTreeSet<_>>(),
        blocks.into_iter().collect()
    );
    let expected_edges = [
        (preheader.id, header.id),
        (header.id, body.id),
        (header.id, exhausted.id),
        (body.id, returned.id),
        (body.id, fallthrough.id),
        (fallthrough.id, header.id),
    ];
    let all_edges = edges(function);
    assert_eq!(all_edges.len(), expected_edges.len());
    assert_eq!(
        all_edges
            .iter()
            .map(|(from, edge)| (*from, edge.target))
            .collect::<BTreeSet<_>>(),
        expected_edges.into_iter().collect()
    );
    let r_map = transport(function, return_edge, head.results[0]);
    let f_map = transport(function, tail_edge, head.results[0]);
    assert_eq!(tail.block, fallthrough.id);
    assert!(matches!(tail.operation, Operation::MapPut { owner, .. }
        if EntityId::Value(owner) == f_map));
    assert_eq!(transport(function, backedge, tail.results[0]), h_map);
    assert_eq!(
        slot(&enter.arguments, seed.results[0]),
        slot(&backedge.arguments, tail.results[0])
    );
    let r_owner = transport(function, return_edge, b_owner);
    let r_loan = transport(function, return_edge, b_loan);
    let r_item = transport(function, return_edge, item);
    let r_stop = transport(function, return_edge, b_stop);
    let f_owner = transport(function, tail_edge, b_owner);
    let f_loan = transport(function, tail_edge, b_loan);
    let f_item = transport(function, tail_edge, item);
    let f_stop = transport(function, tail_edge, b_stop);
    let f_cursor = transport(function, tail_edge, b_cursor);
    let f_length = transport(function, tail_edge, b_length);
    for (actual, expected) in [
        (f_owner, h_owner),
        (f_loan, h_loan),
        (f_stop, h_stop),
        (f_length, h_length),
    ] {
        assert_eq!(transport(function, backedge, actual), expected);
    }
    for loan in [
        stop, h_stop, b_stop, e_stop, r_stop, f_stop, item, r_item, f_item,
    ] {
        assert_eq!(
            function.entity(loan).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                target: int
            }
        );
    }
    let advance = only(function, |instruction| {
        matches!(
            instruction.operation,
            Operation::Binary {
                operator: BinaryOperator::Add,
                ..
            }
        )
    });
    assert_eq!(advance.block, fallthrough.id);
    assert_eq!(
        advance.results.as_slice(),
        [backedge.arguments[cursor_slot]]
    );
    let Operation::Binary { left, right, .. } = advance.operation else {
        unreachable!();
    };
    assert_eq!(EntityId::Value(left), f_cursor);
    let step = definition(function, EntityId::Value(right));
    assert_eq!(step.block, preheader.id);
    assert_eq!(
        step.operation,
        Operation::Constant(ScalarConstant::Integer(1))
    );

    let query = at(unit, function, source.query, |operation| {
        matches!(operation, Operation::MapContains { owner, key }
            if *owner == r_map && *key == r_item)
    });
    let tail_query = at(
        unit,
        function,
        source.tail_query,
        |operation| matches!(operation, Operation::MapContains { owner, .. } if *owner == e_map),
    );
    assert_eq!(query.block, returned.id);
    assert_eq!(tail_query.block, exhausted.id);
    let tail_key = unit.types().map_contains(source.tail_query).unwrap().key();
    let key = at(unit, function, tail_key, |operation| {
        *operation == Operation::Constant(ScalarConstant::Integer(9))
    });
    assert!(
        matches!(tail_query.operation, Operation::MapContains { key: actual, .. }
        if key.results.as_slice() == [actual])
    );
    before(function, key, tail_query);
    let return_read = read(unit, function, source.operand, r_item);
    let r_map_drop = drop(function, r_map);
    let e_map_drop = drop(function, e_map);
    for instruction in [r_map_drop, e_map_drop] {
        assert_eq!(
            instruction.origin,
            Origin::Source(symbol_span(unit, source.cache))
        );
    }
    let r_element_end = end(function, r_item);
    let r_source_end = end(function, r_loan);
    let r_source_drop = drop(function, r_owner);
    let f_element_end = end(function, f_item);
    let e_source_end = end(function, e_loan);
    let e_source_drop = drop(function, e_owner);
    for instruction in [r_element_end, f_element_end, r_source_end, e_source_end] {
        assert_eq!(instruction.origin, Origin::Source(source.loop_span));
    }
    for instruction in [r_source_drop, e_source_drop] {
        assert_eq!(
            instruction.origin,
            Origin::Source(span(unit, source.provider))
        );
    }
    for pair in [
        query,
        r_map_drop,
        return_read,
        r_element_end,
        r_source_end,
        r_source_drop,
    ]
    .windows(2)
    {
        before(function, pair[0], pair[1]);
    }
    for pair in [tail, f_element_end, advance].windows(2) {
        before(function, pair[0], pair[1]);
    }
    for pair in [e_source_end, e_source_drop, tail_query, e_map_drop].windows(2) {
        before(function, pair[0], pair[1]);
    }
    for (block, expression) in [(returned, source.returned), (exhausted, source.last_return)] {
        let terminator = block.terminator.as_ref().unwrap();
        assert_eq!(terminator.origin, Origin::Source(span(unit, expression)));
        let TerminatorKind::Return { values } = &terminator.kind else {
            panic!("original terminal Return has no outgoing transport");
        };
        let value = EntityId::Value(*one(values));
        assert!(all_edges.iter().all(|(from, _)| *from != block.id));
        if block.id == returned.id {
            assert_eq!(return_read.results.as_slice(), [value]);
        } else {
            let zero = definition(function, value);
            assert_eq!(zero.block, exhausted.id);
            assert_eq!(zero.origin, Origin::Source(span(unit, source.last_operand)));
            assert_eq!(
                zero.operation,
                Operation::Constant(ScalarConstant::Integer(0))
            );
        }
    }
    // Exact-use inventories close presence and version obligations: entry always
    // brings m0, the only backedge brings unconsumed m2, and Return never feeds H.
    uses(function, seed.results[0], &[], &[(preheader.id, header.id)]);
    uses(
        function,
        h_map,
        &[],
        &[(header.id, body.id), (header.id, exhausted.id)],
    );
    uses(function, b_map, &[head], &[]);
    uses(
        function,
        head.results[0],
        &[],
        &[(body.id, returned.id), (body.id, fallthrough.id)],
    );
    uses(function, r_map, &[query, r_map_drop], &[]);
    uses(function, f_map, &[tail], &[]);
    uses(
        function,
        tail.results[0],
        &[],
        &[(fallthrough.id, header.id)],
    );
    uses(function, e_map, &[tail_query, e_map_drop], &[]);
    let versions = [
        seed.results[0],
        h_map,
        b_map,
        head.results[0],
        r_map,
        f_map,
        tail.results[0],
        e_map,
    ];
    assert_eq!(
        versions.into_iter().collect::<BTreeSet<_>>().len(),
        versions.len()
    );
    let map_type = function.entity(seed.results[0]).unwrap().ty;
    for entity in versions {
        assert_eq!(function.entity(entity).unwrap().ty, map_type);
        assert_ne!(entity, source_owner.results[0]);
    }
    let map_drops = function
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(instruction.operation, Operation::Drop { owner }
            if function.entity(EntityId::Value(owner)).unwrap().ty == map_type)
        })
        .map(|instruction| instruction.id)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        map_drops,
        [r_map_drop.id, e_map_drop.id].into_iter().collect()
    );
    let source_type = function.entity(source_owner.results[0]).unwrap().ty;
    let source_drops = function
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(instruction.operation, Operation::Drop { owner }
            if function.entity(EntityId::Value(owner)).unwrap().ty == source_type)
        })
        .map(|instruction| instruction.id)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        source_drops,
        [r_source_drop.id, e_source_drop.id].into_iter().collect()
    );
    let map_operations = function
        .instructions
        .iter()
        .filter(|instruction| {
            matches!(
                instruction.operation,
                Operation::MapConstruct { .. }
                    | Operation::MapPut { .. }
                    | Operation::MapContains { .. }
                    | Operation::MapRemove { .. }
            )
        })
        .map(|instruction| instruction.id)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        map_operations,
        [seed.id, head.id, tail.id, query.id, tail_query.id]
            .into_iter()
            .collect()
    );
    let provider_ends = function
        .instructions
        .iter()
        .filter(|instruction| {
            instruction.origin == Origin::Source(source.loop_span)
                && matches!(instruction.operation, Operation::BorrowEnd { .. })
        })
        .map(|instruction| instruction.id)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        provider_ends,
        [
            r_element_end.id,
            f_element_end.id,
            r_source_end.id,
            e_source_end.id
        ]
        .into_iter()
        .collect()
    );
}
