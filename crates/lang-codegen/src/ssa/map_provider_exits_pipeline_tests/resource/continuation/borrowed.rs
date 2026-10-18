//! Newly admitted shared Resource Continue: callee provider lease and caller owner stay distinct.
use super::super::identity::Input;
use crate::ssa::{
    map_provider_exits_pipeline_tests::{
        fixture::{Failure, failure, need},
        query_graph::{Graph, Uses, loan, result, value},
        query_inputs::one,
    },
    model::*,
};

pub(super) fn check(
    program: &Program,
    input: &Input,
    parsed: &lang_frontend::parser::ParsedFile,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "one original module")?;
    let scan = one(
        module
            .functions
            .iter()
            .filter(|f| f.origin == Origin::Source(input.scan)),
        "actual Shared scan",
    )?;
    let g = Graph::new(scan)?;
    let [anchor] = g.block(scan.entry_block().unwrap())?.parameters.as_slice() else {
        return Err(failure("single original caller loan", scan));
    };
    need(
        matches!(
            scan.entity(*anchor).unwrap().ty,
            EntityType::Loan {
                kind: LoanKind::Shared,
                ..
            }
        ),
        "Shared parameter ABI",
        anchor,
    )?;
    let op = |predicate: fn(&Operation) -> bool| {
        one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| predicate(&i.operation)),
            "unique Shared provider op",
        )
    };
    let first = op(|o| matches!(o, Operation::MapIterationFirst { .. }))?;
    let guard = op(|o| matches!(o, Operation::MapIterationHasNext { .. }))?;
    let key = op(|o| matches!(o, Operation::MapIterationKeyLoan { .. }))?;
    let val = op(|o| matches!(o, Operation::MapIterationValueLoan { .. }))?;
    let next = op(|o| matches!(o, Operation::MapIterationNext { .. }))?;
    need(
        [first, guard, key, val]
            .iter()
            .all(|i| i.origin == Origin::Source(input.for_span)),
        "original Shared For origins",
        input.for_span,
    )?;
    let next_span = parsed
        .ast()
        .expressions()
        .get(input.continued.unwrap())
        .unwrap()
        .span();
    need(
        next.origin == Origin::Source(next_span),
        "actual Shared Continue Next origin",
        next,
    )?;
    let Operation::MapIterationFirst { source } = first.operation else {
        unreachable!()
    };
    let birth = g.definition(EntityId::Loan(source))?;
    let Operation::SharedReborrow { source: caller } = birth.operation else {
        return Err(failure("provider owns its reborrow", birth));
    };
    let mut roots = super::graph::transport(&g, EntityId::Loan(caller), *anchor)?;
    loop {
        let before = roots.len();
        for (_, edge) in &g.edges {
            for (&argument, &parameter) in
                edge.arguments.iter().zip(&g.block(edge.target)?.parameters)
            {
                if roots.contains(&argument) {
                    roots.insert(parameter);
                }
            }
        }
        if before == roots.len() {
            break;
        }
    }
    for &root in &roots {
        super::graph::transport(&g, root, *anchor)?;
    }
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
        return Err(failure("provider branch", guard));
    };
    need(
        *condition == value(guard)?
            && when_true.target == key.block
            && val.block == key.block
            && next.block == key.block
            && val.operation
                == Operation::MapIterationValueLoan {
                    source: bs,
                    cursor: bc,
                }
            && next.operation
                == Operation::MapIterationNext {
                    source: bs,
                    cursor: bc,
                },
        "actual provider guard/K/V/Next",
        (guard, key, val, next),
    )?;
    g.incoming_from(guard.block, &[first.block, key.block])?;
    g.incoming_from(key.block, &[guard.block])?;
    g.incoming_from(when_false.target, &[guard.block])?;
    let entry = g.branch(first.block)?;
    let back = g.branch(key.block)?;
    need(
        g.transported(entry, EntityId::Loan(source))? == EntityId::Loan(hs)
            && g.transported(entry, result(first)?)? == EntityId::Value(hc)
            && g.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc)
            && back.target == guard.block
            && g.argument(back, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.argument(back, EntityId::Value(hc))? == result(next)?,
        "all Shared provider incoming columns",
        (entry, back),
    )?;
    let xs = g.transported(when_false, EntityId::Loan(hs))?;
    let end = one(
        g.instructions.iter().copied().filter(
            |i| matches!(i.operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==xs),
        ),
        "unique own-provider End",
    )?;
    need(
        end.block == when_false.target && !roots.contains(&xs),
        "End only callee lease on exhaustion",
        end,
    )?;
    let read=one(g.instructions.iter().copied().filter(|i|matches!(&i.operation,Operation::DirectCall{arguments,receiver:None,..} if arguments.as_slice()==[result(key).unwrap(),result(val).unwrap()])),"actual Shared K/V read")?;
    let Operation::DirectCall { callee, .. } = read.operation else {
        unreachable!()
    };
    need(
        module
            .function(callee)
            .is_some_and(|f| f.origin == Origin::Source(input.read)),
        "original read callee",
        read,
    )?;
    let mut uses = Uses::default();
    for cursor in [
        result(first)?,
        EntityId::Value(hc),
        EntityId::Value(bc),
        result(next)?,
        g.transported(when_false, EntityId::Value(hc))?,
    ] {
        uses.protect(cursor);
    }
    uses.permit(EntityId::Value(hc), guard);
    for i in [key, val, next] {
        uses.permit(EntityId::Value(bc), i);
    }

    for &root in &roots {
        uses.protect(root);
    }
    uses.permit(EntityId::Loan(caller), birth);
    for entity in [EntityId::Loan(hs), EntityId::Loan(bs), xs] {
        for root in super::graph::transport(&g, entity, EntityId::Loan(source))? {
            uses.protect(root);
        }
    }
    uses.permit(EntityId::Loan(source), first);
    uses.permit(EntityId::Loan(hs), guard);
    for i in [key, val, next] {
        uses.permit(EntityId::Loan(bs), i);
    }
    uses.permit(xs, end);
    let mut component_ends = vec![];
    for component in [key, val] {
        let entity = result(component)?;
        uses.protect(entity);
        uses.permit(entity, read);
        let end = one(
            g.instructions.iter().copied().filter(|i| {
                i.operation
                    == Operation::BorrowEnd {
                        loan: loan(component).unwrap(),
                    }
            }),
            "component End",
        )?;
        uses.permit(entity, end);
        component_ends.push(end);
    }
    need(
        component_ends.iter().all(|i| i.block == next.block)
            && g.position(read)? < g.position(component_ends[1])?
            && g.position(component_ends[1])? < g.position(component_ends[0])?
            && g.position(component_ends[0])? < g.position(next)?,
        "Shared read/value End/key End/Next",
        &component_ends,
    )?;
    need(!g.instructions.iter().any(|i|matches!(i.operation,Operation::BorrowEnd{loan} if roots.contains(&EntityId::Loan(loan)))) && !g.instructions.iter().any(|i|matches!(i.operation,Operation::Drop{owner} if matches!(scan.entity(EntityId::Value(owner)).unwrap().ty,EntityType::Value(t) if matches!(module.type_kind(t),Some(SsaTypeKind::MapContainer{..}))))),"callee preserves caller loan and never drops its Map",scan.id)?;
    uses.check(&g, None)?;
    // Caller owns separate per-call loans. Both end after the actual scan returns;
    // any post-scan MapPut must follow the first loan End on that exact owner.
    let caller = one(
        module.functions.iter().filter(|f| {
            f.instructions
                .iter()
                .filter(
                    |i| matches!(i.operation,Operation::DirectCall{callee,..} if callee==scan.id),
                )
                .count()
                == 2
        }),
        "actual caller has populated and empty calls",
    )?;
    let cg = Graph::with_abort(caller)?;
    let calls = cg
        .instructions
        .iter()
        .copied()
        .filter(|i| matches!(i.operation,Operation::DirectCall{callee,..} if callee==scan.id))
        .collect::<Vec<_>>();
    let mut original = None;
    for call in &calls {
        let Operation::DirectCall {
            arguments,
            receiver: None,
            ..
        } = &call.operation
        else {
            unreachable!()
        };
        let [EntityId::Loan(argument)] = arguments.as_slice() else {
            return Err(failure("caller Borrow delivery", call));
        };
        let birth = cg.definition(EntityId::Loan(*argument))?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = birth.operation
        else {
            return Err(failure("caller scoped loan", birth));
        };
        let root = cg.definition(EntityId::Place(place))?;
        let Operation::RootPlace { owner } = root.operation else {
            return Err(failure("caller original root", root));
        };
        let ended = one(
            cg.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::BorrowEnd { loan: *argument }),
            "caller loan end",
        )?;
        need(
            birth.block == call.block
                && ended.block == call.block
                && cg.position(birth)? < cg.position(call)?
                && cg.position(call)? < cg.position(ended)?,
            "caller loan only ends after scan return",
            call,
        )?;
        if original.is_none() {
            original = Some((owner, ended));
        }
    }
    let (owner, ended) = original.unwrap();
    let puts = cg
        .instructions
        .iter()
        .copied()
        .filter(|i| matches!(i.operation, Operation::MapPut { .. }))
        .collect::<Vec<_>>();
    let mut current = owner;
    for put in puts {
        let Operation::MapPut { owner: actual, .. } = put.operation else {
            unreachable!()
        };
        need(
            actual == owner
                && put.block == ended.block
                && cg.position(ended)? < cg.position(put)?,
            "post-scan put uses caller owner after loan End",
            put,
        )?;
        current = value(put)?;
    }
    let sizes = cg
        .instructions
        .iter()
        .copied()
        .filter(|i| matches!(i.operation, Operation::MapSize { .. }))
        .collect::<Vec<_>>();
    need(sizes.len() == 1, "actual caller post-use size", &sizes)?;
    need(
        matches!(sizes[0].operation,Operation::MapSize{owner:EntityId::Value(v)} if v==current)
            && sizes[0].block == ended.block
            && cg.position(ended)? < cg.position(sizes[0])?,
        "caller size after its own loan End",
        sizes[0],
    )?;
    Ok(())
}
