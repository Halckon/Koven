//! 每个已证明路径的实际 Guard/KV/source 消费与 Return operand 的求值顺序。
use super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, result, value},
    query_inputs::one,
};
use super::{
    ast::{Anchors, Input},
    query_inputs::Inputs,
    query_ssa::Roles,
};
use crate::ssa::model::*;
use lang_frontend::{
    parser::{Expression, Statement},
    source::Span,
};

pub(super) struct Tail {
    pub next: Option<ValueId>,
    pub returned: Option<ValueId>,
}

pub(super) struct Context<'a, 'ir> {
    pub graph: &'a Graph<'ir>,
    pub module: &'ir Module,
    pub f: &'a Input,
    pub a: &'a Anchors,
    pub input: &'a Inputs,
}

impl Context<'_, '_> {
    fn end_source(
        &self,
        block: BlockId,
        roles: Roles,
        span: Span,
        uses: &mut Uses,
    ) -> Result<Vec<InstructionId>, Failure> {
        let (graph, f, a, input) = (self.graph, self.f, self.a, self.input);
        let end = one(
            graph
                .ops(block)?
                .into_iter()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: roles.source }),
            "conditional identity path EndSource",
        )?;
        need(
            end.origin == Origin::Source(span),
            "conditional identity exact EndSource origin",
            end,
        )?;
        uses.permit(EntityId::Loan(roles.source), end);
        let mut expected = vec![end.id];
        if f.map {
            let Some(EntityId::Value(owner)) = roles.seed else {
                return Err(failure("conditional identity current source owner", roles));
            };
            let drop = one(
                graph
                    .ops(block)?
                    .into_iter()
                    .filter(|i| i.operation == Operation::Drop { owner }),
                "conditional identity path source Drop",
            )?;
            need(
                Some(drop.origin.span()) == input.source_drop
                    && drop.origin == Origin::Source(input.parameter)
                    && graph.position(drop)? == graph.position(end)? + 1,
                "conditional identity current owner ends after its provider loan",
                (end, drop),
            )?;
            uses.permit(EntityId::Value(owner), drop);
            expected.push(drop.id);
        } else {
            need(
                input.source_drop.is_none(),
                "conditional identity Borrow has no source Drop fact",
                input,
            )?;
        }
        need(
            a.span.source_id() == span.source_id(),
            "conditional identity cleanup belongs to actual source",
            span,
        )?;
        Ok(expected)
    }

    pub(super) fn body(
        &self,
        index: usize,
        block: BlockId,
        roles: Roles,
        uses: &mut Uses,
    ) -> Result<Tail, Failure> {
        let (graph, module, f, a) = (self.graph, self.module, self.f, self.a);
        let (key, val, guard) = roles
            .body
            .ok_or_else(|| failure("conditional identity live cleanup body", roles))?;
        let transfer = if index < 3 {
            f.expression(a.branches[index].transfer)?.1
        } else {
            a.span
        };
        // Both layouts consume Unit cleanup facts with the actual For origin.
        let cleanup_span = a.span;
        let ops = graph.ops(block)?;
        let guard_drop = one(
            ops.iter()
                .copied()
                .filter(|i| i.operation == Operation::Drop { owner: guard }),
            "conditional identity current Guard Drop",
        )?;
        need(
            guard_drop.origin == Origin::Source(a.local),
            "conditional identity Guard lexical origin",
            guard_drop,
        )?;
        uses.permit(EntityId::Value(guard), guard_drop);
        let mut expected = vec![guard_drop.id];
        for component in [val, key] {
            let end = one(
                ops.iter()
                    .copied()
                    .filter(|i| i.operation == Operation::BorrowEnd { loan: component }),
                "conditional identity current component EndBinding",
            )?;
            need(
                end.origin == Origin::Source(cleanup_span),
                "conditional identity exact component cleanup origin",
                end,
            )?;
            uses.permit(EntityId::Loan(component), end);
            expected.push(end.id);
        }
        let mut output = Tail {
            next: None,
            returned: None,
        };
        if index == 0 || index == 3 {
            let next = one(
                ops.iter()
                    .copied()
                    .filter(|i| matches!(i.operation, Operation::MapIterationNext { .. })),
                "conditional identity this path unique Next",
            )?;
            need(
                next.operation
                    == Operation::MapIterationNext {
                        source: roles.source,
                        cursor: roles.cursor,
                    }
                    && next.origin == Origin::Source(transfer)
                    && graph.function.entity(result(next)?).map(|e| e.ty)
                        == graph
                            .function
                            .entity(EntityId::Value(roles.cursor))
                            .map(|e| e.ty),
                "conditional identity this path current lease/cursor and Next result",
                next,
            )?;
            uses.permit(EntityId::Loan(roles.source), next);
            uses.permit(EntityId::Value(roles.cursor), next);
            uses.protect(result(next)?);
            expected.push(next.id);
            output.next = Some(value(next)?);
        } else {
            need(
                !ops.iter()
                    .any(|i| matches!(i.operation, Operation::MapIterationNext { .. })),
                "conditional identity Break/Return has no Next",
                &ops,
            )?;
            expected.extend(self.end_source(block, roles, cleanup_span, uses)?);
        }
        let drop_position = graph.position(guard_drop)?;
        need(
            ops[drop_position..]
                .iter()
                .map(|i| i.id)
                .collect::<Vec<_>>()
                == expected,
            "conditional identity full terminal cleanup suffix",
            (&ops, &expected),
        )?;
        let direct_calls = ops
            .iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
            .collect::<Vec<_>>();
        if index == 0 {
            need(
                direct_calls.is_empty(),
                "conditional identity Continue has no component call",
                &direct_calls,
            )?;
        } else {
            let call_index = index - 1;
            let (name, expression, _) = a.component_calls[call_index];
            let target = one(
                a.functions.iter().filter(|v| v.name == name),
                "conditional identity consumer AST",
            )?;
            let consumer = function(module, target.span)?;
            let call = one(
                direct_calls.into_iter(),
                "conditional identity exact body source call",
            )?;
            need(
                call.origin == Origin::Source(f.expression(expression)?.1)
                    && matches!(&call.operation, Operation::DirectCall { callee, receiver: None, arguments }
                if *callee == consumer.id && arguments.as_slice() == [EntityId::Loan(key), EntityId::Loan(val)])
                    && graph.position(call)? < drop_position,
                "conditional identity ordered current K/V call before cleanup",
                call,
            )?;
            let entry = consumer
                .entry_block()
                .and_then(|b| consumer.block(b))
                .ok_or_else(|| failure("conditional identity consumer entry", consumer.id))?;
            need(
                entry.parameters.len() == 2
                    && consumer.receiver.is_none()
                    && consumer.borrow_return.is_none()
                    && entry.parameters.iter().zip([key, val]).all(|(&p, c)| {
                        consumer.entity(p).map(|e| e.ty)
                            == graph.function.entity(EntityId::Loan(c)).map(|e| e.ty)
                    }),
                "conditional identity call parameters match actual K/V types",
                consumer,
            )?;
            uses.permit(EntityId::Loan(key), call);
            uses.permit(EntityId::Loan(val), call);
            if index == 2 {
                let initial = value(call)?;
                let ty = graph
                    .function
                    .entity(EntityId::Value(initial))
                    .ok_or_else(|| failure("conditional identity Return result entity", initial))?
                    .ty;
                need(
                    matches!(ty, EntityType::Value(t)
                if module.type_kind(t) == Some(&SsaTypeKind::Integer { bits: 32, signed: true })
                    && consumer.return_types.as_slice() == [t] && graph.function.return_types.as_slice() == [t]),
                    "conditional identity real operand Int result/signatures",
                    ty,
                )?;
                uses.protect(EntityId::Value(initial));
                let mut cursor = initial;
                let mut prefix = vec![call.id];
                for i in ops
                    .iter()
                    .skip(graph.position(call)? + 1)
                    .take(drop_position - graph.position(call)? - 1)
                {
                    need(
                        i.operation == Operation::Copy { source: cursor },
                        "conditional identity Return only actual forward Copy chain",
                        i,
                    )?;
                    uses.permit(EntityId::Value(cursor), i);
                    cursor = value(i)?;
                    uses.protect(EntityId::Value(cursor));
                    prefix.push(i.id);
                }
                need(
                    ops[..drop_position]
                        .iter()
                        .map(|i| i.id)
                        .collect::<Vec<_>>()
                        == prefix
                        && graph.term(block)?.origin == Origin::Source(transfer)
                        && matches!(&graph.term(block)?.kind, TerminatorKind::Return { values } if values.as_slice() == [cursor]),
                    "conditional identity actual operand and copies precede cleanup, same value returned",
                    graph.term(block)?,
                )?;
                output.returned = Some(cursor);
            } else {
                need(
                    call.results.is_empty() && consumer.return_types.is_empty(),
                    "conditional identity read has actual Unit result",
                    call,
                )?;
            }
        }
        Ok(output)
    }

    pub(super) fn exhausted(
        &self,
        block: BlockId,
        roles: Roles,
        uses: &mut Uses,
    ) -> Result<(), Failure> {
        let (graph, a) = (self.graph, self.a);
        need(
            roles.body.is_none(),
            "conditional identity false path never acquired K/V/Guard",
            roles,
        )?;
        let expected = self.end_source(block, roles, a.span, uses)?;
        need(
            graph.ops(block)?.iter().map(|i| i.id).collect::<Vec<_>>() == expected,
            "conditional identity exhaustion only source cleanup, no payload/Next",
            graph.ops(block)?,
        )
    }

    pub(super) fn fallback(&self, block: BlockId) -> Result<(), Failure> {
        let (graph, f, a) = (self.graph, self.f, self.a);
        let scan = one(
            a.functions.iter().filter(|v| v.name == "scan"),
            "conditional identity fallback scan",
        )?;
        let body = f
            .parsed
            .ast()
            .statements()
            .get(scan.body)
            .map_err(|e| failure("conditional identity scan body", e))?;
        let Statement::Block { elements } = body.payload() else {
            return Err(failure("conditional identity scan block", body));
        };
        let statement = elements
            .last()
            .ok_or_else(|| failure("conditional identity fallback statement", elements))?;
        let statement = f
            .parsed
            .ast()
            .statements()
            .get(*statement)
            .map_err(|e| failure("conditional identity fallback AST", e))?;
        let Statement::Expression { expression } = statement.payload() else {
            return Err(failure(
                "conditional identity fallback expression",
                statement,
            ));
        };
        let (returned, span) = f.expression(*expression)?;
        let Expression::Return {
            value: Some(operand),
            ..
        } = returned
        else {
            return Err(failure(
                "conditional identity actual fallback Return",
                returned,
            ));
        };
        let (_, operand_span) = f.expression(*operand)?;
        let TerminatorKind::Return { values } = &graph.term(block)?.kind else {
            return Err(failure(
                "conditional identity after-loop Return",
                graph.term(block)?,
            ));
        };
        let returned = one(
            values.iter().copied(),
            "conditional identity fallback sole result",
        )?;
        let constant = graph.definition(EntityId::Value(returned))?;
        need(
            graph.term(block)?.origin == Origin::Source(span)
                && constant.block == block
                && constant.origin == Origin::Source(operand_span)
                && constant.operation == Operation::Constant(ScalarConstant::Integer(0)),
            "conditional identity source fallback exact zero after source cleanup",
            constant,
        )?;
        need(
            !graph.ops(block)?.iter().any(|i| {
                matches!(
                    i.operation,
                    Operation::MapIterationFirst { .. }
                        | Operation::MapIterationHasNext { .. }
                        | Operation::MapIterationKeyLoan { .. }
                        | Operation::MapIterationValueLoan { .. }
                        | Operation::MapIterationNext { .. }
                        | Operation::HeapAllocate { .. }
                        | Operation::DirectCall { .. }
                )
            }),
            "conditional identity after-loop cannot read provider payload or execute body",
            graph.ops(block)?,
        )
    }
}
