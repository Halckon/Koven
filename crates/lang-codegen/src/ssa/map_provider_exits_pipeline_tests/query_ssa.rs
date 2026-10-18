//! 从真实 operand 发现 source/cursor 列，逐边证明所有 incoming；没有 arena 序号 oracle。
use super::{
    fixture::{Exit, Failure, failure, need},
    query_cleanup,
    query_graph::{Graph, Uses, function, loan, result, value},
    query_inputs::{Inputs, one},
};
use crate::ssa::model::*;
use std::collections::BTreeSet;

pub(super) fn check(program: &Program, input: &Inputs) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "identity SSA module")?;
    let scan = function(module, input.scan)?;
    let graph = Graph::new(scan)?;
    let all = &graph.instructions;
    let first = one(
        all.iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::MapIterationFirst { .. })),
        "identity SSA unique First",
    )?;
    let guard = one(
        all.iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::MapIterationHasNext { .. })),
        "identity SSA unique HasNext",
    )?;
    let key = one(
        all.iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::MapIterationKeyLoan { .. })),
        "identity SSA unique KeyLoan",
    )?;
    let val = one(
        all.iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::MapIterationValueLoan { .. })),
        "identity SSA unique ValueLoan",
    )?;
    let nexts = all
        .iter()
        .copied()
        .filter(|i| matches!(i.operation, Operation::MapIterationNext { .. }))
        .collect::<Vec<_>>();
    need(
        nexts.len() == usize::from(input.exit == Exit::Continue),
        "identity SSA Next exists only for Continue",
        &nexts,
    )?;
    let Operation::MapIterationFirst { source: lease } = first.operation else {
        return Err(failure("identity SSA First shape", first));
    };
    let Operation::MapIterationHasNext {
        source: hs,
        cursor: hc,
    } = guard.operation
    else {
        return Err(failure("identity SSA guard shape", guard));
    };
    let Operation::MapIterationKeyLoan {
        source: bs,
        cursor: bc,
    } = key.operation
    else {
        return Err(failure("identity SSA key shape", key));
    };
    need(
        val.operation
            == Operation::MapIterationValueLoan {
                source: bs,
                cursor: bc,
            },
        "identity SSA K/V same current source/cursor",
        (key, val),
    )?;
    for i in [first, guard, key, val] {
        need(
            i.origin == Origin::Source(input.site.span),
            "identity SSA provider exact For origin",
            i,
        )?;
    }
    let pre = first.block;
    let header = guard.block;
    let body = key.block;
    let TerminatorKind::Conditional {
        condition,
        when_true,
        when_false,
    } = &graph.term(header)?.kind
    else {
        return Err(failure(
            "identity SSA real HasNext branch",
            graph.term(header)?,
        ));
    };
    need(
        *condition == value(guard)? && when_true.target == body && val.block == body,
        "identity SSA guard true edge dominates component births",
        graph.term(header)?,
    )?;
    let exhausted = when_false.target;
    let pre_edge = graph.branch(pre)?;
    need(
        pre_edge.target == header && scan.entry_block() == Some(pre),
        "identity SSA First entry edge",
        pre_edge,
    )?;
    need(
        graph.ops(header)?.iter().map(|i| i.id).collect::<Vec<_>>() == [guard.id],
        "identity SSA header has only current HasNext",
        graph.ops(header)?,
    )?;
    let mut roles = vec![pre, header, body, exhausted];
    if input.exit == Exit::Break {
        roles.push(graph.branch(body)?.target);
    }
    let unique = roles.iter().copied().collect::<BTreeSet<_>>();
    need(
        unique.len() == roles.len()
            && graph.blocks.iter().copied().collect::<BTreeSet<_>>() == unique
            && graph.blocks.len() == scan.blocks.len()
            && graph.instructions.len() == scan.instructions.len(),
        "identity SSA closed reachable CFG and actual instruction arena",
        (&graph.blocks, &roles),
    )?;
    graph.incoming_from(pre, &[])?;
    graph.incoming_from(
        header,
        if input.exit == Exit::Continue {
            &[pre, body]
        } else {
            &[pre]
        },
    )?;
    graph.incoming_from(body, &[header])?;
    graph.incoming_from(exhausted, &[header])?;
    need(
        graph.argument(pre_edge, EntityId::Loan(hs))? == EntityId::Loan(lease)
            && graph.argument(pre_edge, EntityId::Value(hc))? == EntityId::Value(value(first)?)
            && graph.transported(when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && graph.transported(when_true, EntityId::Value(hc))? == EntityId::Value(bc),
        "identity SSA actual First seed and current header/body lease/cursor",
        &graph.edges,
    )?;
    let false_source = graph.transported(when_false, EntityId::Loan(hs))?;
    let false_cursor = graph.transported(when_false, EntityId::Value(hc))?;
    let seed = *graph
        .block(pre)?
        .parameters
        .get(input.parameter_slot)
        .ok_or_else(|| failure("identity SSA FE parameter slot", input))?;
    let seed_data = scan
        .entity(seed)
        .ok_or_else(|| failure("identity SSA original parameter", seed))?;
    need(
        graph.block(pre)?.parameters.len() == 1
            && scan.receiver.is_none()
            && seed_data.definition
                == Definition::BlockParameter {
                    block: pre,
                    index: input.parameter_slot,
                }
            && seed_data.origin == Origin::Source(input.scan)
            && input.parameter.source_id() == input.scan.source_id()
            && input.scan.start() <= input.parameter.start()
            && input.parameter.end() <= input.scan.end(),
        "identity SSA entry parameter is FE original slot",
        seed_data,
    )?;
    let map_type = seed_data.ty.semantic_type();
    let (kind, kt, vt) = module
        .map_container(map_type)
        .ok_or_else(|| failure("identity SSA actual Map type", map_type))?;
    need(
        kind == if input.owned {
            MapContainerKind::Map
        } else {
            MapContainerKind::MutableMap
        } && module.type_kind(kt) == Some(&SsaTypeKind::StringOwner)
            && module.type_kind(vt) == Some(&SsaTypeKind::StringOwner),
        "identity SSA source and K/V actual storage",
        (map_type, kind, kt, vt),
    )?;
    let shared_map = EntityType::Loan {
        kind: LoanKind::Shared,
        target: map_type,
    };
    for entity in [
        EntityId::Loan(lease),
        EntityId::Loan(hs),
        EntityId::Loan(bs),
        false_source,
    ] {
        need(
            scan.entity(entity).is_some_and(|d| d.ty == shared_map),
            "identity SSA source Shared type",
            entity,
        )?;
    }
    need(scan.entity(result(guard)?).is_some_and(|d| matches!(d.ty, EntityType::Value(t) if module.type_kind(t) == Some(&SsaTypeKind::Boolean))),
        "identity SSA actual HasNext Boolean", guard)?;
    for entity in [
        result(first)?,
        EntityId::Value(hc),
        EntityId::Value(bc),
        false_cursor,
    ] {
        need(scan.entity(entity).is_some_and(|d| matches!(d.ty, EntityType::Value(t) if module.type_kind(t) == Some(&SsaTypeKind::Integer { bits: 64, signed: false }))),
            "identity SSA actual u64 cursor", entity)?;
    }
    need(
        loan(key)? != loan(val)?,
        "identity SSA independent K/V identities",
        (key, val),
    )?;
    for (entity, target) in [(result(key)?, kt), (result(val)?, vt)] {
        need(
            scan.entity(entity).is_some_and(|d| {
                d.ty == EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }
            }),
            "identity SSA two Shared component types",
            entity,
        )?;
    }
    let mut uses = Uses::default();
    for (entity, operations) in [
        (EntityId::Loan(lease), vec![first]),
        (EntityId::Loan(hs), vec![guard]),
        (EntityId::Loan(bs), vec![key, val]),
        (EntityId::Value(hc), vec![guard]),
        (EntityId::Value(bc), vec![key, val]),
    ] {
        for i in operations {
            uses.permit(entity, i);
        }
    }
    for entity in [seed, result(first)?, false_source, false_cursor] {
        uses.protect(entity);
    }
    let birth = graph.definition(EntityId::Loan(lease))?;
    need(
        birth.block == pre && birth.origin == Origin::Source(input.site.span),
        "identity SSA lease birth at actual For",
        birth,
    )?;
    let mut pre_ops = vec![];
    match (input.owned, seed, &birth.operation) {
        (
            true,
            EntityId::Value(owner),
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
        ) => {
            let root = graph.definition(EntityId::Place(*place))?;
            need(
                root.block == pre
                    && root.operation == Operation::RootPlace { owner }
                    && root.origin == Origin::Source(input.site.span),
                "identity SSA lease Place rooted at actual owner",
                root,
            )?;
            need(
                scan.entity(EntityId::Place(*place))
                    .is_some_and(|d| d.ty == EntityType::Place(map_type)),
                "identity SSA root Place type",
                root,
            )?;
            uses.permit(seed, root);
            uses.permit(EntityId::Place(*place), birth);
            pre_ops.push(root.id);
        }
        (false, EntityId::Loan(entry), Operation::SharedReborrow { source }) => {
            need(
                *source == entry && lease != entry && seed_data.ty == shared_map,
                "identity SSA own provider reborrow distinct from caller Borrow",
                birth,
            )?;
            uses.permit(seed, birth);
        }
        _ => {
            return Err(failure(
                "identity SSA actual provider acquisition",
                (seed, birth),
            ));
        }
    }
    pre_ops.extend([birth.id, first.id]);
    need(
        graph.ops(pre)?.iter().map(|i| i.id).collect::<Vec<_>>() == pre_ops,
        "identity SSA unique source evaluation/lease/First order",
        graph.ops(pre)?,
    )?;
    let header_seed = graph
        .block(header)?
        .parameters
        .iter()
        .copied()
        .filter(|&p| graph.argument(pre_edge, p).is_ok_and(|v| v == seed))
        .collect::<Vec<_>>();
    need(
        header_seed.len() <= 1 && (!input.owned || header_seed.len() == 1),
        "identity SSA optional caller Borrow or required owner column",
        &header_seed,
    )?;
    let mut seeds = vec![seed];
    let mut expected_pre = vec![EntityId::Loan(lease), result(first)?];
    if !header_seed.is_empty() {
        expected_pre.push(seed);
    }
    need(
        pre_edge.arguments.len() == expected_pre.len()
            && pre_edge.arguments.iter().copied().collect::<BTreeSet<_>>()
                == expected_pre.iter().copied().collect::<BTreeSet<_>>(),
        "identity SSA every preheader column grounded in source/lease/First",
        pre_edge,
    )?;
    for edge in [when_true, when_false] {
        let parameters = &graph.block(header)?.parameters;
        need(
            edge.arguments.len() == parameters.len()
                && edge.arguments.iter().copied().collect::<BTreeSet<_>>()
                    == parameters.iter().copied().collect::<BTreeSet<_>>(),
            "identity SSA every true/false column transports current header entity",
            edge,
        )?;
    }
    let (body_seed, false_seed) = if let Some(&hs) = header_seed.first() {
        let bs = graph.transported(when_true, hs)?;
        let fs = graph.transported(when_false, hs)?;
        seeds.extend([hs, bs, fs]);
        (Some(bs), Some(fs))
    } else {
        (None, None)
    };
    for &entity in &seeds {
        uses.protect(entity);
    }
    let false_cleanup = query_cleanup::end_source(
        &graph,
        input,
        exhausted,
        false_source,
        false_seed,
        &mut uses,
    )?;
    need(
        graph
            .ops(exhausted)?
            .iter()
            .take(false_cleanup.len())
            .map(|i| i.id)
            .collect::<Vec<_>>()
            == false_cleanup,
        "identity SSA zero-round false starts its own terminal cleanup",
        graph.ops(exhausted)?,
    )?;
    let tail = match input.exit {
        Exit::Continue => {
            let next = one(nexts.into_iter(), "identity SSA Continue Next")?;
            need(
                next.block == body
                    && next.origin == Origin::Source(input.site.transfer_span)
                    && next.operation
                        == Operation::MapIterationNext {
                            source: bs,
                            cursor: bc,
                        },
                "identity SSA Continue Next uses current round source/cursor",
                next,
            )?;
            let next_result = result(next)?;
            need(
                scan.entity(next_result).map(|d| d.ty)
                    == scan.entity(EntityId::Value(hc)).map(|d| d.ty),
                "identity SSA Next cursor result type",
                next,
            )?;
            uses.permit(EntityId::Loan(bs), next);
            uses.permit(EntityId::Value(bc), next);
            uses.protect(next_result);
            let back = graph.branch(body)?;
            need(
                back.target == header
                    && graph.term(body)?.origin == Origin::Source(input.site.transfer_span),
                "identity SSA Continue targets actual Header",
                back,
            )?;
            for &parameter in &graph.block(header)?.parameters {
                let initial = graph.argument(pre_edge, parameter)?;
                let expected = if initial == EntityId::Loan(lease) {
                    EntityId::Loan(bs)
                } else if initial == result(first)? {
                    next_result
                } else {
                    body_seed
                        .ok_or_else(|| failure("identity SSA backedge source carrier", initial))?
                };
                need(
                    graph.argument(back, parameter)? == expected,
                    "identity SSA every Header backedge column current owner/lease/Next",
                    (parameter, back),
                )?;
            }
            vec![next.id]
        }
        Exit::Break | Exit::Return => query_cleanup::end_source(
            &graph,
            input,
            body,
            EntityId::Loan(bs),
            body_seed,
            &mut uses,
        )?,
    };
    let returned = query_cleanup::body(&graph, module, input, [key, val], &tail, &mut uses)?;
    match input.exit {
        Exit::Break => {
            let from_body = graph.branch(body)?;
            let from_false = graph.branch(exhausted)?;
            let merge = from_body.target;
            need(
                from_false.target == merge,
                "identity SSA separate break/exhaustion cleanup join",
                (from_body, from_false),
            )?;
            graph.incoming_from(merge, &[body, exhausted])?;
            let carried_borrow = if input.owned {
                None
            } else {
                body_seed.zip(false_seed)
            };
            if let Some((b, f)) = carried_borrow {
                need(
                    from_body.arguments.as_slice() == [b]
                        && from_false.arguments.as_slice() == [f]
                        && graph.transported(from_body, b)? == graph.transported(from_false, f)?,
                    "identity SSA all merge incoming preserve same caller Borrow",
                    (from_body, from_false),
                )?;
                let merged = graph.transported(from_body, b)?;
                uses.protect(merged);
                seeds.push(merged);
            } else {
                need(
                    from_body.arguments.is_empty()
                        && from_false.arguments.is_empty()
                        && graph.block(merge)?.parameters.is_empty(),
                    "identity SSA ended owners/leases do not enter Break merge",
                    (from_body, from_false),
                )?;
            }
            need(
                matches!(&graph.term(merge)?.kind, TerminatorKind::Return { values } if values.is_empty()),
                "identity SSA Break merge leaves function without fake backedge",
                graph.term(merge)?,
            )?;
        }
        Exit::Continue => need(
            matches!(&graph.term(exhausted)?.kind, TerminatorKind::Return { values } if values.is_empty()),
            "identity SSA Continue exhaustion terminates without backedge",
            graph.term(exhausted)?,
        )?,
        Exit::Return => {
            let TerminatorKind::Return { values } = &graph.term(exhausted)?.kind else {
                return Err(failure(
                    "identity SSA Return exhaustion terminal",
                    graph.term(exhausted)?,
                ));
            };
            let zero = one(
                values.iter().copied(),
                "identity SSA exhausted fallback result",
            )?;
            let producer = graph.definition(EntityId::Value(zero))?;
            need(
                producer.block == exhausted
                    && producer.operation == Operation::Constant(ScalarConstant::Integer(0)),
                "identity SSA exhausted path returns actual zero",
                producer,
            )?;
        }
    }
    let expected_edges = match input.exit {
        Exit::Continue => 4,
        Exit::Break => 5,
        Exit::Return => 3,
    };
    need(
        graph.edges.len() == expected_edges,
        "identity SSA all CFG edges counted without hidden transfer",
        &graph.edges,
    )?;
    // Every seed alias and provider capability use is accounted for, including false/merge paths.
    // In Borrow mode only the original seed may feed the initial reborrow; no alias is ended.
    uses.check(&graph, returned.map(|value| (body, value)))?;
    println!(
        "SPEC0331 EXIT IDENTITY SSA scan={:?} roles={roles:?} seed_aliases={seeds:?} source={:?} cursor={:?} edges={:#?} returned={returned:?}",
        scan.id,
        [
            EntityId::Loan(lease),
            EntityId::Loan(hs),
            EntityId::Loan(bs),
            false_source
        ],
        [
            result(first)?,
            EntityId::Value(hc),
            EntityId::Value(bc),
            false_cursor
        ],
        graph.edges
    );
    Ok(())
}
