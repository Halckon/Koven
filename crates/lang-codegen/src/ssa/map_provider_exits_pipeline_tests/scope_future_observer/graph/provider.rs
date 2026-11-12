//! Original own parameter/local flows through every provider column to Return/Move/put.
use crate::test_support::diagnostics::diagnostic;
mod transport {
    //! Exact owner/source/cursor columns over all four real provider edges.
    use super::*;
    use crate::ssa::map_provider_exits_pipeline_tests::query_map_normal_body as body;
    pub(super) struct Carried<'a> {
        pub owner: EntityId,
        pub map_entities: BTreeSet<EntityId>,
        pub pre: BlockId,
        pub header: BlockId,
        pub body: BlockId,
        pub exit: BlockId,
        pub end: &'a Instruction,
    }
    fn via(g: &Graph<'_>, edge: &Edge, roles: [EntityId; 3]) -> Result<[EntityId; 3], Failure> {
        need(
            edge.arguments.len() == 3
                && edge.arguments.iter().copied().collect::<BTreeSet<_>>()
                    == roles.into_iter().collect(),
            "D every complete edge carries exactly current owner/source/cursor",
            (edge, roles),
        )?;
        Ok([
            g.transported(edge, roles[0])?,
            g.transported(edge, roles[1])?,
            g.transported(edge, roles[2])?,
        ])
    }
    /// The exhaustion edge carries owner and source; the refinement layer drops the dead
    /// cursor there, the per-family path carries it too (SPEC-0394).
    fn via_exhausted(
        g: &Graph<'_>,
        edge: &Edge,
        roles: [EntityId; 3],
    ) -> Result<([EntityId; 2], Option<EntityId>), Failure> {
        if edge.arguments.len() == 3 {
            let [owner, source, cursor] = via(g, edge, roles)?;
            return Ok(([owner, source], Some(cursor)));
        }
        need(
            edge.arguments.len() == 2
                && edge.arguments.iter().copied().collect::<BTreeSet<_>>()
                    == [roles[0], roles[1]].into_iter().collect(),
            "D exhaustion edge carries exactly current owner/source",
            (edge, roles),
        )?;
        Ok((
            [
                g.transported(edge, roles[0])?,
                g.transported(edge, roles[1])?,
            ],
            None,
        ))
    }
    pub(super) fn check<'a>(
        g: &Graph<'a>,
        m: &Module,
        p: &ParsedFile,
        s: &ast::Site,
        owner: EntityId,
        uses: &mut Uses,
    ) -> Result<Carried<'a>, Failure> {
        let at = |test: fn(&Operation) -> bool| {
            one(
                g.instructions
                    .iter()
                    .copied()
                    .filter(|op| op.origin == Origin::Source(s.span) && test(&op.operation)),
                "D unique actual original provider operation",
            )
        };
        let root = at(|op| matches!(op, Operation::RootPlace { .. }))?;
        let begin = at(|op| matches!(op, Operation::BorrowBegin { .. }))?;
        let first = at(|op| matches!(op, Operation::MapIterationFirst { .. }))?;
        let has = at(|op| matches!(op, Operation::MapIterationHasNext { .. }))?;
        let next = at(|op| matches!(op, Operation::MapIterationNext { .. }))?;
        let EntityId::Value(seed) = owner else {
            return Err(failure("D source actual owned Value", owner));
        };
        let EntityId::Place(place) = result(root)? else {
            return Err(failure("D provider actual RootPlace result", root));
        };
        let lease = loan(begin)?;
        need(
            root.operation == Operation::RootPlace { owner: seed }
                && begin.operation
                    == Operation::BorrowBegin {
                        place,
                        kind: LoanKind::Shared,
                    }
                && first.operation == Operation::MapIterationFirst { source: lease }
                && [root, begin].iter().all(|op| op.block == first.block)
                && g.position(root)? < g.position(begin)?
                && g.position(begin)? < g.position(first)?,
            "D actual current source Root/Shared lease/First sequence",
            (root, begin, first),
        )?;
        let map = g.function.entity(owner).unwrap().ty.semantic_type();
        let (_, key, val) = m
            .map_container(map)
            .ok_or_else(|| failure("D represented source Map", map))?;
        let initial = [owner, EntityId::Loan(lease), result(first)?];
        let enter = g.branch(first.block)?;
        let header = via(g, enter, initial)?;
        let [_, EntityId::Loan(hs), EntityId::Value(hc)] = header else {
            return Err(failure("D exact Header role kinds", header));
        };
        need(
            has.block == enter.target
                && has.operation
                    == Operation::MapIterationHasNext {
                        source: hs,
                        cursor: hc,
                    }
                && g.ops(has.block)?.len() == 1,
            "D Header guard uses current source/cursor only",
            has,
        )?;
        let TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } = &g.term(has.block)?.kind
        else {
            return Err(failure("D actual HasNext conditional", has));
        };
        need(
            *condition == value(has)?
                && g.term(has.block)?.origin == Origin::Source(s.span)
                && !g.instructions.iter().any(|op| {
                    op.operation
                        .entities()
                        .contains(&EntityId::Value(*condition))
                })
                && g.blocks.iter().all(|&b| {
                    b == has.block
                        || !g
                            .term(b)
                            .unwrap()
                            .kind
                            .entities()
                            .contains(&EntityId::Value(*condition))
                })
                && !when_true.arguments.contains(&EntityId::Value(*condition))
                && !when_false.arguments.contains(&EntityId::Value(*condition)),
            "D HasNext result is used only as its actual guard",
            has,
        )?;
        let current = via(g, when_true, header)?;
        let (exhausted, exhausted_cursor) = via_exhausted(g, when_false, header)?;
        let [_, EntityId::Loan(bs), EntityId::Value(bc)] = current else {
            return Err(failure("D exact body roles", current));
        };
        let reader = function(m, s.reader)?;
        let checked = body::check(
            g,
            reader,
            body::Body {
                block: when_true.target,
                span: s.span,
                read: span(p, s.read),
                arguments: s.arguments.map(|e| span(p, e)),
                types: [key, val],
            },
            bs,
            bc,
            uses,
        )?;
        need(
            checked.id == next.id,
            "D full normal body reader/reborrows/reverse component Ends/Next",
            checked,
        )?;
        let back = g.branch(next.block)?;
        need(
            back.target == has.block
                && via(g, back, [current[0], current[1], result(next)?])? == header,
            "D complete current owner/source/next backedge",
            back,
        )?;
        g.incoming_from(has.block, &[first.block, next.block])?;
        g.incoming_from(next.block, &[has.block])?;
        g.incoming_from(when_false.target, &[has.block])?;
        let EntityId::Loan(es) = exhausted[1] else {
            unreachable!()
        };
        let end = one(
            g.instructions
                .iter()
                .copied()
                .filter(|op| op.operation == Operation::BorrowEnd { loan: es }),
            "D exact current provider lease End",
        )?;
        need(
            end.block == when_false.target
                && end.origin == Origin::Source(s.span)
                && g.ops(end.block)?.first().map(|op| op.id) == Some(end.id),
            "D EndSource is first at actual exhaustion",
            end,
        )?;
        let sources = [initial[1], header[1], current[1], exhausted[1]]
            .into_iter()
            .collect::<BTreeSet<_>>();
        need(g.instructions.iter().filter(|op|matches!(op.operation,Operation::BorrowEnd{loan}if sources.contains(&EntityId::Loan(loan)))).count()==1,"D no stale earlier provider source End",&sources)?;
        let mut role_sets = vec![initial.to_vec(), header.to_vec(), current.to_vec()];
        role_sets.push(exhausted.into_iter().chain(exhausted_cursor).collect());
        for roles in &role_sets {
            g.anchored(roles[0], &[owner])?;
            g.anchored(roles[1], &[initial[1]])?;
            if let Some(&cursor) = roles.get(2) {
                g.anchored(cursor, &[result(first)?, result(next)?])?;
            }
            need(
                g.function.entity(roles[0]).unwrap().ty == EntityType::Value(map)
                    && g.function.entity(roles[1]).unwrap().ty
                        == EntityType::Loan {
                            kind: LoanKind::Shared,
                            target: map,
                        }
                    && roles.get(2).is_none_or(|&cursor| matches!(g.function.entity(cursor).unwrap().ty,EntityType::Value(t)if m.type_kind(t)==Some(&SsaTypeKind::Integer{bits:64,signed:false}))),
                "D exact represented current owner/source/cursor types",
                roles,
            )?;
            for &e in roles {
                uses.protect(e);
            }
        }
        uses.permit(owner, root);
        uses.protect(EntityId::Place(place));
        uses.permit(EntityId::Place(place), begin);
        for op in [first, has, end] {
            for e in op.operation.entities() {
                uses.permit(e, op);
            }
        }
        let mut map_entities = BTreeSet::from([result(root)?]);
        for roles in &role_sets {
            map_entities.extend([roles[0], roles[1]]);
        }
        Ok(Carried {
            owner: exhausted[0],
            map_entities,
            pre: first.block,
            header: has.block,
            body: next.block,
            exit: when_false.target,
            end,
        })
    }
}
use super::super::{ast::span, facts::Proof};
use super::*;
use crate::ssa::map_provider_exits_pipeline_tests::query_graph::{Uses, loan};
pub(super) fn check(
    c: &Case,
    p: &ParsedFile,
    i: &Input,
    proof: &Proof,
    m: &Module,
) -> Result<(), Failure> {
    let f = function(m, i.scan)?;
    let g = Graph::new(f)?;
    let site = ast::site(p, i)?;
    diagnostic!(
        "SCOPE D FUTURE PROVIDER label={} function={f:#?} edges={:#?}",
        c.label,
        g.edges
    );
    need(
        f.receiver.is_none() && f.borrow_return.is_none() && f.carrier_return.is_none(),
        "D scanner ordinary owned ABI",
        f.id,
    )?;
    let entry = g.block(
        f.entry_block()
            .ok_or_else(|| failure("D actual scanner entry", f.id))?,
    )?;
    let seed = if let Some(birth) = i.birth {
        need(
            entry.parameters.is_empty(),
            "D local MutableMap scanner no synthetic parameters",
            entry,
        )?;
        let factory = function(m, i.factory)?;
        let call = one(
            g.instructions.iter().copied().filter(|op| {
                op.origin == Origin::Source(span(p, birth))
                    && op.operation
                        == Operation::DirectCall {
                            callee: factory.id,
                            receiver: None,
                            arguments: vec![],
                        }
            }),
            "D original selected MutableMap birth Call",
        )?;
        let value = value(call)?;
        need(
            call.block == entry.id
                && factory.return_types
                    == [f.entity(EntityId::Value(value)).unwrap().ty.semantic_type()],
            "D actual local factory exact return ABI",
            call,
        )?;
        EntityId::Value(value)
    } else {
        let [EntityId::Value(owner)] = entry.parameters.as_slice() else {
            return Err(failure("D exact own Value ABI entry parameter", entry));
        };
        EntityId::Value(*owner)
    };
    let map = f.entity(seed).unwrap().ty.semantic_type();
    need(
        m.map_container(map).is_some()
            && f.return_types == if c.code == 19 { vec![map] } else { vec![] },
        "D exact Map return or Unit scanner ABI",
        f.id,
    )?;
    let mut uses = Uses::default();
    let carried = transport::check(&g, m, p, &site, seed, &mut uses)?;
    need(
        carried.pre == entry.id,
        "D own/local source enters actual original provider directly",
        carried.pre,
    )?;
    let EntityId::Value(current) = carried.owner else {
        unreachable!()
    };
    let mut anchors = vec![seed];
    let final_owner = if let Some(put) = i.put {
        let op = one(
            g.instructions.iter().copied().filter(|op| {
                op.origin == Origin::Source(span(p, put))
                    && matches!(op.operation, Operation::MapPut { .. })
            }),
            "D actual post-provider put",
        )?;
        let Operation::MapPut {
            owner,
            key,
            value: val,
        } = op.operation
        else {
            unreachable!()
        };
        need(
            owner == current
                && op.block == carried.exit
                && g.position(carried.end)? < g.position(op)?,
            "D future put consumes exact exhausted current owner after lease End",
            op,
        )?;
        super::clones(&g, p, put, op, [key, val])?;
        let updated = value(op)?;
        need(
            updated != current
                && f.entity(EntityId::Value(updated)).unwrap().ty == EntityType::Value(map),
            "D post-put fresh represented owner",
            op,
        )?;
        uses.permit(carried.owner, op);
        anchors.push(EntityId::Value(updated));
        EntityId::Value(updated)
    } else {
        carried.owner
    };
    let returned = if let Some((ret, name)) = i.returned {
        let term = g.term(carried.exit)?;
        need(
            matches!(&term.kind,TerminatorKind::Return{values}if values.as_slice()==[current])
                && term.origin == Origin::Source(span(p, ret))
                && span(p, name).start() > site.span.end()
                && g.ops(carried.exit)?.len() == 1,
            "D actual Return directly delivers exact false-edge current owner after EndSource",
            term,
        )?;
        Some((carried.exit, current))
    } else {
        let (call, arg) = *one(
            i.observes.iter(),
            "D original sole final scanner observation",
        )?;
        let observer = function(m, i.observer)?;
        let observed = one(
            g.instructions.iter().copied().filter(|op| {
                op.origin == Origin::Source(span(p, call))
                    && matches!(op.operation,Operation::DirectCall{callee,..}if callee==observer.id)
            }),
            "D actual final selected observe",
        )?;
        let Operation::DirectCall {
            receiver: None,
            arguments,
            ..
        } = &observed.operation
        else {
            return Err(failure("D final observe ordinary ABI", observed));
        };
        let [EntityId::Loan(observe_loan)] = arguments.as_slice() else {
            return Err(failure("D final observe Borrow argument", observed));
        };
        let begin = g.definition(EntityId::Loan(*observe_loan))?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = begin.operation
        else {
            return Err(failure("D exact final Shared borrow birth", begin));
        };
        let root = g.definition(EntityId::Place(place))?;
        let EntityId::Value(owner) = final_owner else {
            unreachable!()
        };
        let observer_entry = observer.block(observer.entry_block().unwrap()).unwrap();
        need(
            root.operation == Operation::RootPlace { owner }
                && [root, begin, observed]
                    .iter()
                    .all(|op| op.block == carried.exit)
                && root.origin == Origin::Source(span(p, arg))
                && begin.origin == Origin::Source(span(p, arg))
                && g.position(carried.end)? < g.position(root)?
                && g.position(root)? < g.position(begin)?
                && g.position(begin)? < g.position(observed)?
                && observer.receiver.is_none()
                && observer.borrow_return.is_none()
                && observer.carrier_return.is_none()
                && observer.return_types.is_empty()
                && matches!(observer_entry.parameters.as_slice(),[entity]if observer.entity(*entity).unwrap().ty==EntityType::Loan{kind:LoanKind::Shared,target:map}),
            "D successor Name/updated source observes exact current owner with matching Shared ABI",
            (root, begin, observed),
        )?;
        let end = one(
            g.instructions.iter().copied().filter(|op| {
                op.operation
                    == Operation::BorrowEnd {
                        loan: *observe_loan,
                    }
            }),
            "D actual sole final observe End",
        )?;
        let drop = one(
            g.instructions
                .iter()
                .copied()
                .filter(|op| op.operation == Operation::Drop { owner }),
            "D actual current owner unique Drop",
        )?;
        need(
            end.block == carried.exit
                && end.origin == Origin::Source(span(p, call))
                && g.position(end)? == g.position(observed)? + 1
                && drop.block == carried.exit
                && drop.origin
                    == Origin::Source(
                        proof
                            .final_origin
                            .ok_or_else(|| failure("D published final Drop origin", proof))?,
                    )
                && g.position(drop)? == g.position(end)? + 1
                && g.ops(carried.exit)?.last().map(|op| op.id) == Some(drop.id)
                && matches!(&g.term(carried.exit)?.kind,TerminatorKind::Return{values}if values.is_empty()),
            "D CallReturn End then exact current Drop and Unit Return",
            (end, drop),
        )?;
        uses.permit(final_owner, root);
        uses.protect(EntityId::Place(place));
        uses.permit(EntityId::Place(place), begin);
        uses.protect(EntityId::Loan(*observe_loan));
        uses.permit(EntityId::Loan(*observe_loan), observed);
        uses.permit(EntityId::Loan(*observe_loan), end);
        uses.permit(final_owner, drop);
        None
    };
    need(
        g.blocks.iter().copied().collect::<BTreeSet<_>>()
            == BTreeSet::from([carried.pre, carried.header, carried.body, carried.exit]),
        "D complete original provider CFG no hidden path",
        &g.blocks,
    )?;
    let map_drops=g.instructions.iter().filter(|op|matches!(op.operation,Operation::Drop{owner}if m.map_container(f.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())).count();
    need(
        map_drops == usize::from(c.code != 19),
        "D exact Map obligation count after Return/Move/put",
        map_drops,
    )?;
    for &b in &g.blocks {
        for entity in g.block(b)?.parameters.iter().copied().chain(
            g.ops(b)?
                .into_iter()
                .flat_map(|op| op.results.iter().copied()),
        ) {
            if matches!(f.entity(entity).unwrap().ty,EntityType::Value(t)if m.map_container(t).is_some())
            {
                g.anchored(entity, &anchors)?;
                uses.protect(entity);
            }
        }
    }
    if c.code == 19 {
        crate::ssa::map_provider_exits_pipeline_tests::query_inventory::check(
            &g,
            &[map],
            &carried.map_entities,
        )?;
    }
    if c.code != 19 {
        crate::ssa::map_provider_exits_pipeline_tests::local_exits::owner_flow::check(
            &g,
            m,
            &anchors.iter().copied().collect(),
        )?;
    }
    uses.check(&g, returned)
}

pub(in crate::ssa) mod provider_16 {
//! Provider, complete incoming and local Guard proof for one original scanner.
use crate::ssa::map_provider_exits_pipeline_tests::local_exits::graph::*;

pub(in crate::ssa) struct Evidence<'a> {
    pub body: BlockId,
    pub exhausted: BlockId,
    pub ends: Vec<&'a Instruction>,
    pub terminal_owners: BTreeMap<BlockId, EntityId>,
    pub key: &'a Instruction,
    pub val: &'a Instruction,
    pub read: &'a Instruction,
    pub local_drop: &'a Instruction,
}

pub(in crate::ssa) fn check<'a>(
    g: &Graph<'a>,
    module: &Module,
    sources: &SourceMap,
    parsed: &ParsedFile,
    site: (Span, lang_frontend::ast::StatementId, usize),
    owners: &BTreeSet<EntityId>,
    uses: &mut Uses,
) -> Result<Evidence<'a>, Failure> {
    let (span, body, exit) = site;
    let terminal = exit >= 2;
    let broken = exit == 2;
    let continued = exit == 1;
    let op = |p: fn(&Operation) -> bool| {
        one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.origin == Origin::Source(span) && p(&i.operation)),
            "actual original For operation",
        )
    };
    let first = op(|o| matches!(o, Operation::MapIterationFirst { .. }))?;
    let guard = op(|o| matches!(o, Operation::MapIterationHasNext { .. }))?;
    let key = op(|o| matches!(o, Operation::MapIterationKeyLoan { .. }))?;
    let val = op(|o| matches!(o, Operation::MapIterationValueLoan { .. }))?;
    let Operation::MapIterationFirst { source } = first.operation else {
        unreachable!()
    };
    let birth = g.definition(EntityId::Loan(source))?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = birth.operation
    else {
        return Err(failure("provider actual source loan birth", birth));
    };
    let root = g.definition(EntityId::Place(place))?;
    let Operation::RootPlace { owner } = root.operation else {
        return Err(failure("provider actual source root place", root));
    };
    need(
        owners.contains(&EntityId::Value(owner)),
        "every First borrows original current owner",
        root,
    )?;
    uses.permit(EntityId::Value(owner), root);
    uses.protect(EntityId::Place(place));
    uses.permit(EntityId::Place(place), birth);
    let Operation::MapIterationHasNext {
        source: hs,
        cursor: hc,
    } = guard.operation
    else {
        unreachable!()
    };
    let Operation::MapIterationKeyLoan {
        source: bs,
        cursor: bc,
    } = key.operation
    else {
        unreachable!()
    };
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(guard.block)?.kind
    else {
        return Err(failure("actual provider guard branch", guard));
    };
    need(
        *condition == value(guard)?
            && when_true.target == key.block
            && val.block == key.block
            && val.operation
                == Operation::MapIterationValueLoan {
                    source: bs,
                    cursor: bc,
                },
        "actual guard and K/V operands",
        (guard, key, val),
    )?;
    let entry = g.branch(first.block)?;
    need(
        entry.target == guard.block
            && g.transported(entry, EntityId::Loan(source))? == EntityId::Loan(hs)
            && g.transported(entry, result(first)?)? == EntityId::Value(hc)
            && g.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc),
        "all provider entry/header/body columns",
        (entry, when_true),
    )?;
    let header_owner = g.transported(entry, EntityId::Value(owner))?;
    let body_owner = g.transported(when_true, header_owner)?;
    let exhausted_owner = g.transported(when_false, header_owner)?;
    need(
        [header_owner, body_owner, exhausted_owner]
            .iter()
            .all(|entity| owners.contains(entity)),
        "actual owner columns retain original factory anchor",
        (header_owner, body_owner, exhausted_owner),
    )?;
    let mut terminal_owners = BTreeMap::from([(when_false.target, exhausted_owner)]);
    if terminal {
        terminal_owners.insert(key.block, body_owner);
    }
    if terminal {
        g.incoming_from(guard.block, &[first.block])?;
    } else {
        g.incoming_from(guard.block, &[first.block, key.block])?;
    }
    g.incoming_from(key.block, &[guard.block])?;
    g.incoming_from(when_false.target, &[guard.block])?;
    let body_span = parsed.ast().statements().get(body).unwrap().span();
    let transfer = parsed
        .ast()
        .expressions()
        .iter()
        .filter_map(|(_, n)| {
            (inside(n.span(), body_span)
                && matches!(
                    n.payload(),
                    Expression::Continue { .. }
                        | Expression::Break { .. }
                        | Expression::Return { .. }
                ))
            .then_some(n.span())
        })
        .collect::<Vec<_>>();
    need(
        transfer.len() == usize::from(exit != 0),
        "actual original direct transfer count",
        &transfer,
    )?;
    let next_span = if continued { transfer[0] } else { span };
    let next = g
        .instructions
        .iter()
        .copied()
        .filter(|i| {
            i.origin == Origin::Source(next_span)
                && matches!(i.operation, Operation::MapIterationNext { .. })
        })
        .collect::<Vec<_>>();
    need(
        next.len() == usize::from(!terminal),
        "actual Next exists exactly for normal/Continue",
        &next,
    )?;
    let xs = g.transported(when_false, EntityId::Loan(hs))?;
    let xc = g.transported(when_false, EntityId::Value(hc))?;
    let leases = family(g, EntityId::Loan(source))?;
    for &lease in &leases {
        uses.protect(lease);
    }
    let mut ends = vec![];
    for i in &g.instructions {
        if matches!(i.operation,Operation::BorrowEnd {loan} if leases.contains(&EntityId::Loan(loan)))
        {
            ends.push(*i);
        }
    }
    need(
        ends.len() == if terminal { 2 } else { 1 },
        "exact provider source End count",
        &ends,
    )?;
    let exhausted = one(
        ends.iter().copied().filter(|i| {
            i.operation
                == match xs {
                    EntityId::Loan(loan) => Operation::BorrowEnd { loan },
                    _ => unreachable!(),
                }
        }),
        "actual false-edge source End",
    )?;
    need(
        exhausted.block == when_false.target,
        "false source ends in actual exhausted block",
        exhausted,
    )?;
    for &end in &ends {
        if let Operation::BorrowEnd { loan } = end.operation {
            uses.permit(EntityId::Loan(loan), end);
        }
    }
    uses.permit(EntityId::Loan(source), first);
    uses.permit(EntityId::Loan(hs), guard);
    for i in [key, val] {
        uses.permit(EntityId::Loan(bs), i);
    }
    let cursors = BTreeSet::from([result(first)?, EntityId::Value(hc), EntityId::Value(bc), xc]);
    for &cursor in &cursors {
        uses.protect(cursor);
        need(g.function.entity(cursor).is_some_and(|e|matches!(e.ty,EntityType::Value(t) if module.type_kind(t)==Some(&SsaTypeKind::Integer {bits:64,signed:false}))),"actual unsigned provider cursor",cursor)?;
    }
    uses.permit(EntityId::Value(hc), guard);
    for i in [key, val] {
        uses.permit(EntityId::Value(bc), i);
    }
    if !terminal {
        let next = next[0];
        let back = g.branch(key.block)?;
        need(
            next.block == key.block
                && next.operation
                    == Operation::MapIterationNext {
                        source: bs,
                        cursor: bc,
                    }
                && back.target == guard.block
                && g.argument(back, header_owner)? == body_owner
                && g.argument(back, EntityId::Loan(hs))? == EntityId::Loan(bs)
                && g.argument(back, EntityId::Value(hc))? == result(next)?,
            "actual normal/Continue full backedge",
            (next, back),
        )?;
        uses.protect(result(next)?);
        uses.permit(EntityId::Loan(bs), next);
        uses.permit(EntityId::Value(bc), next);
    }
    let read_decl=one(parsed.roots().iter().filter_map(|id|{let n=parsed.ast().items().get(*id).ok()?;matches!(n.payload(),Item::Function{name:NameMarker::Present(name),..} if sources.slice(*name)==Ok("read")).then_some(n.span())}),"original read declaration")?;
    let reader = one(
        module
            .functions
            .iter()
            .filter(|f| f.origin == Origin::Source(read_decl)),
        "actual read callee",
    )?;
    let read=one(g.instructions.iter().copied().filter(|i|i.block==key.block && matches!(&i.operation,Operation::DirectCall {callee,receiver:None,arguments} if *callee==reader.id && arguments.as_slice()==[result(key).unwrap(),result(val).unwrap()])),"actual K/V read selected callee/arguments")?;
    let mut component_ends = vec![];
    for (index, component) in [key, val].into_iter().enumerate() {
        let entity = result(component)?;
        let ty = g.function.entity(entity).unwrap().ty;
        need(
            matches!(ty,EntityType::Loan {kind:LoanKind::Shared,target} if module.type_kind(target)==Some(&SsaTypeKind::StringOwner))
                && reader
                    .block(reader.entry_block().unwrap())
                    .unwrap()
                    .parameters
                    .get(index)
                    .and_then(|p| reader.entity(*p))
                    .is_some_and(|e| e.ty == ty),
            "K/V original Shared String reader ABI",
            component,
        )?;
        let end=one(g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::BorrowEnd {loan} if EntityId::Loan(loan)==entity)),"unique actual K/V End")?;
        need(end.block == key.block, "body ends own K/V", end)?;
        uses.protect(entity);
        uses.permit(entity, read);
        uses.permit(entity, end);
        component_ends.push(end);
    }
    need(
        g.position(read)? < g.position(component_ends[1])?
            && g.position(component_ends[1])? < g.position(component_ends[0])?,
        "actual read/VEnd/KEnd order",
        &component_ends,
    )?;
    let local_drop = guard_drop(
        g,
        module,
        sources,
        parsed,
        body,
        (read, component_ends[1]),
        uses,
    )?;
    if terminal {
        let end = one(
            ends.iter().copied().filter(|i| i.block == key.block),
            "actual Break source End",
        )?;
        need(
            g.position(component_ends[0])? < g.position(end)?,
            "Break KEnd before EndSource",
            end,
        )?;
        if broken {
            need(
                g.branch(key.block)?.target == g.branch(when_false.target)?.target,
                "actual Break/exhaustion continuation merge",
                span,
            )?;
            g.incoming_from(g.branch(key.block)?.target, &[key.block, when_false.target])?;
        }
    } else {
        need(
            g.position(component_ends[0])? < g.position(next[0])?,
            "KEnd before actual Next",
            next[0],
        )?;
    }
    Ok(Evidence {
        body: key.block,
        exhausted: when_false.target,
        ends,
        terminal_owners,
        key,
        val,
        read,
        local_drop,
    })
}

fn guard_drop<'a>(
    g: &Graph<'a>,
    module: &Module,
    sources: &SourceMap,
    parsed: &ParsedFile,
    body: lang_frontend::ast::StatementId,
    read_end: (&Instruction, &Instruction),
    uses: &mut Uses,
) -> Result<&'a Instruction, Failure> {
    let (read, value_end) = read_end;
    let span = parsed.ast().statements().get(body).unwrap().span();
    let (name, initializer) = one(
        parsed.ast().items().iter().filter_map(|(_, node)| {
            let Item::Variable {
                name: NameMarker::Present(name),
                initializer,
                ..
            } = node.payload()
            else {
                return None;
            };
            (inside(node.span(), span) && sources.slice(*name) == Ok("local"))
                .then_some((*name, *initializer))
        }),
        "original body Guard binding",
    )?;
    let origin = parsed.ast().expressions().get(initializer).unwrap().span();
    need(
        sources.slice(origin) == Ok("Guard()"),
        "original Guard construction",
        origin,
    )?;
    let allocation = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(origin)
                && matches!(i.operation, Operation::HeapAllocate { .. })
        }),
        "actual original Guard allocation",
    )?;
    let Operation::HeapAllocate { owner: ty, payload } = allocation.operation else {
        unreachable!()
    };
    let class = one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            let Item::Classifier(c) = n.payload() else {
                return None;
            };
            matches!(c.name,NameMarker::Present(name) if sources.slice(name)==Ok("Guard"))
                .then_some(c)
        }),
        "original Guard class",
    )?;
    let deinit = one(
        class
            .body
            .as_ref()
            .unwrap()
            .members
            .iter()
            .filter_map(|id| parsed.ast().items().get(*id).ok())
            .filter(|n| matches!(n.payload(), Item::Deinit { .. })),
        "original Guard deinit",
    )?;
    need(
        module
            .deinits
            .get(&ty)
            .and_then(|id| module.function(*id))
            .is_some_and(|f| f.origin == Origin::Source(deinit.span())),
        "actual Guard type/deinit identity",
        ty,
    )?;
    let payload = g.definition(EntityId::Value(payload))?;
    need(
        payload.origin == Origin::Source(origin)
            && payload.block == read.block
            && matches!(&payload.operation,Operation::AggregateConstruct {fields,..} if fields.is_empty()),
        "actual Guard empty payload",
        payload,
    )?;
    let owner = value(allocation)?;
    let drop = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::Drop { owner }),
        "exact actual local Guard Drop",
    )?;
    need(
        allocation.block == read.block
            && drop.block == read.block
            && drop.origin == Origin::Source(name)
            && g.position(allocation)? < g.position(read)?
            && g.position(read)? < g.position(drop)?
            && g.position(drop)? < g.position(value_end)?,
        "actual Guard construct/read/Drop/VEnd order",
        drop,
    )?;
    uses.protect(EntityId::Value(owner));
    uses.permit(EntityId::Value(owner), drop);
    Ok(drop)
}
}
