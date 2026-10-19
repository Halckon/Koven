//! Future Borrow calls restore Shared loans; Resource current owner survives the size If.
use super::*;
pub(super) struct Last<'p> {
    pub instruction: &'p Instruction,
    pub current: EntityId,
}
pub(super) fn check<'a, 'p>(
    q: &Query<'a, 'p>,
    site: &Site,
    form: usize,
    current: EntityId,
    owners: &BTreeSet<EntityId>,
    uses: &mut Uses,
) -> Result<Last<'p>, Failure> {
    let g = q.g;
    let mut calls = q
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
            (anchors::inside(n.span(), site.scan)
                && q.sources
                    .slice(q.parsed.ast().expressions().get(*callee).ok()?.span())
                    == Ok("readMap"))
            .then_some((n.span(), arguments))
        })
        .collect::<Vec<_>>();
    calls.sort_by_key(|(s, _)| s.start());
    need(
        calls.len() == if form == 0 { 2 } else { 1 },
        "all original future readMap calls",
        &calls,
    )?;
    let reader = function(
        q.module,
        anchors::declaration(q.parsed, q.sources, "readMap")?,
    )?;
    let rg = Graph::new(reader)?;
    let [parameter] = rg
        .block(reader.entry_block().unwrap())?
        .parameters
        .as_slice()
    else {
        return Err(failure("readMap one actual parameter", reader));
    };
    let owner_ty = g.function.entity(current).unwrap().ty.semantic_type();
    need(
        reader.entity(*parameter).unwrap().ty
            == EntityType::Loan {
                kind: LoanKind::Shared,
                target: owner_ty,
            },
        "actual readMap Shared Map ABI",
        parameter,
    )?;
    let reader_size = one(
        rg.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::MapSize { owner: *parameter }),
        "readMap actually reads its Borrow parameter size",
    )?;
    let size_span = match reader_size.origin {
        Origin::Source(s) => s,
        _ => return Err(failure("readMap original size origin", reader_size)),
    };
    need(
        q.sources.slice(size_span) == Ok("source.size")
            && !rg.instructions.iter().any(|i| matches!(i.operation, Operation::Drop {owner} if reader.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()==owner_ty)),
        "actual Borrow reader observes without consuming source",
        reader.id,
    )?;
    let mut completed = Vec::new();
    for (span, args) in calls {
        need(
            matches!(args.as_slice(),[a] if q.sources.slice(q.parsed.ast().expressions().get(a.value).unwrap().span())==Ok("source")),
            "original future Borrow reads named root",
            args,
        )?;
        let call = one(
            g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(span)
                    && matches!(i.operation, Operation::DirectCall { .. })
            }),
            "actual future readMap Call",
        )?;
        let Operation::DirectCall {
            callee,
            receiver: None,
            arguments,
        } = &call.operation
        else {
            return Err(failure("actual future direct ABI", call));
        };
        let [arg @ EntityId::Loan(loan)] = arguments.as_slice() else {
            return Err(failure("actual future Shared loan argument", call));
        };
        need(
            *callee == reader.id
                && g.function.entity(*arg).unwrap().ty == reader.entity(*parameter).unwrap().ty,
            "original readMap selected with exact represented loan",
            call,
        )?;
        let birth = g.definition(*arg)?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = birth.operation
        else {
            return Err(failure("future original Shared loan birth", birth));
        };
        let root = g.definition(EntityId::Place(place))?;
        need(
            root.operation
                == Operation::RootPlace {
                    owner: match current {
                        EntityId::Value(v) => v,
                        _ => return Err(failure("future current Value", current)),
                    },
                }
                && owners.contains(&current),
            "first and every future Borrow uses exact last-provider current owner",
            root,
        )?;
        let end = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: *loan }),
            "future Borrow actual unique End",
        )?;
        need(
            root.block == call.block
                && birth.block == call.block
                && end.block == call.block
                && g.position(root)? < g.position(birth)?
                && g.position(birth)? < g.position(call)?
                && g.position(call)? < g.position(end)?,
            "future Root/Borrow/Call/End complete scope order",
            (root, birth, call, end),
        )?;
        uses.permit(current, root);
        uses.protect(EntityId::Place(place));
        uses.permit(EntityId::Place(place), birth);
        uses.protect(*arg);
        uses.permit(*arg, call);
        uses.permit(*arg, end);
        need(
            g.edges.iter().all(|(_, edge)| {
                !edge.arguments.contains(arg) && !edge.arguments.contains(&EntityId::Place(place))
            }),
            "completed future Borrow exports no loan/place into later control flow",
            call,
        )?;
        completed.push((call, end));
    }
    let sizes = g
        .instructions
        .iter()
        .copied()
        .filter(|i| matches!(i.operation, Operation::MapSize { .. }))
        .collect::<Vec<_>>();
    if form == 2 {
        need(
            sizes.is_empty(),
            "repeat continuation has only final Borrow",
            &sizes,
        )?;
        return Ok(Last {
            instruction: completed[0].1,
            current,
        });
    }
    let [size] = sizes.as_slice() else {
        return Err(failure("exact final scanner size", &sizes));
    };
    need(
        size.operation == Operation::MapSize { owner: current }
            && matches!(size.origin,Origin::Source(span) if anchors::inside(span,site.scan) && q.sources.slice(span)==Ok("source.size")),
        "final size uses same exact current owner as all Borrow calls",
        size,
    )?;
    need(
        completed.iter().all(|(_, end)| {
            end.block == size.block && g.position(end).unwrap() < g.position(size).unwrap()
        }) && g.position(completed[0].1)? < g.position(completed[1].0)?,
        "both actual Borrow Ends restore source before final size",
        &completed,
    )?;
    uses.permit(current, size);
    Ok(Last {
        instruction: size,
        current: condition(q, site, size, current, uses)?,
    })
}

fn condition(
    q: &Query<'_, '_>,
    site: &Site,
    size: &Instruction,
    current: EntityId,
    uses: &mut Uses,
) -> Result<EntityId, Failure> {
    let (span, condition, yes, no) = one(
        q.parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(_, n)| match n.payload() {
                Expression::If {
                    condition,
                    then_branch,
                    else_branch: Some(no),
                    ..
                } if anchors::inside(n.span(), site.scan) => {
                    Some((n.span(), *condition, *then_branch, *no))
                }
                _ => None,
            }),
        "actual post-last-use scanner If",
    )?;
    let ast = q.parsed.ast().expressions().get(condition).unwrap();
    let Expression::Binary {
        left,
        operator: lang_frontend::parser::BinaryOperator::Equal,
        right,
        ..
    } = ast.payload()
    else {
        return Err(failure("original count equality", ast));
    };
    let left = q.parsed.ast().expressions().get(*left).unwrap();
    need(
        left.payload() == &Expression::Name && q.sources.slice(left.span()) == Ok("count"),
        "actual count reference",
        left,
    )?;
    let init = one(
        q.parsed
            .ast()
            .items()
            .iter()
            .filter_map(|(_, n)| match n.payload() {
                Item::Variable {
                    name: NameMarker::Present(name),
                    initializer,
                    ..
                } if anchors::inside(n.span(), site.scan)
                    && q.sources.slice(*name) == Ok("count") =>
                {
                    Some(*initializer)
                }
                _ => None,
            }),
        "actual count declaration initializer",
    )?;
    need(
        size.origin == Origin::Source(q.parsed.ast().expressions().get(init).unwrap().span()),
        "count formed by precise original size",
        size,
    )?;
    let zero_span = q.parsed.ast().expressions().get(*right).unwrap().span();
    need(
        q.sources.slice(zero_span) == Ok("0"),
        "original count compares zero",
        zero_span,
    )?;
    let g = q.g;
    let zero = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(zero_span)
                && i.operation == Operation::Constant(ScalarConstant::Integer(0))
        }),
        "actual equality zero",
    )?;
    let compare = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(ast.span())
                && i.operation
                    == Operation::Compare {
                        operator: ComparisonOperator::Equal,
                        left: value(size).unwrap(),
                        right: value(zero).unwrap(),
                    }
        }),
        "actual count size/zero Compare",
    )?;
    need(
        size.block == zero.block
            && zero.block == compare.block
            && g.position(size)? < g.position(zero)?
            && g.position(zero)? < g.position(compare)?,
        "actual size/zero/compare order",
        compare,
    )?;
    for i in [size, zero] {
        need(g.function.entity(result(i)?).is_some_and(|e|matches!(e.ty,EntityType::Value(t) if q.module.type_kind(t)==Some(&SsaTypeKind::Integer {bits:32,signed:true}))),"actual size/zero Int storage",i)?;
        uses.protect(result(i)?);
        uses.permit(result(i)?, compare);
    }
    let term = g.term(compare.block)?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &term.kind
    else {
        return Err(failure("actual original If Conditional", term));
    };
    need(*condition==value(compare)? && term.origin==Origin::Source(span) && g.function.entity(result(compare)?).is_some_and(|e|matches!(e.ty,EntityType::Value(t) if q.module.type_kind(t)==Some(&SsaTypeKind::Boolean))),"actual original Boolean If identity",term)?;
    let ce = result(compare)?;
    need(
        g.instructions
            .iter()
            .all(|i| !i.operation.entities().contains(&ce))
            && g.edges.iter().all(|(_, e)| !e.arguments.contains(&ce))
            && g.blocks
                .iter()
                .all(|&b| b == compare.block || !g.term(b).unwrap().kind.entities().contains(&ce)),
        "Boolean only selects original Conditional",
        compare,
    )?;
    for (target, body, wanted) in [
        (when_true.target, yes, "zero"),
        (when_false.target, no, "nonzero"),
    ] {
        g.incoming_from(target, &[compare.block])?;
        let marker = anchors::print(
            q,
            q.parsed.ast().statements().get(body).unwrap().span(),
            wanted,
        )?;
        need(
            marker.block == target,
            "actual original empty/nonempty successor marker",
            marker,
        )?;
    }
    let yes = g.branch(when_true.target)?;
    let no = g.branch(when_false.target)?;
    need(
        yes.target == no.target
            && g.term(when_true.target)?.origin == Origin::Source(span)
            && g.term(when_false.target)?.origin == Origin::Source(span),
        "actual original If merge",
        (yes, no),
    )?;
    g.incoming_from(yes.target, &[when_true.target, when_false.target])?;
    let yo = g.transported(when_true, current)?;
    let no_owner = g.transported(when_false, current)?;
    let merged = g.transported(yes, yo)?;
    need(
        g.argument(no, merged)? == no_owner,
        "Resource live current owner retained through both post-last-use arms",
        (yo, no_owner, merged),
    )?;
    Ok(merged)
}
