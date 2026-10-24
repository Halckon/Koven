//! Real entry owner -> call loan -> scan parameter, and read's borrowed consumer slots.
use super::super::{
    fixture::{Failure, contains, failure, need},
    query_graph::{Graph, Uses, function, loan, result, value},
    query_inputs::one,
};
use super::{
    ast::Input,
    query_graph::{Roles, Witness, full_graph, ty},
    query_inputs::Inputs,
    query_tail,
};
use crate::ssa::model::*;
use lang_frontend::parser::Expression;
use std::collections::BTreeSet;

fn signature(
    function: &Function,
    expected: &[EntityType],
    returns: &[SsaTypeId],
) -> Result<(), Failure> {
    let entry = function
        .entry_block()
        .and_then(|b| function.block(b))
        .ok_or_else(|| failure("nested SSA callable entry", function.id))?;
    need(
        function.receiver.is_none()
            && function.borrow_return.is_none()
            && function.carrier_return.is_none()
            && function.return_types == returns
            && entry.origin == function.origin
            && entry.parameters.len() == expected.len()
            && entry
                .parameters
                .iter()
                .zip(expected)
                .enumerate()
                .all(|(index, (&p, ty))| {
                    function.entity(p).is_some_and(|e| {
                        e.ty == *ty
                            && e.origin == function.origin
                            && e.definition
                                == Definition::BlockParameter {
                                    block: entry.id,
                                    index,
                                }
                    })
                }),
        "nested SSA source callable actual parameter definitions/signature",
        function,
    )
}

fn consumer(read: &Function, f: &Input, inputs: &Inputs, string: SsaTypeId) -> Result<(), Failure> {
    let shared = EntityType::Loan {
        kind: LoanKind::Shared,
        target: string,
    };
    signature(read, &[shared, shared], &[])?;
    let g = Graph::new(read)?;
    full_graph(&g)?;
    let block = one(
        g.blocks.iter().copied(),
        "nested SSA actual read single body",
    )?;
    let params = &g.block(block)?.parameters;
    let calls = f
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, n)| {
            matches!(n.payload(), Expression::Call { .. })
                .then_some(n.span())
                .filter(|&s| contains(inputs.read, s))
        })
        .collect::<Vec<_>>();
    need(
        calls.len() == 2 && g.instructions.len() == 2 && params[0] != params[1],
        "nested SSA exact source read consumers",
        (&calls, read),
    )?;
    let mut calls = calls;
    calls.sort_by_key(|s| s.start());
    let mut uses = Uses::default();
    for (index, &param) in params.iter().enumerate() {
        uses.protect(param);
        let instruction = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.origin == Origin::Source(calls[index])),
            "nested SSA actual println source consumer",
        )?;
        let EntityId::Loan(actual) = param else {
            return Err(failure("nested SSA actual read Borrow slot", param));
        };
        need(
            instruction.operation == Operation::PrintString { value: actual }
                && instruction.results.is_empty()
                && g.position(instruction)? == index,
            "nested SSA consumer reads its own ordered Borrow parameter",
            instruction,
        )?;
        uses.permit(param, instruction);
    }
    need(
        matches!(&g.term(block)?.kind, TerminatorKind::Return { values } if values.is_empty())
            && g.term(block)?.origin == Origin::Source(inputs.read),
        "nested SSA read Unit return no borrowed escape",
        g.term(block)?,
    )?;
    uses.check(&g, None)
}

pub(super) fn check(
    module: &Module,
    map_type: SsaTypeId,
    f: &Input,
    inputs: &Inputs,
) -> Result<(), Failure> {
    let entry = function(module, inputs.entry)?;
    let scan = function(module, inputs.scan)?;
    let read = function(module, inputs.read)?;
    let factory = function(module, inputs.factory)?;
    let (_, string, val) = module
        .map_container(map_type)
        .ok_or_else(|| failure("nested SSA caller map type", map_type))?;
    need(
        string == val,
        "nested SSA exact fixture String K/V",
        map_type,
    )?;
    let shared = EntityType::Loan {
        kind: LoanKind::Shared,
        target: map_type,
    };
    signature(entry, &[], &[])?;
    signature(scan, &[shared], &[])?;
    signature(factory, &[], &[map_type])?;
    consumer(read, f, inputs, string)?;
    let g = Graph::new(entry)?;
    full_graph(&g)?;
    let mut witness = Witness::default();
    let mut protected = BTreeSet::new();
    let mut calls = BTreeSet::new();
    let mut tail = entry
        .entry_block()
        .ok_or_else(|| failure("nested SSA source entry", entry.id))?;
    g.incoming_from(tail, &[])?;
    for (index, anchor) in inputs.callers.iter().enumerate() {
        let birth = one(g.instructions.iter().copied().filter(|i| i.origin == Origin::Source(anchor.initializer)
            && if index == 0 { matches!(&i.operation, Operation::DirectCall { callee, receiver: None, arguments }
                if *callee == factory.id && arguments.is_empty()) } else { i.operation == Operation::MapConstruct { map_type } }),
            "nested SSA actual distinct caller owner initializer")?;
        let owner = value(birth)?;
        need(
            birth.block == tail && ty(&g, result(birth)?)? == EntityType::Value(map_type),
            "nested SSA actual owning caller result in source-order continuation",
            birth,
        )?;
        if index == 0 {
            calls.insert(birth.id);
        }
        let root = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::RootPlace { owner }),
            "nested SSA actual caller owner unique RootPlace",
        )?;
        let EntityId::Place(place) = result(root)? else {
            return Err(failure("nested SSA actual caller Place", root));
        };
        let begin = one(
            g.instructions.iter().copied().filter(|i| {
                i.operation
                    == Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    }
            }),
            "nested SSA actual caller Shared loan",
        )?;
        let source = loan(begin)?;
        let call = one(g.instructions.iter().copied().filter(|i| i.origin == Origin::Source(anchor.call)
            && matches!(&i.operation, Operation::DirectCall { callee, receiver: None, arguments }
                if *callee == scan.id && arguments.as_slice() == [EntityId::Loan(source)])), "nested SSA actual caller scan argument")?;
        calls.insert(call.id);
        let end = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: source }),
            "nested SSA actual caller call-loan end",
        )?;
        let size = query_tail::size(&g, anchor.size, EntityId::Value(owner), &mut witness)?;
        let drop = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::Drop { owner }),
            "nested SSA caller exact owner Drop",
        )?;
        let sequence = [birth, root, begin, call, end, size, drop];
        need(
            sequence.iter().all(|i| i.block == tail)
                && sequence.windows(2).all(|p| {
                    g.position(p[0])
                        .ok()
                        .zip(g.position(p[1]).ok())
                        .is_some_and(|(a, b)| a < b)
                })
                && root.origin == Origin::Source(anchor.argument)
                && begin.origin == Origin::Source(anchor.argument)
                && end.origin == Origin::Source(anchor.call)
                && drop.origin == Origin::Source(anchor.drop)
                && anchor.drop == anchor.local
                && call.results.is_empty()
                && end.results.is_empty()
                && drop.results.is_empty()
                && ty(&g, EntityId::Place(place))? == EntityType::Place(map_type)
                && ty(&g, EntityId::Loan(source))? == shared,
            "nested SSA caller loan protects actual owner through scan then size before actual FE Drop",
            (&sequence, anchor),
        )?;
        for entity in [
            EntityId::Value(owner),
            EntityId::Place(place),
            EntityId::Loan(source),
        ] {
            need(
                protected.insert(entity),
                "nested SSA two caller identities remain disjoint",
                entity,
            )?;
            witness.uses.protect(entity);
        }
        witness.uses.permit(EntityId::Value(owner), root);
        witness.uses.permit(EntityId::Value(owner), drop);
        witness.uses.permit(EntityId::Place(place), begin);
        witness.uses.permit(EntityId::Loan(source), call);
        witness.uses.permit(EntityId::Loan(source), end);
        // A call links distinct function-scoped entity IDs by the actual callee/slot/type only.
        (tail, _) = query_tail::conditional(&g, f, size, &Roles::default(), &mut witness)?;
    }
    need(
        g.ops(tail)?.is_empty()
            && matches!(&g.term(tail)?.kind, TerminatorKind::Return { values } if values.is_empty())
            && g.term(tail)?.origin == Origin::Source(inputs.entry),
        "nested SSA caller reaches actual Unit return",
        g.term(tail)?,
    )?;
    need(
        g.instructions
            .iter()
            .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
            .map(|i| i.id)
            .collect::<BTreeSet<_>>()
            == calls
            && g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapSize { .. }))
                .count()
                == 2
            && !g
                .instructions
                .iter()
                .any(|i| super::query_body::provider(i)),
        "nested SSA exact complete caller call/size set with no hidden provider",
        &calls,
    )?;
    super::super::query_inventory::check(&g, &[map_type], &protected)?;
    witness.finish(&g)
}
