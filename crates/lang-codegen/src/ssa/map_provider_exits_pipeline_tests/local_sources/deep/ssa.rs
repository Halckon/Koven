//! All real CFG incoming columns, exact lifetime consumers, and source-separated uses.
use super::super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, loan, result, value},
    query_inputs::one,
};
use super::super::ast::{Anchors, Input, Site};
use crate::ssa::model::*;
use std::collections::BTreeSet;

pub(in super::super) fn query(
    input: &Input,
    anchors: &Anchors,
    program: &Program,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "local SSA one source module")?;
    let reader = anchors
        .function("read")
        .ok_or_else(|| failure("local AST reader", anchors))?;
    let read_function = function(module, reader.span)?;
    for scanner in anchors
        .functions
        .iter()
        .filter(|f| f.name.starts_with("scan"))
    {
        let scan = function(module, scanner.span)?;
        let g = if input.later {
            Graph::with_abort(scan)?
        } else {
            Graph::new(scan)?
        };
        let sites = anchors
            .sites
            .iter()
            .filter(|s| s.scanner_span == scanner.span)
            .collect::<Vec<_>>();
        need(
            sites.len() == 2 && g.blocks.len() == if input.later { 11 } else { 7 },
            "local complete reachable two-loop CFG",
            (&sites, &g.blocks),
        )?;
        let mut uses = Uses::default();
        let mut previous_exit = None;
        let mut map_drops = BTreeSet::new();
        let mut owner_births = BTreeSet::new();
        for site in sites {
            let (pre, exit, drop, owner_birth) = site_query(
                input,
                anchors,
                module,
                &g,
                site,
                read_function.id,
                &mut uses,
            )?;
            if let Some(previous) = previous_exit {
                if input.later {
                    let TerminatorKind::Conditional {
                        when_true,
                        when_false,
                        ..
                    } = &g.term(previous)?.kind
                    else {
                        return Err(failure("local previous size continuation", previous));
                    };
                    need(
                        when_true.target == pre
                            && matches!(g.term(when_false.target)?.kind, TerminatorKind::Abort),
                        "local second formation only on actual size success",
                        (when_true, when_false),
                    )?;
                    g.incoming_from(pre, &[previous])?;
                } else {
                    need(
                        pre == previous,
                        "local second formation follows prior source cleanup in same block",
                        (pre, previous),
                    )?;
                }
            } else {
                need(
                    Some(pre) == scan.entry_block() && g.block(pre)?.parameters.is_empty(),
                    "local first formation in zero-parameter scanner entry",
                    pre,
                )?;
                g.incoming_from(pre, &[])?;
            }
            previous_exit = Some(exit);
            map_drops.insert(drop);
            owner_births.insert(owner_birth);
        }
        let actual_drops = g
            .instructions
            .iter()
            .filter_map(|i| match i.operation {
                Operation::Drop { owner }
                    if scan
                        .entity(EntityId::Value(owner))
                        .is_some_and(|e| module.map_container(e.ty.semantic_type()).is_some()) =>
                {
                    Some(i.id)
                }
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        need(
            actual_drops == map_drops && map_drops.len() == 2,
            "local exactly two Map drops, no stale predecessor duplicate",
            actual_drops,
        )?;
        let terminal = previous_exit.ok_or_else(|| failure("local final provider", scanner))?;
        if input.later {
            let TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } = &g.term(terminal)?.kind
            else {
                return Err(failure("local final size branch", terminal));
            };
            need(
                matches!(g.term(when_true.target)?.kind, TerminatorKind::Return { ref values } if values.is_empty())
                    && matches!(g.term(when_false.target)?.kind, TerminatorKind::Abort),
                "local final size success returns, failure aborts",
                g.term(terminal)?,
            )?;
            g.incoming_from(when_true.target, &[terminal])?;
            g.incoming_from(when_false.target, &[terminal])?;
        } else {
            need(
                matches!(g.term(terminal)?.kind, TerminatorKind::Return {ref values} if values.is_empty()),
                "local final exhaustion returns",
                terminal,
            )?;
        }
        need(
            g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapIterationFirst { .. }))
                .count()
                == 2
                && g.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::MapIterationHasNext { .. }))
                    .count()
                    == 2
                && g.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::MapIterationKeyLoan { .. }))
                    .count()
                    == 2
                && g.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::MapIterationValueLoan { .. }))
                    .count()
                    == 2
                && g.instructions
                    .iter()
                    .filter(|i| matches!(i.operation, Operation::MapIterationNext { .. }))
                    .count()
                    == 2,
            "local complete provider operation inventory",
            scanner,
        )?;
        crate::ssa::map_provider_exits_pipeline_tests::local_exits::owner_flow::check(
            &g,
            module,
            &owner_births,
        )?;
        uses.check(&g, None)?;
    }
    Ok(())
}

fn site_query(
    input: &Input,
    anchors: &Anchors,
    module: &Module,
    g: &Graph<'_>,
    site: &Site,
    reader: FunctionId,
    uses: &mut Uses,
) -> Result<(BlockId, BlockId, InstructionId, EntityId), Failure> {
    let op = |predicate: fn(&Operation) -> bool| {
        one(
            g.instructions
                .iter()
                .copied()
                .filter(|i| i.origin == Origin::Source(site.span) && predicate(&i.operation)),
            "local original For unique SSA operation",
        )
    };
    let first = op(|o| matches!(o, Operation::MapIterationFirst { .. }))?;
    let guard = op(|o| matches!(o, Operation::MapIterationHasNext { .. }))?;
    let key = op(|o| matches!(o, Operation::MapIterationKeyLoan { .. }))?;
    let val = op(|o| matches!(o, Operation::MapIterationValueLoan { .. }))?;
    let next = op(|o| matches!(o, Operation::MapIterationNext { .. }))?;
    let Operation::MapIterationFirst { source: lease } = first.operation else {
        return Err(failure("local First", first));
    };
    let Operation::MapIterationHasNext {
        source: hs,
        cursor: hc,
    } = guard.operation
    else {
        return Err(failure("local HasNext", guard));
    };
    let Operation::MapIterationKeyLoan {
        source: bs,
        cursor: bc,
    } = key.operation
    else {
        return Err(failure("local Key", key));
    };
    let pre = first.block;
    let header = guard.block;
    let body = key.block;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &g.term(header)?.kind
    else {
        return Err(failure("local actual guard", header));
    };
    let exit = when_false.target;
    control_use(g, guard, header)?;
    need(
        *condition == value(guard)?
            && when_true.target == body
            && val.block == body
            && next.block == body
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
        "local actual guard/component/Next source identity",
        (guard, key, val, next),
    )?;
    g.incoming_from(header, &[pre, body])?;
    g.incoming_from(body, &[header])?;
    g.incoming_from(exit, &[header])?;
    let factory = anchors
        .function(site.factory.target)
        .ok_or_else(|| failure("local original factory", site))?;
    let callee = function(module, factory.span)?;
    let birth = one(g.instructions.iter().copied().filter(|i| i.origin == Origin::Source(site.factory.span) && matches!(i.operation, Operation::DirectCall {callee: c, ref arguments, receiver: None} if c == callee.id && arguments.is_empty())), "local exact zero-arg factory SSA birth")?;
    let seed = result(birth)?;
    let lease_birth = g.definition(EntityId::Loan(lease))?;
    let Operation::BorrowBegin {
        place,
        kind: LoanKind::Shared,
    } = lease_birth.operation
    else {
        return Err(failure("local own source BorrowBegin", lease_birth));
    };
    let root = g.definition(EntityId::Place(place))?;
    need(
        root.operation
            == Operation::RootPlace {
                owner: value(birth)?,
            }
            && birth.block == pre
            && root.block == pre
            && lease_birth.block == pre
            && g.position(birth)? < g.position(root)?
            && g.position(root)? < g.position(lease_birth)?
            && g.position(lease_birth)? < g.position(first)?,
        "local actual factory result roots provider loan",
        (birth, root, lease_birth),
    )?;
    let entry = g.branch(pre)?;
    let back = g.branch(body)?;
    let ho = g.transported(entry, seed)?;
    let bo = g.transported(when_true, ho)?;
    let xo = g.transported(when_false, ho)?;
    need(
        entry.target == header
            && back.target == header
            && g.transported(entry, EntityId::Loan(lease))? == EntityId::Loan(hs)
            && g.transported(entry, result(first)?)? == EntityId::Value(hc)
            && g.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc)
            && g.argument(back, ho)? == bo
            && g.argument(back, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && g.argument(back, EntityId::Value(hc))? == result(next)?,
        "local every owner/lease/cursor incoming column",
        (entry, when_true, when_false, back),
    )?;
    let xs = g.transported(when_false, EntityId::Loan(hs))?;
    let xc = g.transported(when_false, EntityId::Value(hc))?;
    for (block, roles) in [
        (header, [ho, EntityId::Loan(hs), EntityId::Value(hc)]),
        (body, [bo, EntityId::Loan(bs), EntityId::Value(bc)]),
        (exit, [xo, xs, xc]),
    ] {
        let actual = &g.block(block)?.parameters;
        need(
            actual.len() == 3
                && actual.iter().copied().collect::<BTreeSet<_>>() == roles.into_iter().collect(),
            "local full parameter set has no dead or decoy owner columns",
            (block, actual),
        )?;
    }
    let EntityId::Loan(xs_loan) = xs else {
        return Err(failure("local exit source loan", xs));
    };
    let end = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::BorrowEnd { loan: xs_loan }),
        "local unique actual EndSource",
    )?;
    let exit_owner = value_entity(xo)?;
    let drop = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::Drop { owner: exit_owner }),
        "local unique actual current owner drop",
    )?;
    need(
        end.block == exit
            && drop.block == exit
            && drop.origin == Origin::Source(site.local.marker)
            && g.position(end)? < g.position(drop)?,
        "local actual source ends before current owner Drop",
        (end, drop),
    )?;
    let read = one(
        g.instructions.iter().copied().filter(|i| {
            i.origin == Origin::Source(site.read.span)
                && matches!(i.operation, Operation::DirectCall {callee,..} if callee == reader)
        }),
        "local actual read call",
    )?;
    need(
        matches!(&read.operation,Operation::DirectCall {receiver:None,arguments,..} if arguments.as_slice() == [result(key)?,result(val)?])
            && read.block == body
            && loan(key)? != loan(val)?,
        "local exact two borrowed read operands",
        read,
    )?;
    let key_loan = loan(key)?;
    let kend = one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.operation == Operation::BorrowEnd { loan: key_loan }),
        "local key end",
    )?;
    let vend = one(g.instructions.iter().copied().filter(|i| matches!(i.operation,Operation::BorrowEnd {loan:l} if val.results.as_slice() == [EntityId::Loan(l)])), "local value end")?;
    need(
        kend.block == body
            && vend.block == body
            && g.position(key)? < g.position(val)?
            && g.position(val)? < g.position(read)?
            && g.position(read)? < g.position(vend)?
            && g.position(vend)? < g.position(kend)?
            && g.position(kend)? < g.position(next)?,
        "local read then value/key cleanup before Next",
        (key, val, read, vend, kend, next),
    )?;
    let map_type = g
        .function
        .entity(seed)
        .ok_or_else(|| failure("local seed type", seed))?
        .ty
        .semantic_type();
    let (kind, kt, vt) = module
        .map_container(map_type)
        .ok_or_else(|| failure("local source Map type", map_type))?;
    need(
        kind == if site.map {
            MapContainerKind::Map
        } else {
            MapContainerKind::MutableMap
        } && kt == vt
            && module.type_kind(kt) == Some(&SsaTypeKind::StringOwner),
        "local actual intrinsic String/String storage",
        map_type,
    )?;
    for entity in [ho, bo, xo] {
        need(
            g.function
                .entity(entity)
                .is_some_and(|e| e.ty == EntityType::Value(map_type)),
            "local exact owner type",
            entity,
        )?;
    }
    for entity in [
        EntityId::Loan(lease),
        EntityId::Loan(hs),
        EntityId::Loan(bs),
        xs,
    ] {
        need(
            g.function.entity(entity).is_some_and(|e| {
                e.ty == EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: map_type,
                }
            }),
            "local exact source loan type",
            entity,
        )?;
        uses.protect(entity);
    }
    for entity in [result(key)?, result(val)?] {
        need(
            g.function.entity(entity).is_some_and(|e| {
                e.ty == EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: kt,
                }
            }),
            "local Shared component type",
            entity,
        )?;
    }
    for entity in [result(first)?, EntityId::Value(hc), EntityId::Value(bc), xc] {
        need(g.function.entity(entity).is_some_and(|e| matches!(e.ty,EntityType::Value(t) if module.type_kind(t) == Some(&SsaTypeKind::Integer {bits:64,signed:false}))), "local u64 cursor columns", entity)?;
        uses.protect(entity);
    }
    for entity in [
        seed,
        ho,
        bo,
        xo,
        result(key)?,
        result(val)?,
        EntityId::Place(place),
    ] {
        uses.protect(entity);
    }
    uses.permit(seed, root);
    uses.permit(EntityId::Place(place), lease_birth);
    uses.permit(EntityId::Loan(lease), first);
    uses.permit(EntityId::Loan(hs), guard);
    uses.permit(EntityId::Value(hc), guard);
    for i in [key, val, next] {
        uses.permit(EntityId::Loan(bs), i);
        uses.permit(EntityId::Value(bc), i);
    }
    uses.permit(xs, end);
    uses.permit(xo, drop);
    uses.permit(result(key)?, read);
    uses.permit(result(val)?, read);
    uses.permit(result(key)?, kend);
    uses.permit(result(val)?, vend);
    if let Some(size) = &site.size {
        let size_op = one(
            g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(size.span)
                    && i.operation == Operation::MapSize { owner: xo }
            }),
            "local actual later size reads current false owner",
        )?;
        need(
            size_op.block == exit
                && g.position(end)? < g.position(size_op)?
                && g.position(size_op)? < g.position(drop)?,
            "local retained source until exact size then ASAP Drop",
            (end, size_op, drop),
        )?;
        let compare_span = input.expression(size.comparison)?.1;
        let size_value = value(size_op)?;
        let compare = one(
            g.instructions.iter().copied().filter(|i| {
                i.origin == Origin::Source(compare_span)
                    && matches!(i.operation,Operation::Compare {left,..} if left == size_value)
            }),
            "local exact size comparison",
        )?;
        let Operation::Compare {
            operator: ComparisonOperator::Equal,
            left,
            right,
        } = compare.operation
        else {
            return Err(failure("local size equality", compare));
        };
        let constant = g.definition(EntityId::Value(right))?;
        control_use(g, compare, exit)?;
        let TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        } = &g.term(exit)?.kind
        else {
            return Err(failure("local exact size successors", exit));
        };
        need(
            when_true.arguments.is_empty()
                && when_false.arguments.is_empty()
                && g.block(when_true.target)?.parameters.is_empty()
                && g.block(when_false.target)?.parameters.is_empty()
                && matches!(g.term(when_false.target)?.kind, TerminatorKind::Abort)
                && g.term(exit)?.origin == Origin::Source(input.expression(size.condition)?.1),
            "local size original If has no residual owner columns and a real Abort successor",
            g.term(exit)?,
        )?;
        g.incoming_from(when_true.target, &[exit])?;
        g.incoming_from(when_false.target, &[exit])?;
        let expected = if site.factory.target.contains("Empty") {
            0
        } else {
            2
        };
        need(
            left == value(size_op)?
                && constant.operation == Operation::Constant(ScalarConstant::Integer(expected))
                && g.position(drop)? < g.position(compare)?
                && matches!(&g.term(exit)?.kind,TerminatorKind::Conditional {condition,..} if *condition == value(compare)?),
            "local exact size result/constant/conditional",
            (size_op, compare, constant),
        )?;
        for i in [size_op, constant] {
            need(g.function.entity(result(i)?).is_some_and(|e| matches!(e.ty,EntityType::Value(t) if module.type_kind(t) == Some(&SsaTypeKind::Integer {bits:32,signed:true}))), "local size/constant i32 storage", i)?;
        }
        uses.permit(xo, size_op);
        uses.protect(result(constant)?);
        uses.permit(result(constant)?, compare);
        uses.protect(result(size_op)?);
        uses.permit(result(size_op)?, compare);
    }
    Ok((pre, exit, drop.id, seed))
}

fn value_entity(entity: EntityId) -> Result<ValueId, Failure> {
    match entity {
        EntityId::Value(v) => Ok(v),
        _ => Err(failure("local Value role", entity)),
    }
}

// Guard/comparison results are control operands rather than edge columns.
// Check every reachable instruction, edge and terminator; no hidden consumer is allowed.
fn control_use(g: &Graph<'_>, instruction: &Instruction, block: BlockId) -> Result<(), Failure> {
    let entity = result(instruction)?;
    let v = value(instruction)?;
    need(
        g.instructions
            .iter()
            .all(|i| !i.operation.entities().contains(&entity))
            && g.edges
                .iter()
                .all(|(_, edge)| !edge.arguments.contains(&entity))
            && g.blocks.iter().all(|&b| match g.term(b) {
                Ok(t) if b == block => {
                    matches!(t.kind,TerminatorKind::Conditional {condition,..} if condition == v)
                }
                Ok(t) => !t.kind.entities().contains(&entity),
                Err(_) => false,
            }),
        "local exact complete boolean control uses",
        (instruction, block),
    )
}
