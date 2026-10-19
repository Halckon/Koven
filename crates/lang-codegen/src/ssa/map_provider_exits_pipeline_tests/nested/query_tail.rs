//! The actual post-loop/caller size Ifs, including both incoming join edges.
use super::super::{
    fixture::{Failure, contains, failure, need},
    query_graph::{Graph, result, value},
    query_inputs::one,
};
use super::{
    ast::Input,
    query_graph::{Roles, Witness},
};
use crate::ssa::model::*;
use lang_frontend::{parser::Expression, source::Span};

pub(super) fn size<'a>(
    graph: &Graph<'a>,
    span: Span,
    owner: EntityId,
    witness: &mut Witness,
) -> Result<&'a Instruction, Failure> {
    let i = one(
        graph.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(span) && matches!(i.operation, Operation::MapSize { .. })
        }),
        "nested SSA actual size instruction",
    )?;
    need(
        i.operation == Operation::MapSize { owner },
        "nested SSA size exact current source",
        i,
    )?;
    witness.uses.permit(owner, i);
    Ok(i)
}

pub(super) fn conditional(
    graph: &Graph<'_>,
    f: &Input,
    size: &Instruction,
    roles: &Roles,
    witness: &mut Witness,
) -> Result<(BlockId, Roles), Failure> {
    let span = size.origin.span();
    let control = one(
        f.parsed.ast().expressions().iter().filter_map(|(_, node)| {
            let Expression::If {
                condition,
                then_branch,
                else_span: None,
                else_branch: None,
                ..
            } = node.payload()
            else {
                return None;
            };
            let condition_node = f.parsed.ast().expressions().get(*condition).ok()?;
            contains(condition_node.span(), span).then_some((
                node.span(),
                condition_node.span(),
                *then_branch,
            ))
        }),
        "nested SSA actual AST size If",
    )?;
    let term = graph.term(size.block)?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &term.kind
    else {
        return Err(failure("nested SSA size actual conditional", term));
    };
    let cmp = graph.definition(EntityId::Value(*condition))?;
    let Operation::Compare {
        operator: ComparisonOperator::Equal,
        left,
        right,
    } = cmp.operation
    else {
        return Err(failure("nested SSA size equality condition", cmp));
    };
    let constant = graph.definition(EntityId::Value(right))?;
    need(
        term.origin == Origin::Source(control.0)
            && cmp.origin == Origin::Source(control.1)
            && cmp.block == size.block
            && left == value(size)?
            && constant.block == size.block
            && contains(control.1, constant.origin.span())
            && matches!(
                constant.operation,
                Operation::Constant(ScalarConstant::Integer(actual))
                    if matches!((f.sources.slice(constant.origin.span()), actual), (Ok("0"), 0) | (Ok("2"), 2))
            )
            && graph.position(size)? < graph.position(cmp)?
            && graph.position(constant)? < graph.position(cmp)?
            && f.sources
                .slice(constant.origin.span())
                .is_ok_and(|s| s == "0" || s == "2"),
        "nested SSA size actual comparison result drives its source If",
        (size, cmp, constant, term),
    )?;
    witness.conditions.insert(size.block, *condition);
    let then = when_true.target;
    let otherwise = when_false.target;
    need(
        then != otherwise && then != size.block && otherwise != size.block,
        "nested SSA distinct size If arms",
        term,
    )?;
    graph.incoming_from(then, &[size.block])?;
    graph.incoming_from(otherwise, &[size.block])?;
    let a = witness.edge(graph, size.block, when_true, roles)?;
    let b = witness.edge(graph, size.block, when_false, roles)?;
    let then_span = f
        .parsed
        .ast()
        .statements()
        .get(control.2)
        .map_err(|e| failure("nested AST then body", e))?
        .span();
    need(
        graph.block(then)?.origin == Origin::Source(then_span)
            && graph.block(otherwise)?.origin == Origin::Source(control.0),
        "nested SSA size arms actual AST origins",
        (graph.block(then)?, graph.block(otherwise)?),
    )?;
    let ea = graph.branch(then)?;
    let eb = graph.branch(otherwise)?;
    need(
        ea.target == eb.target
            && ea.target != then
            && ea.target != otherwise
            && graph.term(then)?.origin == Origin::Source(control.0)
            && graph.term(otherwise)?.origin == Origin::Source(control.0),
        "nested SSA both actual size arms enter same join",
        (ea, eb),
    )?;
    graph.incoming_from(ea.target, &[then, otherwise])?;
    let joined = witness.edge(graph, then, ea, &a)?;
    let other_joined = witness.edge(graph, otherwise, eb, &b)?;
    need(
        joined == other_joined && graph.block(ea.target)?.origin == Origin::Source(control.0),
        "nested SSA all size join incoming preserve identical role columns",
        (&joined, &other_joined),
    )?;
    // All MapSize operations are checked by the caller. Neither If arm acquires provider roles.
    for block in [then, otherwise] {
        need(
            !graph.ops(block)?.iter().any(|i| {
                matches!(
                    i.operation,
                    Operation::DirectCall { .. }
                        | Operation::MapSize { .. }
                        | Operation::SharedReborrow { .. }
                )
            }),
            "nested SSA marker-only size branch",
            graph.ops(block)?,
        )?;
    }
    let _ = result(size)?;
    Ok((ea.target, joined))
}
