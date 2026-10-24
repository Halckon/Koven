//! The original outer source is evaluated before the actual temporary provider entry.
use crate::ssa::{
    map_provider_exits_pipeline_tests::{
        fixture::{Failure, failure, need},
        query_graph::{Graph, result},
        query_inputs::one,
    },
    model::*,
};
use lang_frontend::parser::{Expression, ParsedFile, Statement};

pub(in crate::ssa::map_provider_exits_pipeline_tests) fn check(
    parsed: &ParsedFile,
    program: &Program,
) -> Result<(), Failure> {
    for (_, statement) in parsed.ast().statements().iter() {
        let Statement::For { source, .. } = statement.payload() else {
            continue;
        };
        let outer = parsed
            .ast()
            .expressions()
            .get(*source)
            .map_err(|e| failure("temporary Entry original outer source", e))?;
        let mut leaf = outer;
        while let Expression::Group { expression } = leaf.payload() {
            leaf = parsed
                .ast()
                .expressions()
                .get(*expression)
                .map_err(|e| failure("temporary Entry complete original Group chain", e))?;
        }
        if !matches!(leaf.payload(), Expression::Call { .. }) {
            continue;
        }
        need(
            outer.span().source_id() == parsed.source_id()
                && leaf.span().source_id() == outer.span().source_id()
                && outer.span().start() <= leaf.span().start()
                && leaf.span().end() <= outer.span().end()
                && outer.span().end() < statement.span().end(),
            "temporary Entry retains exact outer AfterExpression and canonical Call spans",
            (outer.span(), leaf.span(), statement.span()),
        )?;
        let mut selected = 0;
        for module in &program.modules {
            for function in &module.functions {
                if !function.instructions.iter().any(|op| {
                    op.origin == Origin::Source(statement.span())
                        && matches!(op.operation, Operation::MapIterationFirst { .. })
                }) {
                    continue;
                }
                selected += 1;
                let graph = Graph::with_abort(function)?;
                let first = one(
                    graph.instructions.iter().copied().filter(|op| {
                        op.origin == Origin::Source(statement.span())
                            && matches!(op.operation, Operation::MapIterationFirst { .. })
                    }),
                    "temporary Entry unique actual original First",
                )?;
                let producer = one(
                    graph.instructions.iter().copied().filter(|op| {
                        op.origin == Origin::Source(leaf.span())
                            && matches!(op.results.as_slice(), [EntityId::Value(value)]
                            if function.entity(EntityId::Value(*value)).is_some_and(|v|
                                module.map_container(v.ty.semantic_type()).is_some()))
                    }),
                    "temporary Entry exact canonical Call owner producer",
                )?;
                need(
                    matches!(
                        producer.operation,
                        Operation::DirectCall { .. }
                            | Operation::CallableInvoke { .. }
                            | Operation::MapConstruct { .. }
                            | Operation::MapConsume { .. }
                    ),
                    "temporary Entry is the original call/intrinsic result, never a substitute",
                    producer,
                )?;
                let Operation::MapIterationFirst { source: lease } = first.operation else {
                    unreachable!()
                };
                let begin = graph.definition(EntityId::Loan(lease))?;
                let Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                } = begin.operation
                else {
                    return Err(failure("temporary Entry actual Shared source lease", begin));
                };
                let root = graph.definition(EntityId::Place(place))?;
                let Operation::RootPlace { owner } = root.operation else {
                    return Err(failure("temporary Entry actual original owner root", root));
                };
                graph.anchored(EntityId::Value(owner), &[result(producer)?])?;
                need(
                    producer.block == root.block
                        && root.block == begin.block
                        && begin.block == first.block
                        && graph.position(producer)? < graph.position(root)?
                        && graph.position(root)? < graph.position(begin)?
                        && graph.position(begin)? < graph.position(first)?
                        && root.origin == Origin::Source(statement.span())
                        && begin.origin == Origin::Source(statement.span()),
                    "temporary Entry follows complete outer source evaluation before acquisition",
                    (outer.span(), producer, root, begin, first),
                )?;
                let edge = graph.branch(first.block)?;
                let header_owner = graph.transported(edge, EntityId::Value(owner))?;
                let header_lease = graph.transported(edge, EntityId::Loan(lease))?;
                let header_cursor = graph.transported(edge, result(first)?)?;
                graph.anchored(header_owner, &[result(producer)?])?;
                let has = one(
                    graph.ops(edge.target)?.into_iter().filter(|op| {
                        op.origin == Origin::Source(statement.span())
                            && matches!(op.operation, Operation::MapIterationHasNext { .. })
                    }),
                    "temporary Entry actual header HasNext",
                )?;
                need(
                    matches!(has.operation, Operation::MapIterationHasNext { source, cursor }
                        if EntityId::Loan(source) == header_lease
                            && EntityId::Value(cursor) == header_cursor)
                        && edge
                            .arguments
                            .iter()
                            .filter(|&&id| id == EntityId::Value(owner))
                            .count()
                            == 1
                        && edge
                            .arguments
                            .iter()
                            .filter(|&&id| id == EntityId::Loan(lease))
                            .count()
                            == 1,
                    "temporary Entry exact owner/lease/cursor columns and all owner predecessors",
                    (edge, header_owner, has),
                )?;
            }
        }
        need(
            selected > 0,
            "temporary Entry original source has an actual selected provider",
            statement.span(),
        )?;
    }
    Ok(())
}
