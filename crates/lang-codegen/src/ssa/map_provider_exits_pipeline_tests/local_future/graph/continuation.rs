//! Actual future Borrow scopes, size condition and final current-owner use.
mod condition;
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
