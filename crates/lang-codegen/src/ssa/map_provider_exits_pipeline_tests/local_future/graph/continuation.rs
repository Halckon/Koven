//! Actual future Borrow scopes, size condition and final current-owner use.
mod condition {
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
                size.origin
                    == Origin::Source(q.parsed.ast().expressions().get(init).unwrap().span()),
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
}
use super::*;

pub(super) struct Last<'a> {
    pub instruction: &'a Instruction,
    pub owner: EntityId,
}
pub(super) fn check<'a, 'p>(
    q: &Query<'a, 'p>,
    site: &Site,
    form: usize,
    expected_current: EntityId,
    owners: &BTreeSet<EntityId>,
    uses: &mut Uses,
) -> Result<Last<'p>, Failure> {
    let g = q.g;
    let mut ast_calls = q
        .parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, n)| {
            let Expression::Call {
                callee, arguments, ..
            } = n.payload()
            else {
                return None;
            };
            (inside(n.span(), site.scan)
                && q.sources
                    .slice(q.parsed.ast().expressions().get(*callee).unwrap().span())
                    == Ok("readMap"))
            .then_some((n.span(), arguments))
        })
        .collect::<Vec<_>>();
    ast_calls.sort_by_key(|(span, _)| span.start());
    need(
        ast_calls.len() == if form == 0 { 2 } else { 1 },
        "actual original future Borrow call count",
        &ast_calls,
    )?;
    let decl=one(q.parsed.roots().iter().filter_map(|id|{let n=q.parsed.ast().items().get(*id).ok()?;matches!(n.payload(),Item::Function {name:NameMarker::Present(name),..} if q.sources.slice(*name)==Ok("readMap")).then_some(n.span())}),"original actual future Borrow declaration")?;
    let callee = one(
        q.module
            .functions
            .iter()
            .filter(|f| f.origin == Origin::Source(decl)),
        "actual reachable future Borrow callee",
    )?;
    let mut calls = vec![];
    for (span, args) in ast_calls {
        need(
            matches!(args.as_slice(),[a] if q.sources.slice(q.parsed.ast().expressions().get(a.value).unwrap().span())==Ok("source")),
            "original future Borrow source argument",
            args,
        )?;
        let call = one(
            g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(span)
                    && matches!(i.operation, Operation::DirectCall { .. })
            }),
            "actual original future Borrow instruction",
        )?;
        let Operation::DirectCall {
            callee: actual,
            receiver: None,
            arguments,
        } = &call.operation
        else {
            return Err(failure("actual future Borrow ABI", call));
        };
        let [arg @ EntityId::Loan(loan)] = arguments.as_slice() else {
            return Err(failure("actual future Borrow single loan argument", call));
        };
        need(
            *actual == callee.id
                && callee
                    .block(callee.entry_block().unwrap())
                    .unwrap()
                    .parameters
                    .len()
                    == 1
                && callee
                    .entity(
                        callee
                            .block(callee.entry_block().unwrap())
                            .unwrap()
                            .parameters[0],
                    )
                    .unwrap()
                    .ty
                    == g.function.entity(*arg).unwrap().ty,
            "selected future Borrow callee/storage ABI",
            call,
        )?;
        let birth = g.definition(*arg)?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = birth.operation
        else {
            return Err(failure("actual future Borrow scoped loan", birth));
        };
        let root = g.definition(EntityId::Place(place))?;
        let Operation::RootPlace { owner } = root.operation else {
            return Err(failure("actual future Borrow current source root", root));
        };
        need(
            owners.contains(&EntityId::Value(owner)),
            "future Borrow preserves original factory owner identity",
            root,
        )?;
        let end = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: *loan }),
            "actual future Borrow loan End",
        )?;
        need(
            root.block == call.block
                && birth.block == call.block
                && end.block == call.block
                && g.position(root)? < g.position(birth)?
                && g.position(birth)? < g.position(call)?
                && g.position(call)? < g.position(end)?,
            "actual future Root/loan/call/End order",
            (root, birth, call, end),
        )?;
        uses.permit(EntityId::Value(owner), root);
        uses.protect(EntityId::Place(place));
        uses.permit(EntityId::Place(place), birth);
        uses.protect(*arg);
        uses.permit(*arg, call);
        uses.permit(*arg, end);
        calls.push((call, end, EntityId::Value(owner)));
    }
    let sizes = g
        .instructions
        .iter()
        .copied()
        .filter(|i| matches!(i.operation, Operation::MapSize { .. }))
        .collect::<Vec<_>>();
    for size in &sizes {
        let Operation::MapSize { owner } = size.operation else {
            unreachable!()
        };
        need(
            owners.contains(&owner)
                && matches!(size.origin,Origin::Source(span) if q.sources.slice(span)==Ok("source.size") && inside(span,site.scan)),
            "actual future size on current original owner",
            size,
        )?;
        uses.permit(owner, size);
    }
    match form {
        0 => need(
            calls[0].2 == expected_current,
            "first future Borrow uses exact provider continuation current",
            calls[0].0,
        )?,
        1 => {
            let [size] = sizes.as_slice() else {
                return Err(failure("first future size use", &sizes));
            };
            need(
                size.operation
                    == Operation::MapSize {
                        owner: expected_current,
                    },
                "first future size uses exact provider continuation current",
                size,
            )?;
        }
        _ => need(
            calls[0].2 == expected_current,
            "final Borrow uses exact second For continuation current",
            calls[0].0,
        )?,
    }
    condition::check(q, site, form, &sizes, &calls, uses)?;
    let last_loop = site.loops.last().unwrap();
    let guard = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(last_loop.span)
                && matches!(i.operation, Operation::MapIterationHasNext { .. })
        }),
        "actual last provider HasNext",
    )?;
    let TerminatorKind::Conditional { when_false, .. } = &g.term(guard.block)?.kind else {
        unreachable!()
    };
    need(
        calls
            .iter()
            .all(|(call, _, _)| reaches(g, when_false.target, call.block, true)),
        "all future calls reachable after actual final provider false edge",
        &calls,
    )?;
    if form == 0 {
        let [size] = sizes.as_slice() else {
            return Err(failure("actual final size-only source use", &sizes));
        };
        need(
            calls.iter().all(|(_, end, _)| {
                end.block == size.block && g.position(end).unwrap() < g.position(size).unwrap()
            }) && g.position(calls[0].1)? < g.position(calls[1].0)?,
            "both Borrow ABI Ends precede actual final size",
            &calls,
        )?;
        let Operation::MapSize { owner } = size.operation else {
            unreachable!()
        };
        need(
            calls.iter().all(|(_, _, current)| *current == owner),
            "sequential calls and final size use same current owner",
            owner,
        )?;
        Ok(Last {
            instruction: size,
            owner,
        })
    } else {
        need(
            sizes.len() == usize::from(form == 1),
            "actual final-Borrow size count",
            &sizes,
        )?;
        let (call, end, owner) = calls[0];
        if form == 2 {
            let howner = one(
                g.block(guard.block)?
                    .parameters
                    .iter()
                    .copied()
                    .filter(|p| owners.contains(p)),
                "last provider actual header owner column",
            )?;
            let false_owner = g.transported(when_false, howner)?;
            need(
                owner == false_owner && call.block == when_false.target,
                "final Borrow uses exact second-provider exhausted owner",
                (owner, false_owner, call),
            )?;
        }
        Ok(Last {
            instruction: end,
            owner,
        })
    }
}
