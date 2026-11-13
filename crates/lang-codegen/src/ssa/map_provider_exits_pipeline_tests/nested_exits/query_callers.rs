//! Real entry owner -> call loan -> scan parameter, and read's borrowed consumer slots.
use super::super::{
    fixture::{Failure, contains, failure, need},
    query_graph::{Graph, Uses, function, loan, result, value},
    query_inputs::one,
};
use super::{
    ast::{Exit, Input},
    query_body::{Context, INNER, OUTER},
    query_graph::{Role, Roles, Witness, full_graph, ty},
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
        .ok_or_else(|| failure("nested exits SSA callable entry", function.id))?;
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
        "nested exits SSA source callable actual parameter definitions/signature",
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
        "nested exits SSA actual read single body",
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
        "nested exits SSA exact source read consumers",
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
            "nested exits SSA actual println source consumer",
        )?;
        let EntityId::Loan(actual) = param else {
            return Err(failure("nested exits SSA actual read Borrow slot", param));
        };
        need(
            instruction.operation == Operation::PrintString { value: actual }
                && instruction.results.is_empty()
                && g.position(instruction)? == index,
            "nested exits SSA consumer reads its own ordered Borrow parameter",
            instruction,
        )?;
        uses.permit(param, instruction);
    }
    need(
        matches!(&g.term(block)?.kind, TerminatorKind::Return { values } if values.is_empty())
            && g.term(block)?.origin == Origin::Source(inputs.read),
        "nested exits SSA read Unit return no borrowed escape",
        g.term(block)?,
    )?;
    uses.check(&g, None)
}

fn operand_consumer(
    module: &Module,
    f: &Input,
    inputs: &Inputs,
    string: SsaTypeId,
) -> Result<(), Failure> {
    let Some(anchor) = &inputs.operand else {
        return Ok(());
    };
    let function = function(module, anchor.function)?;
    let int = *one(
        function.return_types.iter(),
        "nested exits SSA operand sole Int result",
    )?;
    need(
        super::query_resources::module_int(module, int),
        "nested exits SSA operand actual Int type",
        int,
    )?;
    let shared = EntityType::Loan {
        kind: LoanKind::Shared,
        target: string,
    };
    signature(function, &[shared; 4], &[int])?;
    let g = Graph::new(function)?;
    full_graph(&g)?;
    let block = one(
        g.blocks.iter().copied(),
        "nested exits SSA operand actual single block",
    )?;
    let mut roles = Roles::default();
    for (role, &entity) in [
        Role::OuterKey,
        Role::OuterValue,
        Role::InnerKey,
        Role::InnerValue,
    ]
    .into_iter()
    .zip(&g.block(block)?.parameters)
    {
        roles.insert(role, entity);
    }
    let mut w = Witness::default();
    w.protect(&roles)?;
    let context = Context {
        graph: &g,
        module,
        read: function_by_read(module, inputs)?,
    };
    let first = context.read(block, &inputs.reads[1], OUTER, &roles, &mut w)?;
    let second = context.read(block, &inputs.reads[2], INNER, &roles, &mut w)?;
    need(
        g.position(first)? < g.position(second)?
            && g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
                .map(|i| i.id)
                .collect::<BTreeSet<_>>()
                == [first.id, second.id].into_iter().collect(),
        "nested exits SSA operand reads its own four ordered Borrow parameters",
        function,
    )?;
    let TerminatorKind::Return { values } = &g.term(block)?.kind else {
        return Err(failure(
            "nested exits SSA operand real Return",
            g.term(block)?,
        ));
    };
    let returned = one(
        values.iter().copied(),
        "nested exits SSA operand one return value",
    )?;
    let producer = g.definition(EntityId::Value(returned))?;
    need(
        producer.operation == Operation::Constant(ScalarConstant::Integer(7))
            && producer.block == block
            && g.position(second)? < g.position(producer)?
            && ty(&g, EntityId::Value(returned))? == EntityType::Value(int)
            && f.sources.slice(producer.origin.span()) == Ok("7")
            && contains(anchor.function, g.term(block)?.origin.span())
            && contains(g.term(block)?.origin.span(), producer.origin.span()),
        "nested exits SSA operand source literal/result return after both actual reads",
        producer,
    )?;
    w.returns.insert(block, vec![returned]);
    w.finish(&g)
}

fn function_by_read<'a>(module: &'a Module, input: &Inputs) -> Result<&'a Function, Failure> {
    function(module, input.read)
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
        .ok_or_else(|| failure("nested exits SSA caller map type", map_type))?;
    need(string == val, "nested exits SSA exact String K/V", map_type)?;
    let shared = EntityType::Loan {
        kind: LoanKind::Shared,
        target: map_type,
    };
    signature(entry, &[], &[])?;
    signature(scan, &[shared], &scan.return_types)?;
    if f.exit == Exit::Return {
        let int = one(
            scan.return_types.iter().copied(),
            "nested exits SSA scan Int signature",
        )?;
        need(
            super::query_resources::module_int(module, int),
            "nested exits SSA scan Int result",
            scan,
        )?;
    } else {
        need(
            scan.return_types.is_empty(),
            "nested exits SSA scan Unit result",
            scan,
        )?;
    }
    signature(factory, &[], &[map_type])?;
    consumer(read, f, inputs, string)?;
    operand_consumer(module, f, inputs, string)?;
    let g = Graph::new(entry)?;
    full_graph(&g)?;
    let mut w = Witness::default();
    let mut protected = BTreeSet::new();
    let mut calls = BTreeSet::new();
    let mut tail = entry
        .entry_block()
        .ok_or_else(|| failure("nested exits SSA entry", entry.id))?;
    g.incoming_from(tail, &[])?;
    let mut carried = Roles::default();
    for (index, anchor) in inputs.callers.iter().enumerate() {
        let role = if index == 0 {
            Role::CallerOwnerA
        } else {
            Role::CallerOwnerB
        };
        let birth = one(g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(anchor.initializer) && if index == 0 {
                matches!(&i.operation, Operation::DirectCall { callee, receiver: None, arguments } if *callee == factory.id && arguments.is_empty())
            } else { i.operation == Operation::MapConstruct { map_type } }
        }), "nested exits SSA caller exact local initializer")?;
        let owner = value(birth)?;
        need(
            birth.block == tail && ty(&g, result(birth)?)? == EntityType::Value(map_type),
            "nested exits SSA caller own result in source continuation",
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
            "nested exits SSA caller exact owner RootPlace",
        )?;
        let EntityId::Place(place) = result(root)? else {
            return Err(failure("nested exits SSA caller Place", root));
        };
        let begin = one(
            g.instructions.iter().copied().filter(|i| {
                i.operation
                    == Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    }
            }),
            "nested exits SSA caller Shared loan",
        )?;
        let source = loan(begin)?;
        let call = one(g.instructions.iter().copied().filter(|i| i.origin == Origin::Source(anchor.call)
            && matches!(&i.operation, Operation::DirectCall { callee, receiver: None, arguments }
                if *callee == scan.id && arguments.as_slice() == [EntityId::Loan(source)])),
            "nested exits SSA caller actual scan target and sole Borrow slot")?;
        calls.insert(call.id);
        let end = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: source }),
            "nested exits SSA caller exact call loan end",
        )?;
        let prefix = [birth, root, begin, call, end];
        need(
            prefix.iter().all(|i| i.block == tail)
                && prefix.windows(2).all(|p| {
                    g.position(p[0])
                        .ok()
                        .zip(g.position(p[1]).ok())
                        .is_some_and(|(a, b)| a < b)
                })
                && root.origin == Origin::Source(anchor.argument)
                && begin.origin == Origin::Source(anchor.argument)
                && end.origin == Origin::Source(anchor.call)
                && end.results.is_empty()
                && ty(&g, EntityId::Place(place))? == EntityType::Place(map_type)
                && ty(&g, EntityId::Loan(source))? == shared,
            "nested exits SSA caller owner root call loan and result exact order",
            prefix,
        )?;
        for entity in [
            EntityId::Value(owner),
            EntityId::Place(place),
            EntityId::Loan(source),
        ] {
            need(
                protected.insert(entity),
                "nested exits SSA two caller owners/places/loans distinct",
                entity,
            )?;
            w.uses.protect(entity);
        }
        w.uses.permit(EntityId::Value(owner), root);
        w.uses.permit(EntityId::Place(place), begin);
        w.uses.permit(EntityId::Loan(source), call);
        w.uses.permit(EntityId::Loan(source), end);
        let mut current = EntityId::Value(owner);
        if let Some(reference) = anchor.result_reference {
            need(
                f.exit == Exit::Return
                    && ty(&g, result(call)?)? == EntityType::Value(scan.return_types[0]),
                "nested exits SSA caller actual returned Int",
                call,
            )?;
            w.uses.protect(result(call)?);
            let mut roles = carried.clone();
            roles.insert(role, current);
            roles.insert(
                if index == 0 {
                    Role::CallerResultA
                } else {
                    Role::CallerResultB
                },
                result(call)?,
            );
            let (joined, next) = query_tail::conditional(
                &g,
                f,
                call,
                Some((reference, if index == 0 { 7 } else { 0 })),
                &roles,
                &mut w,
            )?;
            tail = joined;
            current = next.get(role)?;
            carried = next;
            carried.0.remove(&role);
        } else {
            need(
                f.exit != Exit::Return && call.results.is_empty(),
                "nested exits SSA caller actual Unit call",
                call,
            )?;
        }
        let size = query_tail::size(&g, anchor.size, current, &mut w)?;
        let EntityId::Value(current_owner) = current else {
            return Err(failure(
                "nested exits SSA transported caller owner",
                current,
            ));
        };
        let drop = one(
            g.instructions.iter().copied().filter(|i| {
                i.operation
                    == Operation::Drop {
                        owner: current_owner,
                    }
            }),
            "nested exits SSA current caller owner Drop",
        )?;
        need(
            size.block == tail
                && drop.block == tail
                && g.position(size)? < g.position(drop)?
                && (f.exit == Exit::Return || g.position(end)? < g.position(size)?)
                && drop.origin == Origin::Source(anchor.drop)
                && anchor.drop == anchor.local
                && drop.results.is_empty(),
            "nested exits SSA caller size then FE Drop on exact current owner",
            (size, drop, anchor),
        )?;
        w.uses.permit(current, drop);
        (tail, carried) = query_tail::conditional(&g, f, size, None, &carried, &mut w)?;
    }
    need(
        g.ops(tail)?.is_empty()
            && matches!(&g.term(tail)?.kind, TerminatorKind::Return { values } if values.is_empty())
            && g.term(tail)?.origin == Origin::Source(inputs.entry),
        "nested exits SSA caller final Unit Return",
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
        "nested exits SSA complete actual caller call/size/provider set",
        &calls,
    )?;
    let mut related = protected;
    for role in [Role::CallerOwnerA, Role::CallerOwnerB] {
        if let Some(entities) = w.aliases.get(&role) {
            related.extend(entities.iter().copied());
        }
    }
    super::super::query_inventory::check(&g, &[map_type], &related)?;
    w.finish(&g)
}
