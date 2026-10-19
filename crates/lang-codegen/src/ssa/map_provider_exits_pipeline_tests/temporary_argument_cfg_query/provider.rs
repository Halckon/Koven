//! Existing normal provider consumes the real post-argument-CFG owner, with every role explicit.
use super::*;
use lang_frontend::parser::{Item, NameMarker};
use std::collections::BTreeMap;
fn via(
    g: &Graph<'_>,
    edge: &Edge,
    frame: [EntityId; 3],
    flag: Option<EntityId>,
) -> Result<([EntityId; 3], Option<EntityId>), Failure> {
    let expected = frame.into_iter().chain(flag).collect::<BTreeSet<_>>();
    need(
        edge.arguments.len() == expected.len()
            && edge.arguments.iter().copied().collect::<BTreeSet<_>>() == expected,
        "complete actual owner/source/cursor and original flag edge",
        (edge, frame, flag),
    )?;
    Ok((
        [
            g.transported(edge, frame[0])?,
            g.transported(edge, frame[1])?,
            g.transported(edge, frame[2])?,
        ],
        flag.map(|e| g.transported(edge, e)).transpose()?,
    ))
}
#[allow(clippy::too_many_arguments)]
pub(super) fn check<'p>(
    sources: &SourceMap,
    parsed: &ParsedFile,
    input: &ast::Input,
    site: &ast::Loop,
    module: &Module,
    g: &Graph<'p>,
    call: &Instruction,
    argument_end: &Instruction,
    drop: &Instruction,
    owners: &BTreeSet<EntityId>,
    uses: &mut Uses,
) -> Result<&'p Instruction, Failure> {
    let owner = value(call)?;
    let root = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::RootPlace { owner }),
        "provider actual sole returned-owner root",
    )?;
    let EntityId::Place(place) = result(root)? else {
        return Err(failure("provider actual root place", root));
    };
    let begin = one(
        g.instructions.iter().copied().filter(|i| {
            i.operation
                == Operation::BorrowBegin {
                    place,
                    kind: LoanKind::Shared,
                }
        }),
        "provider actual source lease",
    )?;
    let first = at(g, site.span, |o| {
        matches!(o, Operation::MapIterationFirst { .. })
    })?;
    let initial = [EntityId::Value(owner), result(begin)?, result(first)?];
    need(
        first.operation
            == Operation::MapIterationFirst {
                source: loan(begin)?,
            }
            && root.origin == Origin::Source(site.span)
            && begin.origin == root.origin
            && [root, begin, first].iter().all(|i| i.block == call.block)
            && g.position(argument_end)? < g.position(root)?
            && g.position(root)? < g.position(begin)?
            && g.position(begin)? < g.position(first)?,
        "argument CallReturn cleanup before real returned-owner acquisition in actual merge preheader",
        (argument_end, root, begin, first),
    )?;
    uses.permit(initial[0], root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), begin);
    uses.permit(initial[1], first);
    let parameter = one(
        g.block(g.function.entry_block().unwrap())?
            .parameters
            .iter()
            .copied(),
        "original scanner flag ABI parameter",
    )?;
    let flags = family(g, parameter)?;
    let current_flags = g
        .block(call.block)?
        .parameters
        .iter()
        .copied()
        .filter(|e| flags.contains(e))
        .collect::<Vec<_>>();
    // Single keeps its immutable entry loan outside the provider; Unit's ordinary
    // argument CFG rebinds it and carries that exact current column through the loop.
    let initial_flag = match current_flags.as_slice() {
        [] => None,
        [flag] => Some(*flag),
        _ => {
            return Err(failure(
                "one actual current flag column at argument merge",
                current_flags,
            ));
        }
    };
    let entry = g.branch(first.block)?;
    let (header, header_flag) = via(g, entry, initial, initial_flag)?;
    let has = at(g, site.span, |o| {
        matches!(o, Operation::MapIterationHasNext { .. })
    })?;
    need(
        has.block == entry.target
            && has.operation.entities() == [header[1], header[2]]
            && g.ops(has.block)?
                .as_slice()
                .iter()
                .map(|i| i.id)
                .collect::<Vec<_>>()
                == [has.id],
        "actual sole current HasNext",
        has,
    )?;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(has.block)?.kind
    else {
        return Err(failure("actual provider Conditional", has));
    };
    need(
        *condition == value(has)? && g.term(has.block)?.origin == Origin::Source(site.span),
        "actual HasNext condition identity",
        has,
    )?;
    let (body, body_flag) = via(g, when_true, header, header_flag)?;
    let (exit, exit_flag) = via(g, when_false, header, header_flag)?;
    let next = body_query(
        sources,
        parsed,
        site,
        module,
        g,
        when_true.target,
        body,
        uses,
    )?;
    let back = g.branch(when_true.target)?;
    need(
        back.target == has.block
            && via(g, back, [body[0], body[1], result(next)?], body_flag)? == (header, header_flag),
        "every normal backedge role preserves exact current column",
        back,
    )?;
    g.incoming_from(has.block, &[first.block, when_true.target])?;
    g.incoming_from(when_true.target, &[has.block])?;
    g.incoming_from(when_false.target, &[has.block])?;
    for flag in [initial_flag, header_flag, body_flag, exit_flag]
        .into_iter()
        .flatten()
    {
        need(
            flags.contains(&flag),
            "extra provider column is original flag family",
            flag,
        )?;
        g.anchored(flag, &[parameter])?;
        uses.protect(flag);
    }
    let map = g.function.entity(initial[0]).unwrap().ty.semantic_type();
    for frame in [initial, header, body, exit] {
        let cursor_type = g.function.entity(frame[2]).unwrap().ty;
        let cursor_is_u64 = matches!(cursor_type, EntityType::Value(t)
            if module.type_kind(t) == Some(&SsaTypeKind::Integer { bits: 64, signed: false }));
        need(
            owners.contains(&frame[0])
                && g.function.entity(frame[0]).unwrap().ty == EntityType::Value(map)
                && g.function.entity(frame[1]).unwrap().ty
                    == EntityType::Loan {
                        kind: LoanKind::Shared,
                        target: map,
                    }
                && cursor_is_u64,
            "every current provider role has exact represented type",
            frame,
        )?;
        g.anchored(frame[0], &[initial[0]])?;
        g.anchored(frame[1], &[initial[1]])?;
        g.anchored(frame[2], &[initial[2], result(next)?])?;
        for e in frame {
            uses.protect(e);
        }
    }
    uses.permit(header[1], has);
    uses.permit(header[2], has);
    let source_end = one(
        g.instructions.iter().copied().filter(
            |i| matches!(i.operation,Operation::BorrowEnd {loan} if EntityId::Loan(loan)==exit[1]),
        ),
        "actual exhausted source End",
    )?;
    let ops = g.ops(when_false.target)?;
    need(
        source_end.block == when_false.target
            && source_end.origin == Origin::Source(site.span)
            && drop.block == source_end.block
            && drop.origin == Origin::Source(input.call)
            && matches!(drop.operation,Operation::Drop {owner} if EntityId::Value(owner)==exit[0])
            && ops.first().map(|i| i.id) == Some(source_end.id)
            && ops.get(1).map(|i| i.id) == Some(drop.id)
            && matches!(&g.term(drop.block)?.kind,TerminatorKind::Return {values} if values.is_empty()),
        "false-edge source End then exact current hidden owner Drop then scanner Return",
        (source_end, drop),
    )?;
    uses.permit(exit[1], source_end);
    uses.permit(exit[0], drop);
    let condition_entity = result(has)?;
    need(
        g.instructions
            .iter()
            .all(|i| !i.operation.entities().contains(&condition_entity))
            && g.edges
                .iter()
                .all(|(_, edge)| !edge.arguments.contains(&condition_entity))
            && g.blocks.iter().all(|&b| {
                b == has.block
                    || !g
                        .term(b)
                        .unwrap()
                        .kind
                        .entities()
                        .contains(&condition_entity)
            }),
        "HasNext result only used by original guard",
        has,
    )?;
    need(
        g.instructions
            .iter()
            .filter(|i| iteration(&i.operation))
            .count()
            == 5,
        "all First/HasNext/K/V/Next operations accounted",
        g.function.id,
    )?;
    // No provider source/cursor is transported beyond its original three-role edges.
    let leases = family(g, initial[1])?;
    let cursors = [initial[2], header[2], body[2], exit[2], result(next)?]
        .into_iter()
        .collect::<BTreeSet<_>>();
    for (from, edge) in &g.edges {
        for e in &edge.arguments {
            if leases.contains(e) || cursors.contains(e) {
                need(
                    (*from == first.block && edge.target == has.block)
                        || (*from == has.block
                            && [when_true.target, when_false.target].contains(&edge.target))
                        || (*from == when_true.target && edge.target == has.block),
                    "provider state never escapes its exact live edges",
                    (from, edge, e),
                )?;
            }
        }
    }
    Ok(source_end)
}

#[allow(clippy::too_many_arguments)]
fn body_query<'p>(
    sources: &SourceMap,
    parsed: &ParsedFile,
    site: &ast::Loop,
    module: &Module,
    g: &Graph<'p>,
    block: BlockId,
    frame: [EntityId; 3],
    uses: &mut Uses,
) -> Result<&'p Instruction, Failure> {
    let reader_span = one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            let Item::Function {
                name: NameMarker::Present(name),
                ..
            } = n.payload()
            else {
                return None;
            };
            (sources.slice(*name) == Ok("read")).then_some(n.span())
        }),
        "original reader declaration",
    )?;
    let reader = function(module, reader_span)?;
    let key = at(g, site.span, |o| {
        matches!(o, Operation::MapIterationKeyLoan { .. })
    })?;
    let val = at(g, site.span, |o| {
        matches!(o, Operation::MapIterationValueLoan { .. })
    })?;
    let next = at(g, site.span, |o| {
        matches!(o, Operation::MapIterationNext { .. })
    })?;
    let args = [loan(key)?, loan(val)?];
    need(
        [key, val, next]
            .iter()
            .all(|i| i.block == block && i.operation.entities() == [frame[1], frame[2]])
            && reader.return_types.is_empty(),
        "body provider uses exact current source and cursor",
        (key, val, next),
    )?;
    let params = &reader
        .block(reader.entry_block().unwrap())
        .unwrap()
        .parameters;
    let shared_string_parameters = args.iter().zip(params).all(|(&l, &p)| {
        let ty = g.function.entity(EntityId::Loan(l)).unwrap().ty;
        matches!(ty, EntityType::Loan { kind: LoanKind::Shared, target }
            if module.type_kind(target) == Some(&SsaTypeKind::StringOwner))
            && reader.entity(p).unwrap().ty == ty
    });
    need(
        params.len() == 2 && shared_string_parameters,
        "original reader has exact Shared String K/V ABI",
        reader.id,
    )?;
    let mut lineage = BTreeMap::from([(args[0], (0usize, None)), (args[1], (1usize, None))]);
    let mut live = BTreeSet::new();
    let mut ended = Vec::new();
    let mut called = false;
    for i in g.ops(block)? {
        match &i.operation {
            Operation::MapIterationKeyLoan { .. } | Operation::MapIterationValueLoan { .. } => {
                let l = loan(i)?;
                need(
                    args.contains(&l) && live.insert(l),
                    "actual component birth once",
                    i,
                )?;
                uses.protect(EntityId::Loan(l));
                uses.permit(frame[1], i);
                uses.permit(frame[2], i);
            }
            Operation::SharedReborrow { source } => {
                let (slot, _) = *lineage
                    .get(source)
                    .ok_or_else(|| failure("actual reader reborrow parent", i))?;
                let child = loan(i)?;
                need(
                    live.contains(source)
                        && i.origin == Origin::Source(site.arguments[slot])
                        && g.function.entity(EntityId::Loan(child)).unwrap().ty
                            == g.function.entity(EntityId::Loan(*source)).unwrap().ty
                        && lineage.insert(child, (slot, Some(*source))).is_none()
                        && live.insert(child),
                    "real ordered typed child loan",
                    i,
                )?;
                uses.permit(EntityId::Loan(*source), i);
                uses.protect(EntityId::Loan(child));
            }
            Operation::DirectCall {
                callee,
                receiver,
                arguments,
            } => {
                need(
                    !called
                        && *callee == reader.id
                        && receiver.is_none()
                        && i.origin == Origin::Source(site.read)
                        && arguments.len() == 2
                        && i.results.is_empty(),
                    "actual source read Call exactly once",
                    i,
                )?;
                for (slot, e) in arguments.iter().enumerate() {
                    let EntityId::Loan(l) = *e else {
                        return Err(failure("reader actual loan argument", e));
                    };
                    need(
                        live.contains(&l) && lineage.get(&l).is_some_and(|(n, _)| *n == slot),
                        "actual K/V caller lineage order",
                        e,
                    )?;
                    uses.permit(*e, i);
                }
                called = true;
            }
            Operation::BorrowEnd { loan } => {
                let (_, parent) = *lineage
                    .get(loan)
                    .ok_or_else(|| failure("ended actual component loan", i))?;
                need(
                    called
                        && live.contains(loan)
                        && !lineage
                            .iter()
                            .any(|(child, (_, p))| *p == Some(*loan) && live.contains(child))
                        && i.origin
                            == Origin::Source(if parent.is_some() {
                                site.read
                            } else {
                                site.span
                            }),
                    "actual child End before component End after read",
                    i,
                )?;
                live.remove(loan);
                if parent.is_none() {
                    ended.push(*loan);
                }
                uses.permit(EntityId::Loan(*loan), i);
            }
            Operation::MapIterationNext { .. } => {
                need(
                    i.id == next.id && called && live.is_empty() && ended == [args[1], args[0]],
                    "read and complete reverse component cleanup before Next",
                    i,
                )?;
                uses.permit(frame[1], i);
                uses.permit(frame[2], i);
                uses.protect(result(i)?);
            }
            _ => {
                return Err(failure(
                    "unexpected ordinary normal provider body operation",
                    i,
                ));
            }
        }
    }
    let ops = g.ops(block)?;
    need(
        called
            && live.is_empty()
            && ended == [args[1], args[0]]
            && ops.first().map(|i| i.id) == Some(key.id)
            && ops.last().map(|i| i.id) == Some(next.id)
            && g.position(key)? < g.position(val)?
            && g.edges.iter().all(|(_, e)| {
                e.arguments
                    .iter()
                    .all(|v| !matches!(v,EntityId::Loan(l) if lineage.contains_key(l)))
            }),
        "complete original body has no component loan escape",
        ended,
    )?;
    // The body anchor is taken from the original For, never a coincidentally nearby call.
    need(
        parsed.ast().statements().get(site.body).is_ok_and(|n| {
            n.span().start() < site.read.start() && site.read.end() < n.span().end()
        }),
        "actual read belongs to original For body",
        site.read,
    )?;
    Ok(next)
}
