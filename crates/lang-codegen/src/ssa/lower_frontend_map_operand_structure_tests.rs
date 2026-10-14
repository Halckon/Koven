//! Independent assertions over original ordinary-source SSA, never edited IR.
use super::structure::{
    before, branch, definition, edges, forwards, only, reaches, slot, transport,
};
use crate::ssa::model::{
    BlockId, EntityId, Function, LoanKind, Operation, Origin, Program, TerminatorKind,
};
use lang_frontend::{
    parser::{Expression, ParsedFile},
    source::Span,
};

fn all_paths_return(
    function: &Function,
    start: BlockId,
    target: BlockId,
    path: &mut Vec<BlockId>,
) -> bool {
    if start == target {
        return true;
    }
    if path.contains(&start) {
        return false;
    }
    path.push(start);
    let successors = function
        .block(start)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
        .kind
        .targets();
    let valid = !successors.is_empty()
        && successors
            .into_iter()
            .all(|next| all_paths_return(function, next, target, path));
    path.pop();
    valid
}

// The parsed Return operand provides an exact location; the chosen intrinsic instruction
// and actual terminator must independently carry those same source origins.
pub(super) fn assert_source(program: &Program, parsed: &ParsedFile) -> usize {
    let mut checked = 0;
    for (_, node) in parsed.ast().expressions().iter() {
        let Expression::Return {
            value: Some(value), ..
        } = node.payload()
        else {
            continue;
        };
        let Expression::If { condition, .. } =
            parsed.ast().expressions().get(*value).unwrap().payload()
        else {
            continue;
        };
        let condition = parsed.ast().expressions().get(*condition).unwrap();
        let call = if let Expression::Binary { left, .. } = condition.payload() {
            parsed.ast().expressions().get(*left).unwrap()
        } else {
            condition
        };
        let functions = program
            .modules
            .iter()
            .flat_map(|m| &m.functions)
            .filter(|f| {
                f.instructions.iter().any(|i| {
                    i.origin == Origin::Source(call.span())
                        && matches!(
                            i.operation,
                            Operation::MapRemove { .. } | Operation::MapContains { .. }
                        )
                })
            })
            .collect::<Vec<_>>();
        if functions.is_empty() {
            continue;
        }
        assert_eq!(functions.len(), 1);
        check(functions[0], node.span(), call.span());
        checked += 1;
    }
    checked
}

fn check(function: &Function, return_span: Span, call_span: Span) {
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
        panic!("source shared loan")
    };
    let root = definition(function, EntityId::Place(place));
    let Operation::RootPlace {
        owner: source_owner,
    } = root.operation
    else {
        panic!("exact hidden owner")
    };
    let source_owner = EntityId::Value(source_owner);
    assert_ne!(seed.results[0], source_owner);
    let source = definition(function, source_owner);
    let preheader = function.block(length.block).unwrap();
    before(function, source, root);
    before(function, root, source_borrow);
    before(function, source_borrow, length);
    let enter = branch(preheader);
    let header = function.block(enter.target).unwrap();
    let seed_inputs = enter
        .arguments
        .iter()
        .copied()
        .filter(|value| forwards(function, *value, seed.results[0]))
        .collect::<Vec<_>>();
    assert_eq!(
        seed_inputs.len(),
        1,
        "constructor guards may forward the exact Map seed before provider entry"
    );
    let map_slot = slot(&enter.arguments, seed_inputs[0]);
    let source_slot = slot(&enter.arguments, source_owner);
    let loan_slot = slot(&enter.arguments, source_loan);
    let length_slot = slot(&enter.arguments, length.results[0]);
    let [header_map, header_source, header_loan, header_length] =
        [map_slot, source_slot, loan_slot, length_slot].map(|s| header.parameters[s]);
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("provider header")
    };
    let body = function.block(when_true.target).unwrap();
    let exhausted = function.block(when_false.target).unwrap();
    let body_map = transport(function, when_true, header_map);
    let body_source = transport(function, when_true, header_source);
    let body_loan = transport(function, when_true, header_loan);
    let exhausted_map = transport(function, when_false, header_map);
    let exhausted_source = transport(function, when_false, header_source);
    let exhausted_loan = transport(function, when_false, header_loan);
    let all_edges = edges(function);
    for target in [body.id, exhausted.id] {
        assert_eq!(
            all_edges
                .iter()
                .filter(|(_, edge)| edge.target == target)
                .map(|(from, _)| *from)
                .collect::<Vec<_>>(),
            [header.id]
        );
    }
    let updates = function
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::MapPut { .. }))
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
    assert!(
        matches!(head.operation, Operation::MapPut { owner, .. } if forwards(function, EntityId::Value(owner), body_map))
    );
    assert!(
        matches!(tail.operation, Operation::MapPut { owner, .. } if forwards(function, EntityId::Value(owner), head.results[0]))
    );
    let call = only(function, |i| {
        i.origin == Origin::Source(call_span)
            && matches!(
                i.operation,
                Operation::MapRemove { .. } | Operation::MapContains { .. }
            )
    });
    let (current, nullable) = match call.operation {
        Operation::MapRemove { owner, .. } => {
            assert!(forwards(function, EntityId::Value(owner), head.results[0]));
            assert_eq!(call.results.len(), 2);
            (call.results[0], Some(call.results[1]))
        }
        Operation::MapContains { owner, .. } => {
            assert!(forwards(function, owner, head.results[0]));
            (owner, None)
        }
        _ => unreachable!(),
    };
    assert_ne!(current, source_owner);
    let map_drop = only(
        function,
        |i| matches!(i.operation, Operation::Drop { owner } if EntityId::Value(owner) == current),
    );
    before(function, call, map_drop);
    if let Some(nullable) = nullable {
        assert_ne!(nullable, current);
        assert_ne!(nullable, source_owner);
        let presence = only(function, |i| {
            matches!(i.operation,
            Operation::AggregateProject { aggregate, field: 0 } | Operation::NullableIsNull { owner: aggregate } if EntityId::Value(aggregate) == nullable)
        });
        let nullable_drop = only(
            function,
            |i| matches!(i.operation, Operation::Drop { owner } if EntityId::Value(owner) == nullable),
        );
        before(function, map_drop, presence);
        before(function, presence, nullable_drop);
        assert_eq!(
            function
                .instructions
                .iter()
                .filter(|i| i.operation.entities().contains(&nullable))
                .count(),
            2,
            "presence read and one independent nullable drop"
        );
        assert_eq!(
            function
                .instructions
                .iter()
                .filter(|i| i.operation.entities().contains(&current))
                .count(),
            1,
            "updated Map result0 is consumed only by its drop"
        );
        assert!(
            all_edges
                .iter()
                .all(|(_, edge)| !edge.arguments.contains(&current)
                    && !edge.arguments.contains(&nullable))
        );
    }
    let returned = function
        .blocks
        .iter()
        .filter(|block| block.terminator.as_ref().unwrap().origin == Origin::Source(return_span))
        .collect::<Vec<_>>();
    assert_eq!(returned.len(), 1);
    let returned = returned[0];
    let TerminatorKind::Return { values } = &returned.terminator.as_ref().unwrap().kind else {
        panic!("actual Return origin")
    };
    assert_eq!(values.len(), 1);
    assert!(
        !values.iter().any(|value| current == EntityId::Value(*value)
            || Some(EntityId::Value(*value)) == nullable
            || source_owner == EntityId::Value(*value))
    );
    assert!(all_paths_return(
        function,
        map_drop.block,
        returned.id,
        &mut Vec::new()
    ));
    assert!(all_edges.iter().all(|(from, _)| *from != returned.id));
    let TerminatorKind::Conditional {
        when_true: selected,
        when_false: fallthrough,
        ..
    } = &function
        .block(head.block)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
        .kind
    else {
        panic!("outer Return selection")
    };
    assert!(all_paths_return(
        function,
        selected.target,
        returned.id,
        &mut Vec::new()
    ));
    assert!(!reaches(function, selected.target, tail.block, header.id));
    assert!(!reaches(
        function,
        fallthrough.target,
        map_drop.block,
        header.id
    ));
    assert!(!reaches(
        function,
        fallthrough.target,
        returned.id,
        header.id
    ));
    let incoming = all_edges
        .iter()
        .filter(|(_, edge)| edge.target == header.id)
        .collect::<Vec<_>>();
    assert_eq!(incoming.len(), 2, "only Entry and Fallthrough");
    let (back_block, back) = **incoming
        .iter()
        .find(|(block, _)| *block != preheader.id)
        .unwrap();
    for (slot, expected) in [
        (map_slot, tail.results[0]),
        (source_slot, body_source),
        (loan_slot, body_loan),
        (length_slot, header_length),
    ] {
        assert!(forwards(function, back.arguments[slot], expected));
    }
    assert!(!forwards(
        function,
        back.arguments[map_slot],
        head.results[0]
    ));
    let element = only(
        function,
        |i| matches!(i.operation, Operation::ContainerElementPlace { owner, .. } if owner == body_loan),
    );
    let element_borrow = only(
        function,
        |i| matches!(i.operation, Operation::BorrowBegin { place, kind: LoanKind::Shared } if EntityId::Place(place) == element.results[0]),
    );
    let element_end = only(function, |i| {
        i.block == returned.id
            && matches!(i.operation, Operation::BorrowEnd { loan } if forwards(function, EntityId::Loan(loan), element_borrow.results[0]))
    });
    let back_element_end = only(function, |i| {
        i.block == back_block
            && matches!(i.operation, Operation::BorrowEnd { loan } if forwards(function, EntityId::Loan(loan), element_borrow.results[0]))
    });
    before(function, tail, back_element_end);
    let source_end = only(function, |i| {
        i.block == returned.id
            && matches!(i.operation, Operation::BorrowEnd { loan } if forwards(function, EntityId::Loan(loan), body_loan))
    });
    let source_drop = only(function, |i| {
        i.block == returned.id
            && matches!(i.operation, Operation::Drop { owner } if forwards(function, EntityId::Value(owner), body_source))
    });
    before(function, element_end, source_end);
    before(function, source_end, source_drop);
    let exhausted_end = only(function, |i| {
        i.block == exhausted.id
            && matches!(i.operation, Operation::BorrowEnd { loan } if forwards(function, EntityId::Loan(loan), exhausted_loan))
    });
    let exhausted_drop = only(function, |i| {
        i.block == exhausted.id
            && matches!(i.operation, Operation::Drop { owner } if forwards(function, EntityId::Value(owner), exhausted_source))
    });
    before(function, exhausted_end, exhausted_drop);
    let normal_map_drop = only(
        function,
        |i| matches!(i.operation, Operation::Drop { owner } if forwards(function, EntityId::Value(owner), exhausted_map)),
    );
    before(function, exhausted_drop, normal_map_drop);
    let normal_queries = function
        .instructions
        .iter()
        .filter(|i| {
            i.origin.span().start() >= return_span.end()
                && matches!(i.operation, Operation::MapContains { .. })
        })
        .collect::<Vec<_>>();
    assert!(normal_queries.len() <= 1);
    if let Some(query) = normal_queries.first() {
        assert!(
            matches!(query.operation, Operation::MapContains { owner, .. } if forwards(function, owner, exhausted_map))
        );
        before(function, exhausted_drop, query);
        before(function, query, normal_map_drop);
    }
    let map_type = function.entity(seed.results[0]).unwrap().ty;
    assert_eq!(function.instructions.iter().filter(|i| matches!(i.operation, Operation::Drop { owner } if function.entity(EntityId::Value(owner)).unwrap().ty == map_type)).count(), 2, "both enumerated drops proved by exact owner above");
}
