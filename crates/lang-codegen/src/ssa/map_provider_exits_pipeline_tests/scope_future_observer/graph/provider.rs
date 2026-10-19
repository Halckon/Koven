//! Original own parameter/local flows through every provider column to Return/Move/put.
mod transport;
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
    println!(
        "SCOPE D FUTURE PROVIDER label={} function={f:#?} edges={:#?}",
        c.label, g.edges
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
    uses.check(&g, returned)
}
