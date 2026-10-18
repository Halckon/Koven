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
        "nested exits SSA actual size instruction",
    )?;
    need(
        i.operation == Operation::MapSize { owner },
        "nested exits SSA size exact current source",
        i,
    )?;
    witness.uses.permit(owner, i);
    Ok(i)
}

pub(super) fn conditional(
    graph: &Graph<'_>,
    f: &Input,
    size: &Instruction,
    checked_reference: Option<(Span, i128)>,
    roles: &Roles,
    witness: &mut Witness,
) -> Result<(BlockId, Roles), Failure> {
    let span = checked_reference.map_or(size.origin.span(), |(span, _)| span);
    let control = one(
        f.parsed.ast().expressions().iter().filter_map(|(_, node)| {
            let Expression::If {
                condition,
                then_branch,
                else_branch,
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
                *else_branch,
            ))
        }),
        "nested exits SSA actual AST size If",
    )?;
    let term = graph.term(size.block)?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &term.kind
    else {
        return Err(failure("nested exits SSA size actual conditional", term));
    };
    let cmp = graph.definition(EntityId::Value(*condition))?;
    let Operation::Compare {
        operator: ComparisonOperator::Equal,
        left,
        right,
    } = cmp.operation
    else {
        return Err(failure("nested exits SSA size equality condition", cmp));
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
                    if checked_reference.map_or(matches!((f.sources.slice(constant.origin.span()), actual), (Ok("0"), 0) | (Ok("2"), 2)), |(_, expected)| actual == expected && f.sources.slice(constant.origin.span()).is_ok_and(|s| s == expected.to_string()))
            )
            && graph.position(size)? < graph.position(cmp)?
            && graph.position(constant)? < graph.position(cmp)?
            && f.sources.slice(constant.origin.span()).is_ok_and(|s| {
                s == "0" || s == "2" || checked_reference.is_some_and(|(_, n)| s == n.to_string())
            }),
        "nested exits SSA size actual comparison result drives its source If",
        (size, cmp, constant, term),
    )?;
    witness.conditions.insert(size.block, *condition);
    if checked_reference.is_some() {
        witness.uses.permit(result(size)?, cmp);
    }
    let then = when_true.target;
    let otherwise = when_false.target;
    need(
        then != otherwise && then != size.block && otherwise != size.block,
        "nested exits SSA distinct size If arms",
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
            && graph.block(otherwise)?.origin
                == Origin::Source(if let Some(id) = control.3 {
                    f.parsed
                        .ast()
                        .statements()
                        .get(id)
                        .map_err(|e| failure("nested exits AST else body", e))?
                        .span()
                } else {
                    control.0
                }),
        "nested exits SSA size arms actual AST origins",
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
        "nested exits SSA both actual size arms enter same join",
        (ea, eb),
    )?;
    graph.incoming_from(ea.target, &[then, otherwise])?;
    let joined = witness.edge(graph, then, ea, &a)?;
    let other_joined = witness.edge(graph, otherwise, eb, &b)?;
    need(
        joined == other_joined && graph.block(ea.target)?.origin == Origin::Source(control.0),
        "nested exits SSA all size join incoming preserve identical role columns",
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
            "nested exits SSA marker-only size branch",
            graph.ops(block)?,
        )?;
    }
    let _ = result(size)?;
    Ok((ea.target, joined))
}
