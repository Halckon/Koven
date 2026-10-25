//! Real terminal item-return SSA; frontend identity remains in the parent helper.
use crate::{
    native_tests::boxed_enum_tests::unit_source::UnitSource,
    ssa::model::{
        BlockId, EntityId, Function, LoanKind, Operation, Origin, Program, TerminatorKind,
    },
};
use lang_frontend::name_resolution::UnitSymbolId;

use super::super::super::structure::{
    before, branch, definition, edges, forwards, only, reaches, slot, source_function, transport,
};
use super::{LoopIdentity, expression_span, one, symbol_span};

fn all_paths_return(
    function: &Function,
    from: BlockId,
    returned: BlockId,
    path: &mut Vec<BlockId>,
) -> bool {
    if from == returned {
        return true;
    }
    if path.contains(&from) {
        return false;
    }
    path.push(from);
    let targets = function
        .block(from)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
        .kind
        .targets();
    let result = !targets.is_empty()
        && targets
            .into_iter()
            .all(|target| all_paths_return(function, target, returned, path));
    path.pop();
    result
}

// The existing operand oracle deliberately returns zero for `return item`.
// Independently prove its real terminal path, latest Map owner and provider transport.
pub(super) fn check(
    unit: &UnitSource,
    program: &Program,
    identity: &LoopIdentity,
    cache: UnitSymbolId,
) {
    let (_, function) = source_function(program, identity.scope);
    let return_span = expression_span(unit, identity.returned);
    let returns = function
        .blocks
        .iter()
        .filter(|block| block.terminator.as_ref().unwrap().origin == Origin::Source(return_span))
        .collect::<Vec<_>>();
    let returned = *one(&returns, "actual item Return terminator");
    let TerminatorKind::Return { values } = &returned.terminator.as_ref().unwrap().kind else {
        panic!("the source Return must terminate rather than feed a successor");
    };
    assert_eq!(values.len(), 1);
    let all_edges = edges(function);
    assert!(all_edges.iter().all(|(from, _)| *from != returned.id));
    let seed = only(function, |instruction| {
        matches!(instruction.operation, Operation::MapConstruct { .. })
    });
    let length = only(function, |instruction| {
        matches!(instruction.operation, Operation::ContainerLength { .. })
    });
    let Operation::ContainerLength { owner: source_loan } = length.operation else {
        unreachable!()
    };
    let borrow = definition(function, source_loan);
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = borrow.operation
    else {
        panic!("the Array provider has its real Shared source loan");
    };
    let root = definition(function, EntityId::Place(place));
    let Operation::RootPlace { owner } = root.operation else {
        panic!("the source loan has its exact hidden Array owner");
    };
    let source_owner = EntityId::Value(owner);
    let source = definition(function, source_owner);
    assert!(matches!(
        source.operation,
        Operation::ContainerConstruct { .. }
    ));
    assert_eq!(
        source.origin,
        Origin::Source(expression_span(unit, identity.provider))
    );
    assert_ne!(source_owner, seed.results[0]);
    before(function, source, root);
    before(function, root, borrow);
    before(function, borrow, length);
    let preheader = function.block(length.block).unwrap();
    let enter = branch(preheader);
    let header = function.block(enter.target).unwrap();
    let seeds = enter
        .arguments
        .iter()
        .copied()
        .filter(|value| forwards(function, *value, seed.results[0]))
        .collect::<Vec<_>>();
    let map_slot = slot(&enter.arguments, *one(&seeds, "entry Map owner"));
    let source_slot = slot(&enter.arguments, source_owner);
    let loan_slot = slot(&enter.arguments, source_loan);
    let length_slot = slot(&enter.arguments, length.results[0]);
    let [header_map, header_source, header_loan, header_length] =
        [map_slot, source_slot, loan_slot, length_slot].map(|index| header.parameters[index]);
    let TerminatorKind::Conditional {
        when_true,
        when_false,
        ..
    } = &header.terminator.as_ref().unwrap().kind
    else {
        panic!("the real provider header branches to body or exhaustion");
    };
    let body_map = transport(function, when_true, header_map);
    let body_source = transport(function, when_true, header_source);
    let body_loan = transport(function, when_true, header_loan);
    let exhausted_map = transport(function, when_false, header_map);
    let exhausted_source = transport(function, when_false, header_source);
    let exhausted_loan = transport(function, when_false, header_loan);
    let updates = function
        .instructions
        .iter()
        .filter(|instruction| matches!(instruction.operation, Operation::MapPut { .. }))
        .collect::<Vec<_>>();
    assert_eq!(updates.len(), 2);
    let heads = updates
        .iter()
        .copied()
        .filter(|instruction| instruction.origin.span().end() < return_span.start())
        .collect::<Vec<_>>();
    let tails = updates
        .iter()
        .copied()
        .filter(|instruction| instruction.origin.span().start() > return_span.end())
        .collect::<Vec<_>>();
    let head = *one(&heads, "head Map update");
    let tail = *one(&tails, "tail Map update");
    assert!(matches!(
        head.operation,
        Operation::MapPut { owner, .. }
            if forwards(function, EntityId::Value(owner), body_map)
    ));
    assert!(matches!(
        tail.operation,
        Operation::MapPut { owner, .. }
            if forwards(function, EntityId::Value(owner), head.results[0])
    ));
    assert_ne!(head.results[0], tail.results[0]);
    let head_terminator = &function
        .block(head.block)
        .unwrap()
        .terminator
        .as_ref()
        .unwrap()
        .kind;
    let TerminatorKind::Conditional {
        when_true: selected,
        when_false: fallthrough,
        ..
    } = head_terminator
    else {
        panic!("the original item == stop selects terminal Return or Fallthrough");
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
        selected.target,
        when_false.target,
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
    assert_eq!(
        incoming.len(),
        2,
        "Header accepts only Entry and Fallthrough, never Return"
    );
    let backedges = incoming
        .iter()
        .copied()
        .filter(|(from, _)| *from != preheader.id)
        .collect::<Vec<_>>();
    let (back_block, back) = **one(&backedges, "Fallthrough backedge");
    assert_eq!(back_block, tail.block);
    for (index, expected) in [
        (map_slot, tail.results[0]),
        (source_slot, body_source),
        (loan_slot, body_loan),
        (length_slot, header_length),
    ] {
        assert!(forwards(function, back.arguments[index], expected));
    }
    assert!(!forwards(
        function,
        back.arguments[map_slot],
        head.results[0]
    ));
    let map_drop = only(function, |instruction| {
        instruction.block == returned.id
            && matches!(
                instruction.operation,
                Operation::Drop { owner }
                    if forwards(function, EntityId::Value(owner), head.results[0])
            )
    });
    assert_eq!(map_drop.origin, Origin::Source(symbol_span(unit, cache)));
    let element = only(function, |instruction| {
        matches!(
            instruction.operation,
            Operation::ContainerElementPlace { owner, .. } if owner == body_loan
        )
    });
    let element_borrow = only(function, |instruction| {
        matches!(
            instruction.operation,
            Operation::BorrowBegin { place, kind: LoanKind::Shared }
                if EntityId::Place(place) == element.results[0]
        )
    });
    let element_end = only(function, |instruction| {
        instruction.block == returned.id
            && matches!(
                instruction.operation,
                Operation::BorrowEnd { loan }
                    if forwards(function, EntityId::Loan(loan), element_borrow.results[0])
            )
    });
    let source_end = only(function, |instruction| {
        instruction.block == returned.id
            && matches!(
                instruction.operation,
                Operation::BorrowEnd { loan }
                    if forwards(function, EntityId::Loan(loan), body_loan)
            )
    });
    let source_drop = only(function, |instruction| {
        instruction.block == returned.id
            && matches!(
                instruction.operation,
                Operation::Drop { owner }
                    if forwards(function, EntityId::Value(owner), body_source)
            )
    });
    before(function, element_end, source_end);
    before(function, source_end, source_drop);
    before(function, source_drop, map_drop);
    let normal_source_end = only(function, |instruction| {
        instruction.block == when_false.target
            && matches!(
                instruction.operation,
                Operation::BorrowEnd { loan }
                    if forwards(function, EntityId::Loan(loan), exhausted_loan)
            )
    });
    let normal_source_drop = only(function, |instruction| {
        instruction.block == when_false.target
            && matches!(
                instruction.operation,
                Operation::Drop { owner }
                    if forwards(function, EntityId::Value(owner), exhausted_source)
            )
    });
    let normal_map_drop = only(function, |instruction| {
        matches!(
            instruction.operation,
            Operation::Drop { owner }
                if forwards(function, EntityId::Value(owner), exhausted_map)
        )
    });
    assert_eq!(
        normal_map_drop.origin,
        Origin::Source(symbol_span(unit, cache))
    );
    before(function, normal_source_end, normal_source_drop);
    before(function, normal_source_drop, normal_map_drop);
    for value in values {
        assert!(!function.instructions.iter().any(|instruction| {
            matches!(instruction.operation, Operation::Drop { owner } if owner == *value)
        }));
    }
}
