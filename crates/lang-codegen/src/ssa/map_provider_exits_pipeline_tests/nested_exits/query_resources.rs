//! Guard allocation identity and exact emitted cleanup, separate from FE cleanup actions.
use super::super::{
    fixture::{Failure, failure, need},
    query_graph::{function, result, value},
    query_inputs::one,
};
use super::{
    query_body::{Context, Layer},
    query_graph::{Role, Roles, Witness, ty},
    query_inputs::{GuardInput, Inputs},
};
use crate::ssa::model::*;
use lang_frontend::source::Span;

pub(super) struct CleanupSite<'a> {
    pub layer: Layer,
    pub guard: Role,
    pub guard_input: &'a GuardInput,
    pub origin: Span,
    pub advance: Option<Span>,
}

impl<'ir> Context<'_, 'ir> {
    pub fn guard(
        &self,
        block: BlockId,
        input: &GuardInput,
        role: Role,
        roles: &mut Roles,
        w: &mut Witness,
    ) -> Result<InstructionId, Failure> {
        let g = self.graph;
        let allocation = one(
            g.ops(block)?.into_iter().filter(|i| {
                i.origin == Origin::Source(input.construction)
                    && matches!(i.operation, Operation::HeapAllocate { .. })
            }),
            "nested exits SSA exact source Guard allocation",
        )?;
        let Operation::HeapAllocate { owner, payload } = allocation.operation else {
            return Err(failure(
                "nested exits SSA actual Guard allocation",
                allocation,
            ));
        };
        let deinit = function(self.module, input.deinit)?;
        let payload_op = g.definition(EntityId::Value(payload))?;
        let Operation::AggregateConstruct {
            aggregate,
            ref fields,
        } = payload_op.operation
        else {
            return Err(failure(
                "nested exits SSA actual Guard aggregate",
                payload_op,
            ));
        };
        need(
            payload_op.block == block
                && payload_op.origin == Origin::Source(input.construction)
                && fields.is_empty()
                && matches!(self.module.type_kind(owner), Some(SsaTypeKind::HeapOwner { payload: Some(actual), .. }) if *actual == aggregate)
                && matches!(self.module.type_kind(aggregate), Some(SsaTypeKind::Aggregate { fields, .. }) if fields.is_empty())
                && self.module.deinits.get(&owner) == Some(&deinit.id)
                && ty(g, result(allocation)?)? == EntityType::Value(owner)
                && ty(g, result(payload_op)?)? == EntityType::Value(aggregate)
                && g.position(payload_op)? == 2
                && g.position(allocation)? == 3
                && input.class_marker != input.marker
                && deinit.return_types.is_empty()
                && deinit.borrow_return.is_none()
                && deinit.carrier_return.is_none()
                && deinit.receiver
                    == Some(EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: owner,
                    }),
            "nested exits SSA Guard actual empty payload/type/deinit and source-order construction",
            (input, payload_op, allocation, deinit),
        )?;
        w.uses.protect(EntityId::Value(payload));
        w.uses.permit(EntityId::Value(payload), allocation);
        roles.insert(role, result(allocation)?);
        w.protect(roles)?;
        Ok(allocation.id)
    }

    /// Returns the exact expected sequence, without treating FinishProvider as a fake SSA op.
    pub fn cleanup(
        &self,
        block: BlockId,
        cleanup: CleanupSite<'_>,
        roles: &Roles,
        w: &mut Witness,
    ) -> Result<(Roles, Vec<InstructionId>), Failure> {
        let CleanupSite {
            layer,
            guard,
            guard_input,
            origin,
            advance,
        } = cleanup;
        let g = self.graph;
        let owner = roles.value(guard)?;
        let drop = one(
            g.ops(block)?
                .into_iter()
                .filter(|i| i.operation == Operation::Drop { owner }),
            "nested exits SSA exact current Guard Drop",
        )?;
        need(
            drop.origin == Origin::Source(guard_input.marker) && drop.results.is_empty(),
            "nested exits SSA current Guard Drop retains actual FE local origin",
            drop,
        )?;
        w.uses.permit(EntityId::Value(owner), drop);
        let mut sequence = vec![drop.id];
        for role in [layer.val, layer.key] {
            let loan = roles.loan(role)?;
            let end = one(
                g.ops(block)?
                    .into_iter()
                    .filter(|i| i.operation == Operation::BorrowEnd { loan }),
                "nested exits SSA unique current V/K EndBinding",
            )?;
            need(
                end.origin == Origin::Source(origin) && end.results.is_empty(),
                "nested exits SSA V/K cleanup exact source point",
                end,
            )?;
            w.uses.permit(EntityId::Loan(loan), end);
            sequence.push(end.id);
        }
        let mut after = roles.clone();
        for role in [guard, layer.val, layer.key] {
            after.0.remove(&role);
        }
        let source = roles.loan(layer.source)?;
        if let Some(advance_origin) = advance {
            let cursor = roles.value(layer.cursor)?;
            let next = one(
                g.ops(block)?
                    .into_iter()
                    .filter(|i| i.operation == Operation::MapIterationNext { source, cursor }),
                "nested exits SSA exact current layer Next",
            )?;
            need(
                next.origin == Origin::Source(advance_origin)
                    && ty(g, result(next)?)? == ty(g, roles.get(layer.cursor)?)?,
                "nested exits SSA Next origin and same cursor type",
                next,
            )?;
            w.uses.permit(EntityId::Loan(source), next);
            w.uses.permit(EntityId::Value(cursor), next);
            w.providers.insert(next.id);
            sequence.push(next.id);
            after.insert(layer.cursor, result(next)?);
        } else {
            let end = one(
                g.ops(block)?
                    .into_iter()
                    .filter(|i| i.operation == Operation::BorrowEnd { loan: source }),
                "nested exits SSA exact current layer EndSource",
            )?;
            need(
                end.origin == Origin::Source(origin) && end.results.is_empty(),
                "nested exits SSA terminal EndSource origin",
                end,
            )?;
            w.uses.permit(EntityId::Loan(source), end);
            sequence.push(end.id);
            after.0.remove(&layer.source);
            after.0.remove(&layer.cursor);
        }
        w.protect(&after)?;
        Ok((after, sequence))
    }

    pub fn suffix(&self, block: BlockId, expected: &[InstructionId]) -> Result<(), Failure> {
        let first = expected
            .first()
            .copied()
            .ok_or_else(|| failure("nested exits SSA nonempty cleanup suffix", expected))?;
        let instruction = self
            .graph
            .function
            .instruction(first)
            .ok_or_else(|| failure("nested exits SSA cleanup head", first))?;
        need(
            self.graph.ops(block)?[self.graph.position(instruction)?..]
                .iter()
                .map(|i| i.id)
                .collect::<Vec<_>>()
                == expected,
            "nested exits SSA full cleanup suffix in exact order",
            (expected, self.graph.ops(block)?),
        )
    }

    pub fn operand(
        &self,
        block: BlockId,
        input: &Inputs,
        roles: &Roles,
        w: &mut Witness,
    ) -> Result<(&'ir Instruction, ValueId), Failure> {
        let g = self.graph;
        let anchor = input
            .operand
            .as_ref()
            .ok_or_else(|| failure("nested exits SSA actual operand bridge", input))?;
        let callee = function(self.module, anchor.function)?;
        let args = [
            Role::OuterKey,
            Role::OuterValue,
            Role::InnerKey,
            Role::InnerValue,
        ]
        .into_iter()
        .map(|role| roles.get(role))
        .collect::<Result<Vec<_>, _>>()?;
        let call = one(
            g.ops(block)?.into_iter().filter(|i| {
                i.origin == Origin::Source(anchor.call)
                    && matches!(i.operation, Operation::DirectCall { .. })
            }),
            "nested exits SSA actual Return operand call",
        )?;
        let entry = callee
            .entry_block()
            .and_then(|id| callee.block(id))
            .ok_or_else(|| failure("nested exits SSA operand callee entry", callee.id))?;
        let initial = value(call)?;
        let int = ty(g, EntityId::Value(initial))?.semantic_type();
        need(
            matches!(&call.operation, Operation::DirectCall { callee: target, receiver: None, arguments }
                if *target == callee.id && arguments == &args)
                && module_int(self.module, int)
                && callee.return_types.as_slice() == [int]
                && g.function.return_types.as_slice() == [int]
                && entry.parameters.len() == 4
                && entry.parameters.iter().zip(&args).all(|(&p, &a)| {
                    callee.entity(p).map(|d| d.ty) == g.function.entity(a).map(|d| d.ty)
                })
                && anchor
                    .arguments
                    .iter()
                    .all(|&span| super::super::fixture::contains(anchor.call, span)),
            "nested exits SSA actual operand declaration and four ordered current Borrow slots",
            (call, callee, &args),
        )?;
        for arg in args {
            w.uses.permit(arg, call);
        }
        let mut current = initial;
        w.uses.protect(EntityId::Value(current));
        for copy in g
            .ops(block)?
            .into_iter()
            .skip(g.position(call)? + 1)
            .take_while(|i| matches!(i.operation, Operation::Copy { .. }))
        {
            need(
                copy.operation == Operation::Copy { source: current }
                    && ty(g, result(copy)?)? == EntityType::Value(int)
                    && copy.origin == Origin::Source(input.transfer),
                "nested exits SSA same Int operand forward Copy chain",
                copy,
            )?;
            w.uses.permit(EntityId::Value(current), copy);
            current = value(copy)?;
            w.uses.protect(EntityId::Value(current));
        }
        need(
            g.term(block)?.origin == Origin::Source(input.transfer)
                && matches!(&g.term(block)?.kind, TerminatorKind::Return { values } if values.as_slice() == [current]),
            "nested exits SSA exact evaluated Int reaches actual Return",
            g.term(block)?,
        )?;
        w.returned = Some((block, current));
        w.returns.insert(block, vec![current]);
        Ok((call, current))
    }
}

pub(super) fn module_int(module: &Module, ty: SsaTypeId) -> bool {
    module.type_kind(ty)
        == Some(&SsaTypeKind::Integer {
            bits: 32,
            signed: true,
        })
}
