//! Independent all-edge/all-use identity query for the two actual nested providers.
use super::super::{
    fixture::{Failure, contains, failure, need},
    query_graph::{Graph, function, result},
    query_inputs::one,
};
use super::{
    ast::{Exit, Input},
    query_body::{Context, INNER, OUTER, provider},
    query_callers,
    query_graph::{Role, Roles, Witness, full_graph, ty},
    query_inputs::Inputs,
    query_resources::CleanupSite,
    query_tail,
};
use crate::ssa::model::*;
use lang_frontend::parser::Expression;
use std::collections::BTreeSet;

pub(super) fn check(
    program: &Program,
    entry_id: FunctionId,
    f: &Input,
    input: &Inputs,
) -> Result<(), Failure> {
    let module = one(
        program.modules.iter(),
        "nested exits SSA sole actual module",
    )?;
    let entry = function(module, input.entry)?;
    let scan = function(module, input.scan)?;
    let read = function(module, input.read)?;
    need(
        entry.id == entry_id
            && module.functions.len() == if f.exit == Exit::Return { 7 } else { 6 },
        "nested exits SSA returned entry and complete actual function set",
        (entry_id, &module.functions),
    )?;
    let g = Graph::new(scan)?;
    full_graph(&g)?;
    let pre = scan
        .entry_block()
        .ok_or_else(|| failure("nested exits SSA scan entry", scan.id))?;
    let caller = *one(
        g.block(pre)?.parameters.iter(),
        "nested exits SSA sole Borrow caller parameter",
    )?;
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target: map_type,
    } = ty(&g, caller)?
    else {
        return Err(failure("nested exits SSA actual Shared map caller", caller));
    };
    let (kind, key, val) = module
        .map_container(map_type)
        .ok_or_else(|| failure("nested exits SSA intrinsic map type", map_type))?;
    need(
        kind == if f.map {
            MapContainerKind::Map
        } else {
            MapContainerKind::MutableMap
        } && module.type_kind(key) == Some(&SsaTypeKind::StringOwner)
            && module.type_kind(val) == Some(&SsaTypeKind::StringOwner)
            && contains(input.scan, input.parameter),
        "nested exits SSA actual Borrow map and String components",
        scan,
    )?;
    query_callers::check(module, map_type, f, input)?;
    let c = Context {
        graph: &g,
        module,
        read,
    };
    let mut w = Witness::default();
    let mut initial = Roles::default();
    initial.insert(Role::Caller, caller);
    c.birth(pre, &input.sites[0], OUTER, &mut initial, &mut w)?;
    need(
        g.ops(pre)?.len() == 2,
        "nested exits SSA exact outer acquisition prefix",
        g.ops(pre)?,
    )?;
    let outer_entry = g.branch(pre)?;
    let oh = outer_entry.target;
    let outer_header = w.edge(&g, pre, outer_entry, &initial)?;
    let (ob, mut outer_body, oe, outer_exit) =
        c.header(oh, &input.sites[0], OUTER, &outer_header, &mut w)?;
    c.project(ob, &input.sites[0], OUTER, &mut outer_body, &mut w)?;
    let go = c.guard(
        ob,
        &input.guards[0],
        Role::OuterGuard,
        &mut outer_body,
        &mut w,
    )?;
    let before = c.read(ob, &input.reads[0], OUTER, &outer_body, &mut w)?;
    c.birth(ob, &input.sites[1], INNER, &mut outer_body, &mut w)?;
    let inner_birth = g.definition(outer_body.get(Role::InnerSource)?)?;
    need(
        g.position(before)? < g.position(inner_birth)?,
        "nested exits SSA read before independent inner caller reborrow",
        (before, inner_birth),
    )?;
    let inner_entry = g.branch(ob)?;
    let ih = inner_entry.target;
    let inner_header = w.edge(&g, ob, inner_entry, &outer_body)?;
    let (ib, mut inner_body, ie, inner_exit) =
        c.header(ih, &input.sites[1], INNER, &inner_header, &mut w)?;
    need(
        [pre, oh, ob, oe, ih, ib, ie]
            .into_iter()
            .collect::<BTreeSet<_>>()
            .len()
            == 7,
        "nested exits SSA seven distinct actual core blocks",
        (pre, oh, ob, oe, ih, ib, ie),
    )?;
    g.incoming_from(pre, &[])?;
    g.incoming_from(
        ih,
        if f.exit == Exit::Continue {
            &[ob, ib]
        } else {
            &[ob]
        },
    )?;
    for (block, from) in [(ob, oh), (oe, oh), (ib, ih), (ie, ih)] {
        g.incoming_from(block, &[from])?;
    }
    c.project(ib, &input.sites[1], INNER, &mut inner_body, &mut w)?;
    let gi = c.guard(
        ib,
        &input.guards[1],
        Role::InnerGuard,
        &mut inner_body,
        &mut w,
    )?;
    let mut calls = [before.id].into_iter().collect::<BTreeSet<_>>();
    let inner_end = c.end_source(ie, &input.sites[1], INNER, &inner_exit, &mut w)?;
    let resumed_roles = [
        Role::Caller,
        Role::OuterGuard,
        Role::OuterSource,
        Role::OuterCursor,
        Role::OuterKey,
        Role::OuterValue,
    ];
    let mut resumed = inner_exit.retain(&resumed_roles)?;
    let mut resume = ie;
    match f.exit {
        Exit::Continue | Exit::Break => {
            let outer_read = c.read(ib, &input.reads[1], OUTER, &inner_body, &mut w)?;
            let inner_read = c.read(ib, &input.reads[2], INNER, &inner_body, &mut w)?;
            calls.extend([outer_read.id, inner_read.id]);
            need(
                g.position(outer_read)? < g.position(inner_read)?,
                "nested exits SSA ordered outer/inner current reads",
                (outer_read, inner_read),
            )?;
            let (after, sequence) = c.cleanup(
                ib,
                CleanupSite {
                    layer: INNER,
                    guard: Role::InnerGuard,
                    guard_input: &input.guards[1],
                    origin: if input.unit {
                        input.sites[1].span
                    } else {
                        input.transfer
                    },
                    advance: (f.exit == Exit::Continue).then_some(input.transfer),
                },
                &inner_body,
                &mut w,
            )?;
            let first = g
                .function
                .instruction(sequence[0])
                .ok_or_else(|| failure("nested exits SSA GI Drop", &sequence))?;
            need(
                g.position(inner_read)? < g.position(first)?,
                "nested exits SSA all reads precede actual GI cleanup",
                (inner_read, first),
            )?;
            c.suffix(ib, &sequence)?;
            let transfer = g.branch(ib)?;
            if f.exit == Exit::Continue {
                need(
                    transfer.target == ih
                        && g.term(ib)?.origin == Origin::Source(input.transfer)
                        && w.edge(&g, ib, transfer, &after)? == inner_header,
                    "nested exits SSA Continue all current GO/caller/outer/inner roles close own Header",
                    transfer,
                )?;
            } else {
                let false_edge = g.branch(ie)?;
                need(
                    g.ops(ie)?.iter().map(|i| i.id).collect::<Vec<_>>() == [inner_end.id]
                        && transfer.target == false_edge.target
                        && g.term(ib)?.origin == Origin::Source(input.sites[1].span)
                        && g.term(ie)?.origin == Origin::Source(input.sites[1].span)
                        && ![pre, oh, ob, oe, ih, ib, ie].contains(&transfer.target),
                    "nested exits SSA Break and false exact independent cleanup then true resume join",
                    (transfer, false_edge),
                )?;
                resume = transfer.target;
                g.incoming_from(resume, &[ib, ie])?;
                let break_roles = w.edge(&g, ib, transfer, &after)?;
                resumed = w.edge(&g, ie, false_edge, &resumed)?;
                need(
                    break_roles == resumed
                        && g.block(resume)?.origin == Origin::Source(input.sites[1].span),
                    "nested exits SSA every Break/false incoming binds same current GO and outer roles",
                    (&break_roles, &resumed),
                )?;
            }
        }
        Exit::Return => {
            let (operand, _) = c.operand(ib, input, &inner_body, &mut w)?;
            calls.insert(operand.id);
            let (after_inner, mut sequence) = c.cleanup(
                ib,
                CleanupSite {
                    layer: INNER,
                    guard: Role::InnerGuard,
                    guard_input: &input.guards[1],
                    origin: if input.unit {
                        input.sites[1].span
                    } else {
                        input.transfer
                    },
                    advance: None,
                },
                &inner_body,
                &mut w,
            )?;
            let (after_outer, outer_sequence) = c.cleanup(
                ib,
                CleanupSite {
                    layer: OUTER,
                    guard: Role::OuterGuard,
                    guard_input: &input.guards[0],
                    origin: if input.unit {
                        input.sites[0].span
                    } else {
                        input.transfer
                    },
                    advance: None,
                },
                &after_inner,
                &mut w,
            )?;
            sequence.extend(outer_sequence);
            need(
                sequence.len() == 8 && after_outer == inner_body.retain(&[Role::Caller])?,
                "nested exits SSA ten FE actions emit complete eight-op inner-then-outer cleanup",
                (&sequence, &after_outer),
            )?;
            let first = g
                .function
                .instruction(sequence[0])
                .ok_or_else(|| failure("nested exits SSA Return cleanup head", &sequence))?;
            need(
                g.position(operand)? < g.position(first)?
                    && g.ops(ib)?[g.position(operand)? + 1..g.position(first)?]
                        .iter()
                        .all(|i| matches!(i.operation, Operation::Copy { .. }))
                    && !g.edges.iter().any(|(from, _)| *from == ib),
                "nested exits SSA evaluated operand precedes whole cleanup and actual Return has no outgoing edge",
                (operand, first),
            )?;
            c.suffix(ib, &sequence)?;
        }
    }
    let last = if f.exit == Exit::Return {
        inner_end
    } else {
        let read = c.read(resume, &input.reads[3], OUTER, &resumed, &mut w)?;
        calls.insert(read.id);
        need(
            resume != ie || g.position(inner_end)? < g.position(read)?,
            "nested exits SSA resumed outer read follows only inner cleanup",
            read,
        )?;
        read
    };
    let (outer_after, outer_sequence) = c.cleanup(
        resume,
        CleanupSite {
            layer: OUTER,
            guard: Role::OuterGuard,
            guard_input: &input.guards[0],
            origin: input.sites[0].span,
            advance: Some(input.sites[0].span),
        },
        &resumed,
        &mut w,
    )?;
    let first_outer = g
        .function
        .instruction(outer_sequence[0])
        .ok_or_else(|| failure("nested exits SSA outer Drop head", &outer_sequence))?;
    need(
        g.position(last)? < g.position(first_outer)?,
        "nested exits SSA resumed work precedes current GO outer cleanup",
        (last, first_outer),
    )?;
    c.suffix(resume, &outer_sequence)?;
    if f.exit == Exit::Return {
        need(
            g.ops(ie)?.iter().map(|i| i.id).collect::<Vec<_>>()
                == std::iter::once(inner_end.id)
                    .chain(outer_sequence.iter().copied())
                    .collect::<Vec<_>>(),
            "nested exits SSA Return false sibling complete inner-prefix and outer-suffix",
            g.ops(ie)?,
        )?;
    }
    let outer_back = g.branch(resume)?;
    need(
        outer_back.target == oh
            && g.term(resume)?.origin == Origin::Source(input.sites[0].span)
            && w.edge(&g, resume, outer_back, &outer_after)? == outer_header,
        "nested exits SSA outer continuation all roles return to actual own Header",
        outer_back,
    )?;
    g.incoming_from(oh, &[pre, resume])?;
    c.end_source(oe, &input.sites[0], OUTER, &outer_exit, &mut w)?;
    let mut tail_roles = outer_exit.retain(&[Role::Caller])?;
    let mut tail = oe;
    for &span in &input.sizes {
        let size = query_tail::size(&g, span, tail_roles.get(Role::Caller)?, &mut w)?;
        need(
            size.block == tail,
            "nested exits SSA post-outer size current caller",
            size,
        )?;
        (tail, tail_roles) = query_tail::conditional(&g, f, size, None, &tail_roles, &mut w)?;
    }
    if f.exit == Exit::Return {
        need(
            input.sizes.is_empty() && tail == oe,
            "nested exits SSA Return fallback no fake size path",
            input,
        )?;
        fallback(&g, f, tail, &mut w)?;
    } else {
        need(
            g.ops(tail)?.is_empty()
                && g.term(tail)?.origin == Origin::Source(input.scan)
                && matches!(&g.term(tail)?.kind, TerminatorKind::Return { values } if values.is_empty()),
            "nested exits SSA final Unit Return preserves caller",
            g.term(tail)?,
        )?;
    }
    need(
        g.instructions
            .iter()
            .filter(|i| provider(i))
            .map(|i| i.id)
            .collect::<BTreeSet<_>>()
            == w.providers
            && w.providers.len() == if f.exit == Exit::Continue { 10 } else { 9 }
            && g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
                .map(|i| i.id)
                .collect::<BTreeSet<_>>()
                == calls
            && g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapSize { .. }))
                .count()
                == input.sizes.len()
            && g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::HeapAllocate { .. }))
                .map(|i| i.id)
                .collect::<BTreeSet<_>>()
                == [go, gi].into_iter().collect(),
        "nested exits SSA exact complete provider/call/size/Guard-allocation sets",
        (&w.providers, &calls),
    )?;
    w.finish(&g)
}

fn fallback(g: &Graph<'_>, f: &Input, block: BlockId, w: &mut Witness) -> Result<(), Failure> {
    let term = g.term(block)?;
    let TerminatorKind::Return { values } = &term.kind else {
        return Err(failure(
            "nested exits SSA actual zero fallback Return",
            term,
        ));
    };
    let zero = one(values.iter().copied(), "nested exits SSA one fallback Int")?;
    let producer = g.definition(EntityId::Value(zero))?;
    let ast_return = one(
        f.parsed
            .ast()
            .expressions()
            .iter()
            .filter_map(|(_, node)| (node.span() == term.origin.span()).then_some(node)),
        "nested exits SSA actual fallback Return expression",
    )?;
    let Expression::Return {
        value: Some(value), ..
    } = ast_return.payload()
    else {
        return Err(failure("nested exits SSA fallback AST Return", ast_return));
    };
    let (literal, span) = f.expression(*value)?;
    need(
        producer.operation == Operation::Constant(ScalarConstant::Integer(0))
            && producer.block == block
            && producer.origin == Origin::Source(span)
            && matches!(literal, Expression::Literal(_))
            && f.sources.slice(span) == Ok("0")
            && g.function.return_types.as_slice() == [ty(g, result(producer)?)?.semantic_type()],
        "nested exits SSA actual fallback zero separate from operand result",
        producer,
    )?;
    w.returns.insert(block, vec![zero]);
    Ok(())
}
