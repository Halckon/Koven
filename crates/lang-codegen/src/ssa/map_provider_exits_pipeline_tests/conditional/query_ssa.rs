//! 真实 roots → 三 If 六边 → 两回边/两出口；每一列独立追到实际 operand。
use super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, loan, result, value},
    query_inputs::one,
};
use super::{
    ast::{Anchors, Input},
    query_cleanup,
    query_inputs::Inputs,
};
use crate::ssa::model::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug)]
pub(super) struct Roles {
    pub source: LoanId,
    pub cursor: ValueId,
    pub seed: Option<EntityId>,
    pub body: Option<(LoanId, LoanId, ValueId)>,
}

impl Roles {
    fn entities(self) -> Vec<EntityId> {
        let mut entities = vec![EntityId::Loan(self.source), EntityId::Value(self.cursor)];
        entities.extend(self.seed);
        if let Some((key, value, guard)) = self.body {
            entities.extend([
                EntityId::Loan(key),
                EntityId::Loan(value),
                EntityId::Value(guard),
            ]);
        }
        entities
    }
    fn protect(self, uses: &mut Uses) {
        for entity in self.entities() {
            uses.protect(entity);
        }
    }
}

fn as_loan(entity: EntityId) -> Result<LoanId, Failure> {
    match entity {
        EntityId::Loan(id) => Ok(id),
        _ => Err(failure("conditional identity actual Loan column", entity)),
    }
}
fn as_value(entity: EntityId) -> Result<ValueId, Failure> {
    match entity {
        EntityId::Value(id) => Ok(id),
        _ => Err(failure("conditional identity actual Value column", entity)),
    }
}

fn transport(
    graph: &Graph<'_>,
    edge: &Edge,
    roles: Roles,
    uses: &mut Uses,
) -> Result<Roles, Failure> {
    let expected = roles.entities();
    let set = |values: &[EntityId]| values.iter().copied().collect::<BTreeSet<_>>();
    need(
        expected.len() == set(&expected).len()
            && edge.arguments.len() == expected.len()
            && set(&edge.arguments) == set(&expected),
        "conditional identity every edge has precisely current role operands",
        (edge, roles),
    )?;
    let next = Roles {
        source: as_loan(graph.transported(edge, EntityId::Loan(roles.source))?)?,
        cursor: as_value(graph.transported(edge, EntityId::Value(roles.cursor))?)?,
        seed: roles.seed.map(|e| graph.transported(edge, e)).transpose()?,
        body: roles
            .body
            .map(|(k, v, g)| -> Result<_, Failure> {
                Ok((
                    as_loan(graph.transported(edge, EntityId::Loan(k))?)?,
                    as_loan(graph.transported(edge, EntityId::Loan(v))?)?,
                    as_value(graph.transported(edge, EntityId::Value(g))?)?,
                ))
            })
            .transpose()?,
    };
    next.protect(uses);
    Ok(next)
}

fn conditional<'a>(
    graph: &Graph<'a>,
    block: BlockId,
) -> Result<(ValueId, &'a Edge, &'a Edge), Failure> {
    match &graph.term(block)?.kind {
        TerminatorKind::Conditional {
            condition,
            when_true,
            when_false,
        } => Ok((*condition, when_true, when_false)),
        other => Err(failure("conditional identity actual Conditional", other)),
    }
}

fn branch<'a>(
    graph: &Graph<'a>,
    f: &Input,
    a: &Anchors,
    index: usize,
    path: (BlockId, Roles),
    prefix: &[InstructionId],
    uses: &mut Uses,
) -> Result<(&'a Edge, &'a Edge), Failure> {
    let (block, roles) = path;
    let anchor = &a.branches[index];
    let if_span = f.expression(anchor.expression)?.1;
    let condition_span = f.expression(anchor.condition)?.1;
    let literal_span = f.expression(anchor.literal)?.1;
    let (key, _, _) = roles
        .body
        .ok_or_else(|| failure("conditional identity If live body roles", roles))?;
    let equal = one(
        graph.ops(block)?.into_iter().filter(|i| {
            i.origin == Origin::Source(condition_span)
                && matches!(i.operation, Operation::StringEqual { .. })
        }),
        "conditional identity exact source StringEqual",
    )?;
    let Operation::StringEqual { left, right } = equal.operation else {
        return Err(failure("conditional identity StringEqual shape", equal));
    };
    let literal = graph.definition(right)?;
    let text = f
        .sources
        .slice(literal_span)
        .map_err(|e| failure("conditional identity literal text", e))?;
    let bytes = text
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .ok_or_else(|| failure("conditional identity plain fixture literal", text))?
        .as_bytes();
    need(
        left == EntityId::Loan(key)
            && literal.block == block
            && literal.origin == Origin::Source(literal_span)
            && matches!(&literal.operation, Operation::StringLiteral { bytes: actual, .. } if actual == bytes),
        "conditional identity current key and actual literal operands",
        (equal, literal, roles),
    )?;
    let drop = one(
        graph.ops(block)?.into_iter().filter(|i| {
            i.operation
                == Operation::Drop {
                    owner: match right {
                        EntityId::Value(v) => v,
                        _ => return false,
                    },
                }
        }),
        "conditional identity condition temporary Drop",
    )?;
    need(
        drop.origin == Origin::Source(literal_span),
        "conditional identity literal Drop origin",
        drop,
    )?;
    let mut expected = prefix.to_vec();
    expected.extend([literal.id, equal.id, drop.id]);
    need(
        graph.ops(block)?.iter().map(|i| i.id).collect::<Vec<_>>() == expected,
        "conditional identity If evaluates and cleans literal before branch",
        graph.ops(block)?,
    )?;
    let (condition, yes, no) = conditional(graph, block)?;
    need(
        condition == value(equal)? && graph.term(block)?.origin == Origin::Source(if_span),
        "conditional identity branch uses actual comparison result and If",
        graph.term(block)?,
    )?;
    uses.permit(EntityId::Loan(key), equal);
    Ok((yes, no))
}

pub(super) fn check(
    program: &Program,
    f: &Input,
    a: &Anchors,
    input: &Inputs,
    unit: bool,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "conditional identity sole module")?;
    let scan_span = one(
        a.functions.iter().filter(|v| v.name == "scan"),
        "conditional identity scan AST",
    )?
    .span;
    let scan = function(module, scan_span)?;
    let graph = Graph::new(scan)?;
    let find = |predicate: fn(&Operation) -> bool| {
        one(
            graph
                .instructions
                .iter()
                .copied()
                .filter(|i| predicate(&i.operation)),
            "conditional identity unique provider operation",
        )
    };
    let first = find(|op| matches!(op, Operation::MapIterationFirst { .. }))?;
    let has = find(|op| matches!(op, Operation::MapIterationHasNext { .. }))?;
    let key = find(|op| matches!(op, Operation::MapIterationKeyLoan { .. }))?;
    let val = find(|op| matches!(op, Operation::MapIterationValueLoan { .. }))?;
    let Operation::MapIterationFirst { source: lease } = first.operation else {
        return Err(failure("conditional identity First", first));
    };
    for operation in [first, has, key, val] {
        need(
            operation.origin == Origin::Source(a.span),
            "conditional identity provider For origin",
            operation,
        )?;
    }
    let pre = first.block;
    let header = has.block;
    need(
        scan.entry_block() == Some(pre)
            && scan.receiver.is_none()
            && graph.block(pre)?.parameters.len() == 1,
        "conditional identity actual scan entry/parameter",
        scan,
    )?;
    let seed = *graph
        .block(pre)?
        .parameters
        .get(input.parameter_slot)
        .ok_or_else(|| failure("conditional identity FE parameter slot", input))?;
    let seed_data = scan
        .entity(seed)
        .ok_or_else(|| failure("conditional identity seed entity", seed))?;
    need(
        seed_data.definition
            == Definition::BlockParameter {
                block: pre,
                index: input.parameter_slot,
            }
            && seed_data.origin == Origin::Source(scan_span)
            && input.parameter.source_id() == scan_span.source_id()
            && input.parameter.start() >= scan_span.start()
            && input.parameter.end() <= scan_span.end(),
        "conditional identity FE parameter and actual SSA definition",
        seed_data,
    )?;
    let map_type = seed_data.ty.semantic_type();
    let (kind, kt, vt) = module
        .map_container(map_type)
        .ok_or_else(|| failure("conditional identity Map type", map_type))?;
    need(
        kind == if f.map {
            MapContainerKind::Map
        } else {
            MapContainerKind::MutableMap
        } && module.type_kind(kt) == Some(&SsaTypeKind::StringOwner)
            && module.type_kind(vt) == Some(&SsaTypeKind::StringOwner),
        "conditional identity actual Map and String storage",
        (kind, kt, vt),
    )?;
    let shared = EntityType::Loan {
        kind: LoanKind::Shared,
        target: map_type,
    };
    let birth = graph.definition(EntityId::Loan(lease))?;
    need(
        birth.block == pre
            && birth.origin == Origin::Source(a.span)
            && scan.entity(EntityId::Loan(lease)).map(|e| e.ty) == Some(shared),
        "conditional identity real provider lease birth",
        birth,
    )?;
    let mut uses = Uses::default();
    uses.protect(seed);
    uses.permit(EntityId::Loan(lease), first);
    let mut pre_ops = Vec::new();
    match (f.map, seed, &birth.operation) {
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
                    && root.origin == Origin::Source(a.span)
                    && seed_data.ty == EntityType::Value(map_type)
                    && scan.entity(EntityId::Place(*place)).map(|e| e.ty)
                        == Some(EntityType::Place(map_type)),
                "conditional identity source owner roots its sole provider borrow",
                root,
            )?;
            uses.permit(seed, root);
            uses.permit(EntityId::Place(*place), birth);
            pre_ops.push(root.id);
        }
        (false, EntityId::Loan(entry), Operation::SharedReborrow { source }) => {
            need(
                *source == entry && entry != lease && seed_data.ty == shared,
                "conditional identity distinct provider reborrow from caller entry",
                birth,
            )?;
            uses.permit(seed, birth);
        }
        _ => {
            return Err(failure(
                "conditional identity source acquisition",
                (seed, birth),
            ));
        }
    }
    pre_ops.extend([birth.id, first.id]);
    need(
        graph.ops(pre)?.iter().map(|i| i.id).collect::<Vec<_>>() == pre_ops,
        "conditional identity one source acquisition before First",
        graph.ops(pre)?,
    )?;
    let pre_edge = graph.branch(pre)?;
    need(
        pre_edge.target == header && graph.term(pre)?.origin == Origin::Source(a.span),
        "conditional identity preheader to actual HasNext",
        pre_edge,
    )?;
    let carried_seed = pre_edge.arguments.contains(&seed).then_some(seed);
    need(
        !f.map || carried_seed.is_some(),
        "conditional identity owned source is carried",
        pre_edge,
    )?;
    let initial = Roles {
        source: lease,
        cursor: value(first)?,
        seed: carried_seed,
        body: None,
    };
    initial.protect(&mut uses);
    let header_roles = transport(&graph, pre_edge, initial, &mut uses)?;
    need(
        has.operation
            == Operation::MapIterationHasNext {
                source: header_roles.source,
                cursor: header_roles.cursor,
            }
            && graph.ops(header)?.iter().map(|i| i.id).collect::<Vec<_>>() == [has.id],
        "conditional identity HasNext exact seeded roles",
        has,
    )?;
    need(
        scan.entity(result(first)?).is_some_and(|e| {
            matches!(e.ty, EntityType::Value(t)
        if module.type_kind(t) == Some(&SsaTypeKind::Integer { bits: 64, signed: false }))
        }) && scan.entity(result(has)?).is_some_and(|e| {
            matches!(e.ty, EntityType::Value(t)
        if module.type_kind(t) == Some(&SsaTypeKind::Boolean))
        }),
        "conditional identity cursor/guard semantic types",
        (first, has),
    )?;
    uses.permit(EntityId::Loan(header_roles.source), has);
    uses.permit(EntityId::Value(header_roles.cursor), has);
    let (test, yes, no) = conditional(&graph, header)?;
    need(
        test == value(has)? && graph.term(header)?.origin == Origin::Source(a.span),
        "conditional identity HasNext result controls real branch",
        graph.term(header)?,
    )?;
    let body = yes.target;
    let exhausted = no.target;
    let mut body_roles = transport(&graph, yes, header_roles, &mut uses)?;
    let exhausted_roles = transport(&graph, no, header_roles, &mut uses)?;
    need(
        key.block == body
            && val.block == body
            && key.operation
                == Operation::MapIterationKeyLoan {
                    source: body_roles.source,
                    cursor: body_roles.cursor,
                }
            && val.operation
                == Operation::MapIterationValueLoan {
                    source: body_roles.source,
                    cursor: body_roles.cursor,
                }
            && loan(key)? != loan(val)?,
        "conditional identity true edge births distinct current K/V",
        (key, val),
    )?;
    for (i, ty) in [(key, kt), (val, vt)] {
        need(
            scan.entity(result(i)?).map(|e| e.ty)
                == Some(EntityType::Loan {
                    kind: LoanKind::Shared,
                    target: ty,
                }),
            "conditional identity component shared type",
            i,
        )?;
        uses.permit(EntityId::Loan(body_roles.source), i);
        uses.permit(EntityId::Value(body_roles.cursor), i);
    }
    let allocation = find(|op| matches!(op, Operation::HeapAllocate { .. }))?;
    let Operation::HeapAllocate {
        owner: guard_type,
        payload,
    } = allocation.operation
    else {
        return Err(failure("conditional identity Guard allocation", allocation));
    };
    let payload = graph.definition(EntityId::Value(payload))?;
    let guard_span = f.expression(a.guard)?.1;
    let deinit = function(module, a.deinit)?;
    need(
        allocation.block == body
            && allocation.origin == Origin::Source(guard_span)
            && scan.entity(result(allocation)?).map(|e| e.ty)
                == Some(EntityType::Value(guard_type))
            && module.deinits.get(&guard_type) == Some(&deinit.id)
            && payload.block == body
            && payload.origin == Origin::Source(guard_span)
            && matches!(&payload.operation, Operation::AggregateConstruct { aggregate, fields }
            if fields.is_empty() && matches!(module.type_kind(guard_type), Some(SsaTypeKind::HeapOwner { payload: Some(p), .. }) if p == aggregate)),
        "conditional identity actual Guard construction and deinit",
        (allocation, payload),
    )?;
    body_roles.body = Some((loan(key)?, loan(val)?, value(allocation)?));
    body_roles.protect(&mut uses);
    let mut branch_block = body;
    let mut current = body_roles;
    let mut decisions = Vec::new();
    let mut paths = Vec::new();
    for index in 0..3 {
        decisions.push(branch_block);
        let prefix = if index == 0 {
            vec![key.id, val.id, payload.id, allocation.id]
        } else {
            vec![]
        };
        let (yes, no) = branch(
            &graph,
            f,
            a,
            index,
            (branch_block, current),
            &prefix,
            &mut uses,
        )?;
        let positive = transport(&graph, yes, current, &mut uses)?;
        let negative = transport(&graph, no, current, &mut uses)?;
        graph.incoming_from(yes.target, &[branch_block])?;
        graph.incoming_from(no.target, &[branch_block])?;
        paths.push((yes.target, positive));
        branch_block = no.target;
        current = negative;
    }
    paths.push((branch_block, current));
    let continued = paths[0].0;
    let broken = paths[1].0;
    let returned = paths[2].0;
    let tail = paths[3].0;
    let after = graph.branch(exhausted)?.target;
    need(
        graph.branch(broken)?.target == after,
        "conditional identity Break/Exhaustion common after-loop",
        after,
    )?;
    let roles = [
        pre,
        header,
        body,
        exhausted,
        continued,
        decisions[1],
        broken,
        decisions[2],
        returned,
        tail,
        after,
    ];
    let expected_blocks = roles.into_iter().collect::<BTreeSet<_>>();
    let actual_blocks = graph.blocks.iter().copied().collect::<BTreeSet<_>>();
    let arena_blocks = scan.blocks.iter().map(|b| b.id).collect::<BTreeSet<_>>();
    let actual_instructions = graph
        .instructions
        .iter()
        .map(|i| i.id)
        .collect::<BTreeSet<_>>();
    let arena_instructions = scan
        .instructions
        .iter()
        .map(|i| i.id)
        .collect::<BTreeSet<_>>();
    need(
        expected_blocks.len() == roles.len()
            && actual_blocks == expected_blocks
            && arena_blocks == expected_blocks
            && scan.blocks.len() == arena_blocks.len()
            && actual_instructions == arena_instructions
            && scan.instructions.len() == arena_instructions.len(),
        "conditional identity full actual block/instruction identity sets",
        (&roles, &graph.blocks),
    )?;
    graph.incoming_from(pre, &[])?;
    graph.incoming_from(header, &[pre, continued, tail])?;
    graph.incoming_from(body, &[header])?;
    graph.incoming_from(exhausted, &[header])?;
    graph.incoming_from(after, &[broken, exhausted])?;
    let cleanup = query_cleanup::Context {
        graph: &graph,
        module,
        f,
        a,
        input,
        unit,
    };
    let mut nexts = Vec::new();
    let mut returned_value = None;
    for (index, &(block, roles)) in paths.iter().enumerate() {
        let output = cleanup.body(index, block, roles, &mut uses)?;
        if let Some(next) = output.next {
            let back = graph.branch(block)?;
            let span = if index == 0 {
                f.expression(a.branches[0].transfer)?.1
            } else {
                a.span
            };
            need(
                back.target == header && graph.term(block)?.origin == Origin::Source(span),
                "conditional identity each Next targets actual header",
                back,
            )?;
            let incoming = transport(
                &graph,
                back,
                Roles {
                    cursor: next,
                    body: None,
                    ..roles
                },
                &mut uses,
            )?;
            need(
                incoming.source == header_roles.source
                    && incoming.cursor == header_roles.cursor
                    && incoming.seed == header_roles.seed,
                "conditional identity each header incoming agrees with grounded columns",
                (incoming, header_roles),
            )?;
            nexts.push(graph.definition(EntityId::Value(next))?.id);
        }
        if let Some(value) = output.returned {
            need(
                index == 2 && block == returned,
                "conditional identity Return belongs to third true edge",
                block,
            )?;
            need(
                returned_value.replace(value).is_none(),
                "conditional identity one operand Return",
                value,
            )?;
        }
    }
    cleanup.exhausted(exhausted, exhausted_roles, &mut uses)?;
    let from_break = graph.branch(broken)?;
    let from_empty = graph.branch(exhausted)?;
    for (block, edge, expected) in [
        (broken, from_break, paths[1].1.seed),
        (exhausted, from_empty, exhausted_roles.seed),
    ] {
        let expected = if f.map { None } else { expected };
        need(
            edge.arguments == expected.into_iter().collect::<Vec<_>>()
                && graph.term(block)?.origin == Origin::Source(a.span),
            "conditional identity only retained caller Borrow enters after-loop",
            edge,
        )?;
    }
    if let (Some(b), Some(e)) = (
        paths[1].1.seed.filter(|_| !f.map),
        exhausted_roles.seed.filter(|_| !f.map),
    ) {
        let b = graph.transported(from_break, b)?;
        need(
            b == graph.transported(from_empty, e)?
                && graph.block(after)?.parameters.as_slice() == [b],
            "conditional identity both after-loop incoming retain exact caller alias",
            after,
        )?;
        uses.protect(b);
    } else {
        need(
            graph.block(after)?.parameters.is_empty(),
            "conditional identity ended provider has no after-loop parameters",
            after,
        )?;
    }
    cleanup.fallback(after)?;
    let expected_provider = [first.id, has.id, key.id, val.id]
        .into_iter()
        .chain(nexts)
        .collect::<BTreeSet<_>>();
    let actual_provider = graph
        .instructions
        .iter()
        .filter(|i| {
            matches!(
                i.operation,
                Operation::MapIterationFirst { .. }
                    | Operation::MapIterationHasNext { .. }
                    | Operation::MapIterationKeyLoan { .. }
                    | Operation::MapIterationValueLoan { .. }
                    | Operation::MapIterationNext { .. }
            )
        })
        .map(|i| i.id)
        .collect::<BTreeSet<_>>();
    need(
        expected_provider.len() == 6
            && expected_provider == actual_provider
            && graph.edges.len() == 13,
        "conditional identity exact provider operations and closed edge set",
        (expected_provider, actual_provider, &graph.edges),
    )?;
    let return_value = returned_value
        .ok_or_else(|| failure("conditional identity operand Return missing", returned))?;
    uses.check(&graph, Some((returned, return_value)))?;
    println!(
        "SPEC0331 CONDITIONAL IDENTITY SSA scan={:?} blocks={roles:?} paths={paths:?} edges={:#?}",
        scan.id, graph.edges
    );
    Ok(())
}
