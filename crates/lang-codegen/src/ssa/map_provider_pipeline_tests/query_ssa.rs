//! 只读真实 SSA 图；所有列由 operand/entity 发现，不调用生产 carrier/qualify。
use super::{
    fixture::{Failure, failure},
    query_inputs::{Inputs, need, one},
};
use crate::ssa::model::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
pub(super) struct Shape {
    pub scan: FunctionId,
    pub read: FunctionId,
    pub blocks: [BlockId; 4],
    pub operations: [InstructionId; 5],
    pub seed: EntityId,
    pub lease: LoanId,
    pub header_source: LoanId,
    pub header_cursor: ValueId,
    pub body_source: LoanId,
    pub body_cursor: ValueId,
    pub edges: Vec<(BlockId, Edge)>,
}

pub(super) fn instructions<'a>(
    f: &'a Function,
    blocks: &[BlockId],
) -> Result<Vec<&'a Instruction>, Failure> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for &id in blocks {
        let block = f
            .block(id)
            .ok_or_else(|| failure("SSA missing reachable block", id))?;
        for &id in &block.instructions {
            let i = f
                .instruction(id)
                .ok_or_else(|| failure("SSA referenced instruction", id))?;
            need(
                i.block == block.id && seen.insert(id),
                "SSA block instruction identity",
                i,
            )?;
            result.push(i);
        }
    }
    Ok(result)
}

type Reachable = (Vec<BlockId>, Vec<(BlockId, Edge)>);

fn reachable(f: &Function) -> Result<Reachable, Failure> {
    let mut todo = vec![
        f.entry_block()
            .ok_or_else(|| failure("SSA entry block", f.id))?,
    ];
    let mut blocks = Vec::new();
    let mut edges = Vec::new();
    while let Some(id) = todo.pop() {
        if blocks.contains(&id) {
            continue;
        }
        let block = f.block(id).ok_or_else(|| failure("SSA CFG block", id))?;
        blocks.push(id);
        let term = block
            .terminator
            .as_ref()
            .ok_or_else(|| failure("SSA terminator", id))?;
        let outgoing = match &term.kind {
            TerminatorKind::Branch(edge) => vec![edge],
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => vec![when_true, when_false],
            TerminatorKind::Return { values } if values.is_empty() => vec![],
            _ => return Err(failure("SSA normal fixture unexpected terminator", term)),
        };
        for edge in outgoing {
            let target = f
                .block(edge.target)
                .ok_or_else(|| failure("SSA edge target", edge))?;
            need(
                edge.arguments.len() == target.parameters.len(),
                "SSA edge full arity",
                edge,
            )?;
            for (index, (&argument, &parameter)) in
                edge.arguments.iter().zip(&target.parameters).enumerate()
            {
                let a = f
                    .entity(argument)
                    .ok_or_else(|| failure("SSA edge argument", argument))?;
                let p = f
                    .entity(parameter)
                    .ok_or_else(|| failure("SSA edge parameter", parameter))?;
                need(
                    a.ty == p.ty
                        && p.definition
                            == Definition::BlockParameter {
                                block: target.id,
                                index,
                            },
                    "SSA every edge parameter identity/type",
                    (edge, parameter),
                )?;
            }
            edges.push((id, edge.clone()));
            todo.push(edge.target);
        }
    }
    Ok((blocks, edges))
}

fn result(i: &Instruction) -> Result<EntityId, Failure> {
    one(i.results.iter().copied(), "SSA operation unique result")
}
fn value(i: &Instruction) -> Result<ValueId, Failure> {
    match result(i)? {
        EntityId::Value(v) => Ok(v),
        e => Err(failure("SSA Value result", e)),
    }
}
fn loan(i: &Instruction) -> Result<LoanId, Failure> {
    match result(i)? {
        EntityId::Loan(v) => Ok(v),
        e => Err(failure("SSA Loan result", e)),
    }
}
fn definition(f: &Function, entity: EntityId) -> Result<&Instruction, Failure> {
    let data = f
        .entity(entity)
        .ok_or_else(|| failure("SSA definition entity", entity))?;
    let Definition::InstructionResult { instruction, index } = data.definition else {
        return Err(failure("SSA instruction definition", data));
    };
    let i = f
        .instruction(instruction)
        .ok_or_else(|| failure("SSA definition instruction", instruction))?;
    need(
        i.results.get(index) == Some(&entity),
        "SSA result definition identity",
        (entity, i),
    )?;
    Ok(i)
}
fn argument(f: &Function, edge: &Edge, parameter: EntityId) -> Result<EntityId, Failure> {
    let data = f
        .entity(parameter)
        .ok_or_else(|| failure("SSA transport parameter", parameter))?;
    let Definition::BlockParameter { block, index } = data.definition else {
        return Err(failure("SSA transport is not parameter", parameter));
    };
    need(
        block == edge.target,
        "SSA transported target identity",
        (edge, parameter),
    )?;
    edge.arguments
        .get(index)
        .copied()
        .ok_or_else(|| failure("SSA transport argument", edge))
}
fn transported(f: &Function, edge: &Edge, input: EntityId) -> Result<EntityId, Failure> {
    let index = one(
        edge.arguments
            .iter()
            .enumerate()
            .filter_map(|(i, &arg)| (arg == input).then_some(i)),
        "SSA unique transported entity column",
    )?;
    Ok(f.block(edge.target)
        .ok_or_else(|| failure("SSA target block", edge))?
        .parameters[index])
}
fn position(f: &Function, i: &Instruction) -> Result<usize, Failure> {
    let block = f
        .block(i.block)
        .ok_or_else(|| failure("SSA instruction block", i))?;
    one(
        block
            .instructions
            .iter()
            .enumerate()
            .filter_map(|(n, id)| (*id == i.id).then_some(n)),
        "SSA instruction position",
    )
}

pub(super) fn check(program: &Program, input: &Inputs) -> Result<Shape, Failure> {
    let module = one(program.modules.iter(), "SSA identity module")?;
    let scan = one(
        module
            .functions
            .iter()
            .filter(|f| f.origin == Origin::Source(input.scan)),
        "SSA actual scan origin",
    )?;
    let read = one(
        module
            .functions
            .iter()
            .filter(|f| f.origin == Origin::Source(input.read)),
        "SSA actual read origin",
    )?;
    let (blocks, edges) = reachable(scan)?;
    let all = instructions(scan, &blocks)?;
    let first = one(
        all.iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::MapIterationFirst { .. })),
        "SSA one First",
    )?;
    let guard = one(
        all.iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::MapIterationHasNext { .. })),
        "SSA one HasNext",
    )?;
    let key = one(
        all.iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::MapIterationKeyLoan { .. })),
        "SSA one KeyLoan",
    )?;
    let val = one(
        all.iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::MapIterationValueLoan { .. })),
        "SSA one ValueLoan",
    )?;
    let next = one(
        all.iter()
            .copied()
            .filter(|i| matches!(i.operation, Operation::MapIterationNext { .. })),
        "SSA one Next",
    )?;
    let Operation::MapIterationFirst { source: lease } = first.operation else {
        return Err(failure("SSA First", first));
    };
    let Operation::MapIterationHasNext {
        source: hs,
        cursor: hc,
    } = guard.operation
    else {
        return Err(failure("SSA HasNext", guard));
    };
    let Operation::MapIterationKeyLoan {
        source: bs,
        cursor: bc,
    } = key.operation
    else {
        return Err(failure("SSA KeyLoan", key));
    };
    need(
        val.operation
            == Operation::MapIterationValueLoan {
                source: bs,
                cursor: bc,
            }
            && next.operation
                == Operation::MapIterationNext {
                    source: bs,
                    cursor: bc,
                },
        "SSA same body source/current cursor",
        (key, val, next),
    )?;
    for i in [first, guard, key, val, next] {
        let output = result(i)?;
        need(
            scan.entity(output).is_some_and(|data| {
                data.definition
                    == Definition::InstructionResult {
                        instruction: i.id,
                        index: 0,
                    }
            }),
            "SSA actual provider result definition",
            (i, output),
        )?;
        need(
            i.origin == Origin::Source(input.loop_span),
            "SSA provider source origin",
            i,
        )?;
    }
    let pre = first.block;
    let header = guard.block;
    let body = key.block;
    let h = scan
        .block(header)
        .ok_or_else(|| failure("SSA header", header))?;
    let Some(Terminator {
        kind:
            TerminatorKind::Conditional {
                condition,
                when_true,
                when_false,
            },
        ..
    }) = &h.terminator
    else {
        return Err(failure("SSA HasNext conditional", h));
    };
    need(
        *condition == value(guard)?
            && when_true.target == body
            && val.block == body
            && next.block == body,
        "SSA actual true edge and body operations",
        h,
    )?;
    let exit = when_false.target;
    let roles = [pre, header, body, exit];
    let expected_blocks = roles.into_iter().collect::<BTreeSet<_>>();
    need(
        expected_blocks.len() == 4
            && blocks.iter().copied().collect::<BTreeSet<_>>() == expected_blocks
            && scan.entry_block() == Some(pre),
        "SSA normal reachable CFG closure",
        (&blocks, roles),
    )?;
    let incoming = |target| {
        edges
            .iter()
            .filter(|(_, e)| e.target == target)
            .collect::<Vec<_>>()
    };
    let pre_edge = one(
        incoming(header)
            .into_iter()
            .filter_map(|(from, e)| (*from == pre).then_some(e)),
        "SSA header entry edge",
    )?;
    let back = one(
        incoming(header)
            .into_iter()
            .filter_map(|(from, e)| (*from == body).then_some(e)),
        "SSA header backedge",
    )?;
    need(
        edges.len() == 4
            && incoming(pre).is_empty()
            && incoming(header).len() == 2
            && incoming(body).len() == 1
            && incoming(exit).len() == 1
            && incoming(body)[0].0 == header
            && incoming(exit)[0].0 == header,
        "SSA all actual incoming edges closed",
        &edges,
    )?;
    need(
        argument(scan, pre_edge, EntityId::Loan(hs))? == EntityId::Loan(lease)
            && argument(scan, pre_edge, EntityId::Value(hc))? == EntityId::Value(value(first)?)
            && transported(scan, when_true, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && transported(scan, when_true, EntityId::Value(hc))? == EntityId::Value(bc)
            && argument(scan, back, EntityId::Loan(hs))? == EntityId::Loan(bs)
            && argument(scan, back, EntityId::Value(hc))? == EntityId::Value(value(next)?),
        "SSA seed and current-round Next/source backedge",
        &edges,
    )?;
    let pre_block = scan
        .block(pre)
        .ok_or_else(|| failure("SSA preheader", pre))?;
    let seed = *pre_block
        .parameters
        .get(input.parameter_slot)
        .ok_or_else(|| failure("SSA FE parameter slot", input))?;
    let seed_data = scan
        .entity(seed)
        .ok_or_else(|| failure("SSA source parameter", seed))?;
    need(
        input.parameter.source_id() == input.scan.source_id()
            && input.scan.start() <= input.parameter.start()
            && input.parameter.end() <= input.scan.end()
            && seed_data.origin == Origin::Source(input.scan),
        "SSA parameter slot belongs to exact FE source function",
        (input.parameter, seed_data),
    )?;
    need(
        pre_block.parameters.len() == 1
            && seed_data.definition
                == Definition::BlockParameter {
                    block: pre,
                    index: input.parameter_slot,
                },
        "SSA actual source entry parameter",
        seed_data,
    )?;
    let map_type = seed_data.ty.semantic_type();
    let (kind, kt, vt) = module
        .map_container(map_type)
        .ok_or_else(|| failure("SSA actual Map type", map_type))?;
    need(
        kind == if input.owned {
            MapContainerKind::Map
        } else {
            MapContainerKind::MutableMap
        } && module.type_kind(kt) == Some(&SsaTypeKind::StringOwner)
            && module.type_kind(vt) == Some(&SsaTypeKind::StringOwner),
        "SSA FE kind/K/V mapped storage",
        (map_type, kind, kt, vt),
    )?;
    need(scan.entity(EntityId::Value(value(guard)?)).is_some_and(|data| matches!(data.ty, EntityType::Value(ty) if module.type_kind(ty) == Some(&SsaTypeKind::Boolean))),
        "SSA actual HasNext result is Boolean", guard)?;
    let shared_map = EntityType::Loan {
        kind: LoanKind::Shared,
        target: map_type,
    };
    for entity in [
        EntityId::Loan(lease),
        EntityId::Loan(hs),
        EntityId::Loan(bs),
    ] {
        need(
            scan.entity(entity).is_some_and(|d| d.ty == shared_map),
            "SSA source Shared capability",
            entity,
        )?;
    }
    for entity in [
        EntityId::Value(value(first)?),
        EntityId::Value(hc),
        EntityId::Value(bc),
        EntityId::Value(value(next)?),
    ] {
        need(
            scan.entity(entity).is_some_and(|d| {
                matches!(d.ty, EntityType::Value(ty)
            if module.type_kind(ty) == Some(&SsaTypeKind::Integer { bits: 64, signed: false }))
            }),
            "SSA u64 cursor identity",
            entity,
        )?;
    }
    let birth = definition(scan, EntityId::Loan(lease))?;
    need(
        birth.block == pre
            && birth.origin == Origin::Source(input.loop_span)
            && position(scan, birth)? < position(scan, first)?,
        "SSA lease before unique First",
        birth,
    )?;
    match (&birth.operation, seed, input.owned) {
        (
            Operation::BorrowBegin {
                place,
                kind: LoanKind::Shared,
            },
            EntityId::Value(owner),
            true,
        ) => {
            let root = definition(scan, EntityId::Place(*place))?;
            need(
                root.operation == Operation::RootPlace { owner }
                    && root.block == pre
                    && position(scan, root)? < position(scan, birth)?,
                "SSA lease root is actual entry owner",
                root,
            )?;
        }
        (Operation::SharedReborrow { source }, EntityId::Loan(entry), false) => {
            need(
                *source == entry && lease != entry,
                "SSA own lease distinct from entry Borrow",
                birth,
            )?;
        }
        _ => return Err(failure("SSA actual source access birth", (birth, seed))),
    }
    let key_loan = loan(key)?;
    let value_loan = loan(val)?;
    need(
        key_loan != value_loan,
        "SSA two distinct component loans",
        (key_loan, value_loan),
    )?;
    for (entity, target) in [
        (EntityId::Loan(key_loan), kt),
        (EntityId::Loan(value_loan), vt),
    ] {
        need(
            scan.entity(entity).is_some_and(|d| {
                d.ty == EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }
            }),
            "SSA actual component Shared K/V",
            entity,
        )?;
    }
    let read_entry = read
        .entry_block()
        .and_then(|id| read.block(id))
        .ok_or_else(|| failure("SSA read entry", read.id))?;
    need(
        read_entry.parameters.len() == 2
            && read.return_types.is_empty()
            && read.borrow_return.is_none(),
        "SSA read actual signature",
        read,
    )?;
    for (&p, target) in read_entry.parameters.iter().zip([kt, vt]) {
        need(
            read.entity(p).is_some_and(|d| {
                d.ty == EntityType::Loan {
                    kind: LoanKind::Shared,
                    target,
                }
            }),
            "SSA read Borrow parameter",
            p,
        )?;
    }
    body_lifetimes(scan, read.id, input, key, val, next)?;
    let exit_source = transported(scan, when_false, EntityId::Loan(hs))?;
    let exit_cursor = transported(scan, when_false, EntityId::Value(hc))?;
    need(
        scan.entity(exit_source).is_some_and(|d| d.ty == shared_map),
        "SSA false current source",
        exit_source,
    )?;
    let exit_ops = instructions(scan, &[exit])?;
    let end = one(exit_ops.iter().copied().filter(|i| matches!(i.operation, Operation::BorrowEnd { loan } if EntityId::Loan(loan) == exit_source)), "SSA false EndSource")?;
    need(
        end.origin == Origin::Source(input.loop_span)
            && exit_ops.first().map(|i| i.id) == Some(end.id),
        "SSA zero-round false begins EndSource without projections",
        &exit_ops,
    )?;
    let seed_headers = pre_edge
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(i, &e)| (e == seed).then_some(h.parameters[i]))
        .collect::<Vec<_>>();
    need(
        seed_headers.len() <= 1 && (!input.owned || seed_headers.len() == 1),
        "SSA original source carrier column",
        &seed_headers,
    )?;
    let first_cursor = EntityId::Value(value(first)?);
    need(
        pre_edge.arguments.len() == 2 + seed_headers.len()
            && pre_edge
                .arguments
                .iter()
                .all(|e| [seed, EntityId::Loan(lease), first_cursor].contains(e)),
        "SSA every carried column grounded at actual source/lease/First",
        pre_edge,
    )?;
    let mut source_drop = None;
    let mut original_aliases = vec![seed];
    if let Some(&header_seed) = seed_headers.first() {
        let body_seed = transported(scan, when_true, header_seed)?;
        let exit_seed = transported(scan, when_false, header_seed)?;
        need(
            argument(scan, back, header_seed)? == body_seed,
            "SSA source current carrier backedge",
            back,
        )?;
        original_aliases.extend([header_seed, body_seed, exit_seed]);
        if input.owned {
            let EntityId::Value(owner) = exit_seed else {
                return Err(failure("SSA exhausted owner carrier", exit_seed));
            };
            let drop = one(all.iter().copied().filter(|i| matches!(i.operation, Operation::Drop { owner: o } if original_aliases.contains(&EntityId::Value(o)))), "SSA exactly one source owner Drop")?;
            need(
                drop.operation == Operation::Drop { owner }
                    && drop.block == exit
                    && drop.origin.span()
                        == input
                            .drop_origin
                            .ok_or_else(|| failure("SSA FE Drop origin missing", input))?
                    && position(scan, drop)? == position(scan, end)? + 1,
                "SSA EndSource then actual owner Drop",
                drop,
            )?;
            source_drop = Some(drop.id);
        }
    }
    if !input.owned {
        need(all.iter().all(|i| !matches!(i.operation, Operation::BorrowEnd { loan } if original_aliases.contains(&EntityId::Loan(loan)))),
            "SSA caller Borrow and every transported alias remain unended", &original_aliases)?;
    }
    let root_instruction = match birth.operation {
        Operation::BorrowBegin { place, .. } => Some(definition(scan, EntityId::Place(place))?.id),
        _ => None,
    };
    let cursors = [
        first_cursor,
        EntityId::Value(hc),
        EntityId::Value(bc),
        EntityId::Value(value(next)?),
        exit_cursor,
    ];
    // Only exhaustion ends the provider's lease; every other use is an exact provider op/edge.
    for i in &all {
        for e in i.operation.entities() {
            if original_aliases.contains(&e) {
                let allowed = if input.owned {
                    Some(i.id) == root_instruction || Some(i.id) == source_drop
                } else {
                    i.id == birth.id
                };
                need(allowed, "SSA complete original owner/caller Borrow uses", i)?;
            }
            if cursors.contains(&e) {
                need(
                    [guard.id, key.id, val.id, next.id].contains(&i.id),
                    "SSA complete cursor operation uses",
                    i,
                )?;
            }
            if [
                EntityId::Loan(lease),
                EntityId::Loan(hs),
                EntityId::Loan(bs),
                exit_source,
            ]
            .contains(&e)
            {
                need(
                    [first.id, guard.id, key.id, val.id, next.id, end.id].contains(&i.id),
                    "SSA complete source lease uses",
                    i,
                )?;
            }
            need(
                e != exit_cursor,
                "SSA zero-round false does not dereference cursor",
                i,
            )?;
        }
    }
    for &block in &blocks {
        let term = scan
            .block(block)
            .and_then(|b| b.terminator.as_ref())
            .ok_or_else(|| failure("SSA terminal use query", block))?;
        for entity in term.kind.entities() {
            let protected = original_aliases.contains(&entity)
                || cursors.contains(&entity)
                || [
                    EntityId::Loan(lease),
                    EntityId::Loan(hs),
                    EntityId::Loan(bs),
                    exit_source,
                ]
                .contains(&entity);
            if protected {
                need(
                    edges
                        .iter()
                        .any(|(from, edge)| *from == block && edge.arguments.contains(&entity)),
                    "SSA protected entity only on checked actual transport edge",
                    (block, entity, term),
                )?;
            }
        }
    }
    let shape = Shape {
        scan: scan.id,
        read: read.id,
        blocks: roles,
        operations: [first.id, guard.id, key.id, val.id, next.id],
        seed,
        lease,
        header_source: hs,
        header_cursor: hc,
        body_source: bs,
        body_cursor: bc,
        edges,
    };
    super::query_owner::check(module, scan, input, &shape)?;
    println!("SPEC0331 PROVIDER IDENTITY SSA {shape:#?}");
    Ok(shape)
}

fn body_lifetimes(
    f: &Function,
    read: FunctionId,
    input: &Inputs,
    key: &Instruction,
    val: &Instruction,
    next: &Instruction,
) -> Result<(), Failure> {
    let k = loan(key)?;
    let v = loan(val)?;
    let mut roots = BTreeMap::<LoanId, (LoanId, Option<LoanId>)>::new();
    let mut live = BTreeSet::new();
    let mut ends = Vec::new();
    let mut calls = 0;
    let ops = instructions(f, &[key.block])?;
    for i in &ops {
        match &i.operation {
            Operation::MapIterationKeyLoan { .. } | Operation::MapIterationValueLoan { .. } => {
                let l = loan(i)?;
                need(
                    [k, v].contains(&l) && roots.insert(l, (l, None)).is_none() && live.insert(l),
                    "SSA component birth",
                    i,
                )?;
            }
            Operation::SharedReborrow { source } => {
                let (root, _) = *roots
                    .get(source)
                    .ok_or_else(|| failure("SSA body reborrow unknown root", i))?;
                let l = loan(i)?;
                need(
                    f.entity(EntityId::Loan(l)).map(|d| d.ty)
                        == f.entity(EntityId::Loan(*source)).map(|d| d.ty)
                        && live.contains(source)
                        && roots.insert(l, (root, Some(*source))).is_none()
                        && live.insert(l),
                    "SSA real component-derived loan chain",
                    i,
                )?;
            }
            Operation::DirectCall {
                callee,
                receiver,
                arguments,
            } => {
                need(
                    receiver.is_none()
                        && *callee == read
                        && i.origin == Origin::Source(input.call)
                        && arguments.len() == 2,
                    "SSA exact source read callee/call",
                    i,
                )?;
                for (&argument, expected_root) in arguments.iter().zip([k, v]) {
                    let EntityId::Loan(l) = argument else {
                        return Err(failure("SSA read must deliver Borrow", argument));
                    };
                    need(
                        live.contains(&l)
                            && roots.get(&l).is_some_and(|(r, _)| *r == expected_root),
                        "SSA ordered actual K/V call delivery",
                        i,
                    )?;
                }
                calls += 1;
            }
            Operation::BorrowEnd { loan } => {
                need(
                    live.contains(loan)
                        && !roots.iter().any(|(child, (_, parent))| {
                            *parent == Some(*loan) && live.contains(child)
                        }),
                    "SSA derived loan ends before its actual parent",
                    i,
                )?;
                live.remove(loan);
                if [k, v].contains(loan) {
                    ends.push(*loan);
                }
            }
            Operation::MapIterationNext { .. } => {
                need(
                    i.id == next.id && live.is_empty() && ends == [v, k] && calls == 1,
                    "SSA all derived loans then value/key end before Next",
                    (i, &live, &ends, calls),
                )?;
            }
            _ => return Err(failure("SSA ordinary body unexpected component use", i)),
        }
    }
    need(
        calls == 1
            && live.is_empty()
            && ends == [v, k]
            && ops.first().map(|i| i.id) == Some(key.id)
            && position(f, key)? < position(f, val)?
            && ops.last().map(|i| i.id) == Some(next.id),
        "SSA body exact lifetime closure",
        (&ops, ends),
    )?;
    // Scan all reachable operation and edge operands, not just intended uses.
    let (blocks, edges) = reachable(f)?;
    for i in instructions(f, &blocks)? {
        if i.operation
            .entities()
            .iter()
            .any(|e| matches!(e, EntityId::Loan(l) if roots.contains_key(l)))
        {
            need(
                i.block == key.block,
                "SSA component cannot escape body operations",
                i,
            )?;
        }
    }
    need(
        edges.iter().all(|(_, e)| {
            e.arguments
                .iter()
                .all(|a| !matches!(a, EntityId::Loan(l) if roots.contains_key(l)))
        }),
        "SSA no component/derived loan on backedge or exit",
        edges,
    )
}
