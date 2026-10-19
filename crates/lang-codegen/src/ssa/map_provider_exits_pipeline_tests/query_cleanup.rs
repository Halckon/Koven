//! 只读清理次序与实体消费；本夹具无条件直线 body，不接纳额外派生 loan。
use super::{
    fixture::{Exit, Failure, failure, need},
    query_graph::{Graph, Uses, function, loan, value},
    query_inputs::{Inputs, one},
};
use crate::ssa::model::*;

pub(super) fn end_source(
    graph: &Graph<'_>,
    input: &Inputs,
    block: BlockId,
    source: EntityId,
    seed: Option<EntityId>,
    uses: &mut Uses,
) -> Result<Vec<InstructionId>, Failure> {
    let end = one(graph.instructions.iter().copied().filter(|i| {
        matches!(i.operation, Operation::BorrowEnd { loan } if EntityId::Loan(loan) == source)
    }), "identity SSA unique path EndSource")?;
    need(
        end.block == block
            && [input.site.span, input.site.transfer_span].contains(&end.origin.span()),
        "identity SSA EndSource current path lease",
        end,
    )?;
    uses.permit(source, end);
    let mut sequence = vec![end.id];
    if input.owned {
        let Some(EntityId::Value(owner)) = seed else {
            return Err(failure("identity SSA path current owner", seed));
        };
        let drop = one(
            graph
                .instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::Drop { owner }),
            "identity SSA unique path source Drop",
        )?;
        need(
            drop.block == block
                && Some(drop.origin.span()) == input.source_drop
                && graph.position(drop)? == graph.position(end)? + 1,
            "identity SSA path EndSource immediately precedes current owner Drop",
            (end, drop),
        )?;
        uses.permit(EntityId::Value(owner), drop);
        sequence.push(drop.id);
    }
    Ok(sequence)
}

pub(super) fn body(
    graph: &Graph<'_>,
    module: &Module,
    input: &Inputs,
    components: [&Instruction; 2],
    tail: &[InstructionId],
    uses: &mut Uses,
) -> Result<Option<ValueId>, Failure> {
    let [key, val] = components;
    let k = loan(key)?;
    let v = loan(val)?;
    let block = key.block;
    let consumer = function(module, input.consumer)?;
    let deinit = function(module, input.deinit)?;
    let ops = graph.ops(block)?;
    let allocation = one(
        ops.iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::HeapAllocate { .. })),
        "identity SSA real Guard allocation",
    )?;
    let Operation::HeapAllocate {
        owner: guard_type,
        payload,
    } = allocation.operation
    else {
        return Err(failure("identity SSA Guard allocation shape", allocation));
    };
    need(
        allocation.origin == Origin::Source(input.site.guard_span)
            && module.deinits.get(&guard_type) == Some(&deinit.id),
        "identity SSA Guard allocation type/deinit",
        allocation,
    )?;
    let payload = graph.definition(EntityId::Value(payload))?;
    need(
        payload.block == block
            && payload.origin == Origin::Source(input.site.guard_span)
            && matches!(&payload.operation, Operation::AggregateConstruct { fields, .. } if fields.is_empty()),
        "identity SSA actual empty Guard payload construction",
        payload,
    )?;
    let guard_owner = value(allocation)?;
    let guard_drop = one(
        graph
            .instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::Drop { owner: guard_owner }),
        "identity SSA unique Guard owner Drop",
    )?;
    need(
        guard_drop.block == block && guard_drop.origin == Origin::Source(input.site.local),
        "identity SSA lexical Guard Drop origin distinct from constructor",
        guard_drop,
    )?;
    uses.permit(EntityId::Value(guard_owner), guard_drop);
    let call = one(graph.instructions.iter().copied().filter(|i| matches!(i.operation, Operation::DirectCall { callee, .. } if callee == consumer.id)),
        "identity SSA unique real consumer call")?;
    need(
        call.block == block
            && call.origin == Origin::Source(input.site.call_span)
            && matches!(&call.operation, Operation::DirectCall { arguments, .. } if arguments.as_slice() == [EntityId::Loan(k), EntityId::Loan(v)]),
        "identity SSA exact ordered K/V call operands",
        call,
    )?;
    let entry = consumer
        .entry_block()
        .and_then(|id| consumer.block(id))
        .ok_or_else(|| failure("identity SSA actual consumer signature", consumer.id))?;
    need(
        entry.parameters.len() == 2
            && consumer.borrow_return.is_none()
            && consumer.receiver.is_none(),
        "identity SSA consumer parameter slots",
        consumer,
    )?;
    for (&parameter, component) in entry.parameters.iter().zip([k, v]) {
        need(
            consumer.entity(parameter).map(|d| d.ty)
                == graph
                    .function
                    .entity(EntityId::Loan(component))
                    .map(|d| d.ty),
            "identity SSA call component/parameter type identity",
            (parameter, component),
        )?;
    }
    let mut ordered = vec![key.id, val.id, payload.id, allocation.id, call.id];
    let mut returned = None;
    if input.exit == Exit::Return {
        let initial = value(call)?;
        let ty = graph
            .function
            .entity(EntityId::Value(initial))
            .ok_or_else(|| failure("identity SSA operand result", initial))?
            .ty;
        need(
            matches!(ty, EntityType::Value(t) if module.type_kind(t) == Some(&SsaTypeKind::Integer { bits: 32, signed: true })
            && consumer.return_types.as_slice() == [t] && graph.function.return_types.as_slice() == [t]),
            "identity SSA actual Int operand/result signature",
            ty,
        )?;
        let mut cursor = initial;
        uses.protect(EntityId::Value(initial));
        // Only an actual forward Copy chain after the call and before cleanup is accepted.
        let call_position = graph.position(call)?;
        for i in ops
            .iter()
            .skip(call_position + 1)
            .take_while(|i| matches!(i.operation, Operation::Copy { .. }))
        {
            need(
                i.operation == Operation::Copy { source: cursor },
                "identity SSA Return Copy chain has actual call root",
                i,
            )?;
            uses.permit(EntityId::Value(cursor), i);
            cursor = value(i)?;
            uses.protect(EntityId::Value(cursor));
            ordered.push(i.id);
        }
        let term = graph.term(block)?;
        need(
            term.origin == Origin::Source(input.site.transfer_span)
                && matches!(&term.kind, TerminatorKind::Return { values } if values.as_slice() == [cursor]),
            "identity SSA actual operand result returned after cleanup",
            term,
        )?;
        returned = Some(cursor);
    } else {
        need(
            call.results.is_empty()
                && consumer.return_types.is_empty()
                && graph.function.return_types.is_empty(),
            "identity SSA actual Unit consumer result",
            call,
        )?;
    }
    ordered.push(guard_drop.id);
    for component in [v, k] {
        let end = one(
            graph
                .instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: component }),
            "identity SSA unique component EndBinding",
        )?;
        need(
            end.block == block
                && [input.site.span, input.site.transfer_span].contains(&end.origin.span()),
            "identity SSA component ends at actual transfer",
            end,
        )?;
        uses.permit(EntityId::Loan(component), call);
        uses.permit(EntityId::Loan(component), end);
        ordered.push(end.id);
    }
    ordered.extend_from_slice(tail);
    need(
        ops.iter().map(|i| i.id).collect::<Vec<_>>() == ordered,
        "identity SSA full body order: acquire, operand, Guard, value/key, terminal cleanup or Next",
        (&ops, ordered),
    )?;
    Ok(returned)
}
