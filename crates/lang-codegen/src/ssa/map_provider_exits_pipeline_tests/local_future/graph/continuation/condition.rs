//! Actual size-to-Boolean chain, both original arms and full live-owner merge.
use super::*;

pub(super) fn check(
    q: &Query<'_, '_>,
    site: &Site,
    form: usize,
    sizes: &[&Instruction],
    calls: &[(&Instruction, &Instruction, EntityId)],
    uses: &mut Uses,
) -> Result<(), Failure> {
    let conditions = q
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, n)| {
            let Expression::If {
                condition,
                then_branch,
                else_branch,
                ..
            } = n.payload()
            else {
                return None;
            };
            inside(n.span(), site.scan).then_some((
                n.span(),
                *condition,
                *then_branch,
                *else_branch,
            ))
        })
        .collect::<Vec<_>>();
    if form == 2 {
        return need(
            conditions.is_empty() && sizes.is_empty(),
            "second-For continuation has no invented size If",
            &conditions,
        );
    }
    let [(if_span, condition, yes, Some(no))] = conditions.as_slice() else {
        return Err(failure("actual single continuation If", &conditions));
    };
    let compare_ast = q.parsed.ast().expressions().get(*condition).unwrap();
    let Expression::Binary {
        left,
        operator: lang_frontend::parser::BinaryOperator::Equal,
        right,
        ..
    } = compare_ast.payload()
    else {
        return Err(failure("actual original size equality", compare_ast));
    };
    let zero_ast = q.parsed.ast().expressions().get(*right).unwrap();
    need(
        q.sources.slice(zero_ast.span()) == Ok("0"),
        "actual original equality zero",
        zero_ast,
    )?;
    let [size] = sizes else {
        return Err(failure("exact actual future size", sizes));
    };
    let left = q.parsed.ast().expressions().get(*left).unwrap();
    if form == 0 {
        need(
            left.payload() == &Expression::Name && q.sources.slice(left.span()) == Ok("count"),
            "actual final count condition",
            left,
        )?;
        let init = one(
            q.parsed.ast().items().iter().filter_map(|(_, n)| {
                let Item::Variable {
                    name: NameMarker::Present(name),
                    initializer,
                    ..
                } = n.payload()
                else {
                    return None;
                };
                (inside(n.span(), site.scan) && q.sources.slice(*name) == Ok("count"))
                    .then_some(*initializer)
            }),
            "actual count binding initializer",
        )?;
        need(
            size.origin == Origin::Source(q.parsed.ast().expressions().get(init).unwrap().span()),
            "actual count aliases precise size result",
            size,
        )?;
    } else {
        need(
            size.origin == Origin::Source(left.span())
                && q.sources.slice(left.span()) == Ok("source.size"),
            "actual direct size condition origin",
            size,
        )?;
    }
    let g = q.g;
    let zero = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(zero_ast.span())
                && i.operation == Operation::Constant(ScalarConstant::Integer(0))
        }),
        "actual original zero Constant",
    )?;
    let compare = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(compare_ast.span())
                && i.operation
                    == Operation::Compare {
                        operator: ComparisonOperator::Equal,
                        left: value(size).unwrap(),
                        right: value(zero).unwrap(),
                    }
        }),
        "actual precise size/zero Compare",
    )?;
    need(
        size.block == compare.block
            && zero.block == compare.block
            && g.position(size)? < g.position(zero)?
            && g.position(zero)? < g.position(compare)?,
        "actual size/zero/Compare order",
        compare,
    )?;
    for i in [*size, zero] {
        need(g.function.entity(result(i)?).is_some_and(|e|matches!(e.ty,EntityType::Value(t) if q.module.type_kind(t)==Some(&SsaTypeKind::Integer {bits:32,signed:true}))),"actual Int size/zero storage",i)?;
        uses.protect(result(i)?);
        uses.permit(result(i)?, compare);
    }
    need(g.function.entity(result(compare)?).is_some_and(|e|matches!(e.ty,EntityType::Value(t) if q.module.type_kind(t)==Some(&SsaTypeKind::Boolean))),"actual condition Boolean storage",compare)?;
    let term = g.term(compare.block)?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &term.kind
    else {
        return Err(failure("actual size If Conditional", term));
    };
    need(
        *condition == value(compare)? && term.origin == Origin::Source(*if_span),
        "actual exact If origin and Boolean",
        term,
    )?;
    let bool_entity = result(compare)?;
    need(
        g.instructions
            .iter()
            .all(|i| !i.operation.entities().contains(&bool_entity))
            && g.edges
                .iter()
                .all(|(_, e)| !e.arguments.contains(&bool_entity))
            && g.blocks.iter().all(|&b| {
                b == compare.block || !g.term(b).unwrap().kind.entities().contains(&bool_entity)
            }),
        "complete Boolean uses only precise Conditional",
        compare,
    )?;
    for (target, body, bytes) in [
        (when_true.target, *yes, "zero"),
        (when_false.target, *no, "nonzero"),
    ] {
        g.incoming_from(target, &[compare.block])?;
        let span = q.parsed.ast().statements().get(body).unwrap().span();
        let marker = print(q, span, bytes)?;
        need(
            marker.block == target,
            "actual original zero/nonzero successor",
            marker,
        )?;
    }
    let yes_edge = g.branch(when_true.target)?;
    let no_edge = g.branch(when_false.target)?;
    need(
        yes_edge.target == no_edge.target
            && g.term(when_true.target)?.origin == Origin::Source(*if_span)
            && g.term(when_false.target)?.origin == Origin::Source(*if_span),
        "both actual source arms reach original merge",
        (yes_edge, no_edge),
    )?;
    let merge = yes_edge.target;
    g.incoming_from(merge, &[when_true.target, when_false.target])?;
    if form == 1 {
        let Operation::MapSize { owner } = size.operation else {
            unreachable!()
        };
        let yo = g.transported(when_true, owner)?;
        let no = g.transported(when_false, owner)?;
        let mo = g.transported(yes_edge, yo)?;
        need(
            g.argument(no_edge, mo)? == no && calls[0].0.block == merge && calls[0].2 == mo,
            "future Borrow after live merge uses exact current owner all columns",
            (yo, no, mo, calls[0].0),
        )?;
    } else {
        need(
            [when_true.target, when_false.target, merge]
                .iter()
                .all(|&b| {
                    g.block(b).unwrap().parameters.iter().all(|p| {
                        !matches!(
                            q.module
                                .type_kind(g.function.entity(*p).unwrap().ty.semantic_type()),
                            Some(SsaTypeKind::MapContainer { .. })
                        )
                    })
                }),
            "post-last-size If carries no dead source owner",
            merge,
        )?;
    }
    Ok(())
}
