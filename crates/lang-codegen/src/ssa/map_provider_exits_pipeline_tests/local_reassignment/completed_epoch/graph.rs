//! Actual factory calls, complete provider edges and unique cleanup of both epochs.
use super::super::super::{
    fixture::{Failure, failure, need},
    query_inputs::one,
};
use super::{
    Case,
    ast::{inside, sites, span},
};
use crate::ssa::{
    map_provider_exits_pipeline_tests::query_graph::{Graph, Uses, loan, result, value},
    model::*,
};
use lang_frontend::{
    ast::ExpressionId,
    parser::{Expression, Item, NameMarker, ParsedFile},
    source::SourceMap,
};

fn factory<'a>(
    g: &Graph<'a>,
    module: &'a Module,
    sources: &SourceMap,
    parsed: &ParsedFile,
    id: ExpressionId,
) -> Result<&'a Instruction, Failure> {
    let n = parsed.ast().expressions().get(id).unwrap();
    let Expression::Call {
        callee, arguments, ..
    } = n.payload()
    else {
        return Err(failure("actual completed producer Call", n));
    };
    need(
        arguments.is_empty(),
        "actual zero-argument completed producer",
        n,
    )?;
    let name = sources
        .slice(span(parsed, *callee))
        .map_err(|e| failure("producer name", e))?;
    let decl = one(
        parsed.roots().iter().filter_map(|id| {
            let n = parsed.ast().items().get(*id).ok()?;
            matches!(
                n.payload(),
                Item::Function { name: NameMarker::Present(s), .. }
                    if sources.slice(*s) == Ok(name)
            )
            .then_some(n.span())
        }),
        "selected original completed producer",
    )?;

    let actual = one(
        module
            .functions
            .iter()
            .filter(|f| f.origin == Origin::Source(decl)),
        "actual selected producer callee",
    )?;
    let call = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(n.span())
                && matches!(
                    &i.operation,
                    Operation::DirectCall { callee, receiver: None, arguments }
                        if *callee == actual.id && arguments.is_empty()
                )
        }),
        "complete actual producer call",
    )?;

    need(
        actual.return_types.as_slice()
            == [g.function.entity(result(call)?).unwrap().ty.semantic_type()],
        "completed producer actual Value return ABI",
        call,
    )?;
    if name.ends_with("Empty") {
        let f = Graph::new(actual)?;
        let made = one(
            f.instructions
                .iter()
                .copied()
                .filter(|i| matches!(i.operation, Operation::MapConstruct { .. })),
            "actual empty epoch construction",
        )?;
        let mut current = result(made)?;
        need(
            !f.instructions.iter().any(|i| {
                matches!(
                    i.operation,
                    Operation::MapPut { .. }
                        | Operation::HeapAllocate { .. }
                        | Operation::DirectCall { .. }
                )
            }),
            "empty producer inserts and allocates no payload",
            actual.id,
        )?;
        for i in &f.instructions {
            if let Operation::MapConsume { owner, .. } = i.operation {
                need(
                    current == EntityId::Value(owner),
                    "empty consume preserves real zero entries",
                    i,
                )?;
                current = result(i)?;
            }
        }
        need(
            f.blocks.iter().any(|&b| {
                matches!(
                    &f.term(b).unwrap().kind,
                    TerminatorKind::Return { values }
                        if matches!(current, EntityId::Value(v) if values.as_slice() == [v])
                )
            }),
            "actual empty construction is returned",
            actual.id,
        )?;
    }
    Ok(call)
}
pub(super) fn check(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    program: &Program,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "completed event one module")?;
    for s in sites(sources, parsed)? {
        let f = one(
            module
                .functions
                .iter()
                .filter(|f| f.origin == Origin::Source(s.scan)),
            "actual completed epoch scanner",
        )?;
        let g = Graph::new(f)?;
        need(
            g.block(f.entry_block().unwrap())?.parameters.is_empty() && f.return_types.is_empty(),
            "actual completed scanner Unit ABI",
            f.id,
        )?;
        let old = factory(&g, module, sources, parsed, s.old)?;
        let new = factory(&g, module, sources, parsed, s.call)?;
        let a = result(old)?;
        let n = result(new)?;
        need(
            a != n && matches!(a, EntityId::Value(_)) && matches!(n, EntityId::Value(_)),
            "completed assignment installs distinct actual Call result",
            (a, n),
        )?;
        let map_type = f.entity(a).unwrap().ty.semantic_type();
        need(
            f.entity(n).unwrap().ty == f.entity(a).unwrap().ty,
            "new epoch preserves Map type",
            new,
        )?;
        let (kind, key_type, value_type) = module
            .map_container(map_type)
            .ok_or_else(|| failure("actual completed Map ABI", map_type))?;
        need(
            kind == if case.mutable {
                MapContainerKind::MutableMap
            } else {
                MapContainerKind::Map
            } && module.type_kind(key_type) == Some(&SsaTypeKind::StringOwner)
                && module.type_kind(value_type) == Some(&SsaTypeKind::StringOwner),
            "actual Pure String map representation",
            map_type,
        )?;
        let for_span = parsed.ast().statements().get(s.statement).unwrap().span();
        let op = |p: fn(&Operation) -> bool| {
            one(
                g.instructions.iter().copied().filter(|i| p(&i.operation)),
                "actual unique completed provider operation",
            )
        };
        let first = op(|o| matches!(o, Operation::MapIterationFirst { .. }))?;
        let has = op(|o| matches!(o, Operation::MapIterationHasNext { .. }))?;
        let key = op(|o| matches!(o, Operation::MapIterationKeyLoan { .. }))?;
        let val = op(|o| matches!(o, Operation::MapIterationValueLoan { .. }))?;
        let next = op(|o| matches!(o, Operation::MapIterationNext { .. }))?;
        need(
            [first, has, key, val, next]
                .iter()
                .all(|i| i.origin == Origin::Source(for_span)),
            "original completed epoch For origins",
            for_span,
        )?;
        let Operation::MapIterationFirst { source } = first.operation else {
            unreachable!()
        };
        let birth = g.definition(EntityId::Loan(source))?;
        let Operation::BorrowBegin {
            place,
            kind: LoanKind::Shared,
        } = birth.operation
        else {
            return Err(failure("old source lease", birth));
        };
        let root = g.definition(EntityId::Place(place))?;
        need(
            matches!(root.operation,Operation::RootPlace{owner} if EntityId::Value(owner)==a),
            "For borrows original make result, not new installation",
            root,
        )?;
        let Operation::MapIterationHasNext {
            source: hs,
            cursor: hc,
        } = has.operation
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
        } = &g.term(has.block)?.kind
        else {
            return Err(failure("old provider actual guard", has));
        };
        need(
            *condition == value(has)?
                && when_true.target == key.block
                && val.block == key.block
                && val.operation
                    == Operation::MapIterationValueLoan {
                        source: bs,
                        cursor: bc,
                    },
            "old actual guard and component identities",
            (has, key, val),
        )?;
        let entry = g.branch(first.block)?;
        let back = g.branch(key.block)?;
        let ho = g.transported(entry, a)?;
        let bo = g.transported(when_true, ho)?;
        let xo = g.transported(when_false, ho)?;
        g.incoming_from(first.block, &[])?;
        g.incoming_from(has.block, &[first.block, key.block])?;
        g.incoming_from(key.block, &[has.block])?;
        g.incoming_from(when_false.target, &[has.block])?;
        need(
            entry.target == has.block
                && g.transported(entry, EntityId::Loan(source))? == EntityId::Loan(hs)
                && g.transported(entry, result(first)?)? == EntityId::Value(hc)
                && g.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
                && g.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc)
                && back.target == has.block
                && g.argument(back, ho)? == bo
                && g.argument(back, EntityId::Loan(hs))? == EntityId::Loan(bs)
                && g.argument(back, EntityId::Value(hc))? == result(next)?
                && next.operation
                    == Operation::MapIterationNext {
                        source: bs,
                        cursor: bc,
                    },
            "every old owner/source/cursor incoming uses exact current columns",
            (entry, back),
        )?;
        let xs = g.transported(when_false, EntityId::Loan(hs))?;
        let xc = g.transported(when_false, EntityId::Value(hc))?;
        let end = one(
            g.instructions.iter().copied().filter(
                |i| matches!(i.operation,Operation::BorrowEnd{loan} if EntityId::Loan(loan)==xs),
            ),
            "old exact source End",
        )?;
        need(
            end.block == when_false.target,
            "old End on actual exhaustion",
            end,
        )?;
        let drops = g
            .instructions
            .iter()
            .copied()
            .filter(|i| {
                matches!(
                    i.operation,
                    Operation::Drop { owner }
                        if f.entity(EntityId::Value(owner)).unwrap().ty.semantic_type() == map_type
                )
            })
            .collect::<Vec<_>>();

        need(
            drops.len() == 2,
            "only actual old and new epoch Drop sites",
            &drops,
        )?;
        let old_drop = one(
            drops.iter().copied().filter(
                |i| matches!(i.operation,Operation::Drop{owner} if EntityId::Value(owner)==xo),
            ),
            "old Drop exact current terminal owner",
        )?;
        let new_drop = one(
            drops.iter().copied().filter(
                |i| matches!(i.operation,Operation::Drop{owner} if EntityId::Value(owner)==n),
            ),
            "new Drop exact installed other result",
        )?;
        need(
            old_drop.block == end.block
                && g.position(old_drop)? == g.position(end)? + 1
                && old_drop.origin == Origin::Source(s.binding),
            "old actual ASAP Drop, not conditional RHS remnant",
            old_drop,
        )?;
        need(
            new_drop.block == new.block
                && g.position(new_drop)? == g.position(new)? + 1
                && new_drop.origin == Origin::Source(span(parsed, s.assignment)),
            "new complete installation ASAP Drop has its own event origin",
            new_drop,
        )?;
        need(
            new.block == old_drop.block && g.position(old_drop)? < g.position(new)?,
            "old actual cleanup precedes later producer",
            (old_drop, new),
        )?;
        let tail_span = one(
            parsed.ast().expressions().iter().filter_map(|(_, i)| {
                let Expression::Call { arguments, .. } = i.payload() else {
                    return None;
                };
                let [arg] = arguments.as_slice() else {
                    return None;
                };
                (inside(i.span(), s.scan)
                    && sources.slice(span(parsed, arg.value)) == Ok("\"tail\""))
                .then_some(i.span())
            }),
            "actual tail marker AST",
        )?;
        let tail = one(
            g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(tail_span)
                    && matches!(
                        i.operation,
                        Operation::PrintString { .. } | Operation::PrintLiteral { .. }
                    )
            }),
            "actual tail marker",
        )?;
        need(
            tail.block == new_drop.block && g.position(new_drop)? < g.position(tail)?,
            "new epoch dropped before tail",
            tail,
        )?;
        let mut uses = Uses::default();
        for e in [
            a,
            ho,
            bo,
            xo,
            n,
            EntityId::Place(place),
            EntityId::Loan(source),
            EntityId::Loan(hs),
            EntityId::Loan(bs),
            xs,
            result(first)?,
            EntityId::Value(hc),
            EntityId::Value(bc),
            xc,
            result(next)?,
            result(key)?,
            result(val)?,
        ] {
            uses.protect(e);
        }
        uses.permit(a, root);
        uses.permit(EntityId::Place(place), birth);
        uses.permit(EntityId::Loan(source), first);
        uses.permit(EntityId::Loan(hs), has);
        uses.permit(EntityId::Value(hc), has);
        for i in [key, val, next] {
            uses.permit(EntityId::Loan(bs), i);
            uses.permit(EntityId::Value(bc), i);
        }
        uses.permit(xs, end);
        uses.permit(xo, old_drop);
        uses.permit(n, new_drop);
        let read = one(
            g.instructions.iter().copied().filter(|i| {
                i.block == key.block
                    && matches!(
                        &i.operation,
                        Operation::DirectCall { receiver: None, arguments, .. }
                            if arguments.as_slice() == [result(key).unwrap(), result(val).unwrap()]
                    )
            }),
            "old component reader exact operands",
        )?;

        let read_decl = one(
            parsed.roots().iter().filter_map(|id| {
                let i = parsed.ast().items().get(*id).ok()?;
                matches!(
                    i.payload(),
                    Item::Function { name: NameMarker::Present(name), .. }
                        if sources.slice(*name) == Ok("read")
                )
                .then_some(i.span())
            }),
            "original component read declaration",
        )?;

        let reader = one(
            module
                .functions
                .iter()
                .filter(|f| f.origin == Origin::Source(read_decl)),
            "actual selected component reader",
        )?;
        need(
            matches!(read.operation,Operation::DirectCall{callee,..} if callee==reader.id),
            "read call selects original source declaration",
            read,
        )?;
        for (slot, (component, target)) in
            [(key, key_type), (val, value_type)].into_iter().enumerate()
        {
            let expected = EntityType::Loan {
                kind: LoanKind::Shared,
                target,
            };
            need(
                f.entity(result(component)?)
                    .is_some_and(|d| d.ty == expected)
                    && reader
                        .block(reader.entry_block().unwrap())
                        .unwrap()
                        .parameters
                        .get(slot)
                        .and_then(|p| reader.entity(*p))
                        .is_some_and(|d| d.ty == expected),
                "component and selected reader preserve actual Shared String ABI",
                component,
            )?;
        }
        let mut ends = Vec::new();
        for component in [key, val] {
            let e = result(component)?;
            let ending = one(
                g.instructions.iter().copied().filter(|i| {
                    i.operation
                        == Operation::BorrowEnd {
                            loan: loan(component).unwrap(),
                        }
                }),
                "actual old component End",
            )?;
            need(ending.block == key.block, "old body component End", ending)?;
            uses.permit(e, read);
            uses.permit(e, ending);
            ends.push(ending);
        }
        need(
            g.position(read)? < g.position(ends[1])?
                && g.position(ends[1])? < g.position(ends[0])?
                && g.position(ends[0])? < g.position(next)?,
            "old read/VEnd/KEnd/Next order",
            &ends,
        )?;
        let body_span = parsed.ast().statements().get(s.body).unwrap().span();
        let (guard_name, guard_call) = one(
            parsed
                .ast()
                .items()
                .iter()
                .filter_map(|(_, i)| match i.payload() {
                    Item::Variable {
                        name: NameMarker::Present(name),
                        initializer,
                        ..
                    } if inside(i.span(), body_span) && sources.slice(*name) == Ok("local") => {
                        Some((*name, *initializer))
                    }
                    _ => None,
                }),
            "old actual Guard local",
        )?;
        let guard = one(
            g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(span(parsed, guard_call))
                    && matches!(i.operation, Operation::HeapAllocate { .. })
            }),
            "old actual Guard allocation",
        )?;
        let guard_owner = value(guard)?;
        let guard_drop = one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.operation == Operation::Drop { owner: guard_owner }),
            "old actual Guard Drop",
        )?;
        need(
            guard.block == read.block
                && guard_drop.block == read.block
                && guard_drop.origin == Origin::Source(guard_name)
                && g.position(guard)? < g.position(read)?
                && g.position(read)? < g.position(guard_drop)?
                && g.position(guard_drop)? < g.position(ends[1])?,
            "Guard is dropped after read before VEnd",
            guard_drop,
        )?;
        uses.protect(EntityId::Value(guard_owner));
        uses.permit(EntityId::Value(guard_owner), guard_drop);
        uses.check(&g, None)?;
        let cleanup = old_drop.block;
        need(
            matches!(&g.term(cleanup)?.kind,TerminatorKind::Return{values} if values.is_empty())
                && g.edges
                    .iter()
                    .all(|(from, e)| !(*from == cleanup && e.target == has.block)),
            "both epoch Drops lie outside loop and lead to Unit Return",
            cleanup,
        )?;
        need(
            g.blocks
                .iter()
                .filter(|&&b| matches!(g.term(b).unwrap().kind, TerminatorKind::Return { .. }))
                .count()
                == 1,
            "every finite path has one common old/new cleanup and Unit Return",
            f.id,
        )?;
    }
    Ok(())
}
