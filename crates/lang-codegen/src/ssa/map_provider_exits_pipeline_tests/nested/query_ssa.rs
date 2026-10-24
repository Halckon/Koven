//! Independent identity proof for the actual nested-normal pair and all its incoming edges.
use super::super::{
    fixture::{Failure, contains, failure, need},
    query_graph::{Graph, function},
    query_inputs::one,
};
use super::{
    ast::Input,
    query_body::{Context, INNER, OUTER, provider},
    query_callers,
    query_graph::{Role, Roles, Witness, full_graph, ty},
    query_inputs::Inputs,
    query_tail,
};
use crate::ssa::model::*;
use std::collections::BTreeSet;

pub(super) fn check(
    program: &Program,
    entry_id: FunctionId,
    f: &Input,
    inputs: &Inputs,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "nested SSA sole actual module")?;
    let entry = function(module, inputs.entry)?;
    let scan = function(module, inputs.scan)?;
    let read = function(module, inputs.read)?;
    need(
        entry.id == entry_id && module.functions.len() == 4,
        "nested SSA returned entry and exact real source functions",
        (entry_id, &module.functions),
    )?;
    let g = Graph::new(scan)?;
    full_graph(&g)?;
    let pre = scan
        .entry_block()
        .ok_or_else(|| failure("nested SSA actual scan entry", scan.id))?;
    let caller = *one(
        g.block(pre)?.parameters.iter(),
        "nested SSA sole actual Borrow caller parameter",
    )?;
    let EntityType::Loan {
        kind: LoanKind::Shared,
        target: map_type,
    } = ty(&g, caller)?
    else {
        return Err(failure("nested SSA actual Shared map entry", caller));
    };
    let (kind, key, val) = module
        .map_container(map_type)
        .ok_or_else(|| failure("nested SSA real map type", map_type))?;
    need(
        kind == if f.map {
            MapContainerKind::Map
        } else {
            MapContainerKind::MutableMap
        } && module.type_kind(key) == Some(&SsaTypeKind::StringOwner)
            && module.type_kind(val) == Some(&SsaTypeKind::StringOwner)
            && scan.return_types.is_empty()
            && scan.receiver.is_none()
            && scan.borrow_return.is_none()
            && scan.carrier_return.is_none()
            && contains(inputs.scan, inputs.parameter),
        "nested SSA actual FE Borrow signature and exact intrinsic storage",
        scan,
    )?;
    query_callers::check(module, map_type, f, inputs)?;
    let c = Context {
        graph: &g,
        module,
        read,
    };
    let mut witness = Witness::default();
    let mut initial = Roles::default();
    initial.insert(Role::Caller, caller);
    c.birth(pre, &inputs.sites[0], OUTER, &mut initial, &mut witness)?;
    need(
        g.ops(pre)?.len() == 2,
        "nested SSA actual outer acquire entry",
        g.ops(pre)?,
    )?;
    let outer_entry = g.branch(pre)?;
    let oh = outer_entry.target;
    let outer_header = witness.edge(&g, pre, outer_entry, &initial)?;
    let (ob, mut outer_body, oe, outer_exit) =
        c.header(oh, &inputs.sites[0], OUTER, &outer_header, &mut witness)?;
    c.project(ob, &inputs.sites[0], OUTER, &mut outer_body, &mut witness)?;
    let before = c.read(ob, &inputs.reads[0], OUTER, &outer_body, &mut witness)?;
    c.birth(ob, &inputs.sites[1], INNER, &mut outer_body, &mut witness)?;
    let inner_birth = g.definition(outer_body.get(Role::InnerSource)?)?;
    need(
        g.position(before)? < g.position(inner_birth)?,
        "nested SSA outer read precedes fresh inner acquire",
        (before, inner_birth),
    )?;
    let inner_entry = g.branch(ob)?;
    let ih = inner_entry.target;
    let inner_header = witness.edge(&g, ob, inner_entry, &outer_body)?;
    let (ib, mut inner_body, ie, inner_exit) =
        c.header(ih, &inputs.sites[1], INNER, &inner_header, &mut witness)?;
    need(
        [pre, oh, ob, oe, ih, ib, ie]
            .into_iter()
            .collect::<BTreeSet<_>>()
            .len()
            == 7,
        "nested SSA seven distinct core roles including actual inner false resume",
        (pre, oh, ob, oe, ih, ib, ie),
    )?;
    g.incoming_from(pre, &[])?;
    g.incoming_from(oh, &[pre, ie])?;
    g.incoming_from(ih, &[ob, ib])?;
    for (block, from) in [(ob, oh), (oe, oh), (ib, ih), (ie, ih)] {
        g.incoming_from(block, &[from])?;
    }
    c.project(ib, &inputs.sites[1], INNER, &mut inner_body, &mut witness)?;
    let outer_in = c.read(ib, &inputs.reads[1], OUTER, &inner_body, &mut witness)?;
    let inner_read = c.read(ib, &inputs.reads[2], INNER, &inner_body, &mut witness)?;
    need(
        g.position(outer_in)? < g.position(inner_read)?,
        "nested SSA actual two inner-body consumers ordered",
        (outer_in, inner_read),
    )?;
    let inner_back_roles = c.cleanup_next(
        ib,
        &inputs.sites[1],
        INNER,
        &inner_body,
        inner_read,
        &mut witness,
    )?;
    let inner_back = g.branch(ib)?;
    need(
        inner_back.target == ih,
        "nested SSA inner Next returns to its own header",
        inner_back,
    )?;
    need(
        witness.edge(&g, ib, inner_back, &inner_back_roles)? == inner_header,
        "nested SSA every inner header incoming matches all seven precise roles",
        inner_back,
    )?;
    let inner_end = c.end_source(ie, &inputs.sites[1], INNER, &inner_exit, &mut witness)?;
    let resumed = inner_exit.retain(&[
        Role::Caller,
        Role::OuterSource,
        Role::OuterCursor,
        Role::OuterKey,
        Role::OuterValue,
    ])?;
    let after_read = c.read(ie, &inputs.reads[3], OUTER, &resumed, &mut witness)?;
    need(
        g.position(inner_end)? < g.position(after_read)?,
        "nested SSA inner-only EndSource before actual resumed outer read",
        (inner_end, after_read),
    )?;
    let outer_back_roles = c.cleanup_next(
        ie,
        &inputs.sites[0],
        OUTER,
        &resumed,
        after_read,
        &mut witness,
    )?;
    let outer_back = g.branch(ie)?;
    need(
        outer_back.target == oh,
        "nested SSA outer Next returns to its own header",
        outer_back,
    )?;
    need(
        witness.edge(&g, ie, outer_back, &outer_back_roles)? == outer_header,
        "nested SSA every outer header incoming matches caller/source/new cursor",
        outer_back,
    )?;
    c.end_source(oe, &inputs.sites[0], OUTER, &outer_exit, &mut witness)?;
    let mut tail_roles = outer_exit.retain(&[Role::Caller])?;
    let mut tail = oe;
    for &span in &inputs.sizes {
        let size = query_tail::size(&g, span, tail_roles.get(Role::Caller)?, &mut witness)?;
        need(
            size.block == tail,
            "nested SSA post-outer size uses actual current continuation",
            size,
        )?;
        (tail, tail_roles) = query_tail::conditional(&g, f, size, &tail_roles, &mut witness)?;
    }
    need(
        g.ops(tail)?.is_empty()
            && g.term(tail)?.origin == Origin::Source(inputs.scan)
            && matches!(&g.term(tail)?.kind, TerminatorKind::Return { values } if values.is_empty()),
        "nested SSA after both sizes normal Unit return preserves caller",
        g.term(tail)?,
    )?;
    need(
        g.instructions
            .iter()
            .filter(|i| provider(i))
            .map(|i| i.id)
            .collect::<BTreeSet<_>>()
            == witness.providers
            && witness.providers.len() == 10,
        "nested SSA all provider operations belong exactly to the two proven sites",
        &witness.providers,
    )?;
    let actual_calls = g
        .instructions
        .iter()
        .filter(|i| matches!(i.operation, Operation::DirectCall { .. }))
        .map(|i| i.id)
        .collect::<BTreeSet<_>>();
    need(
        actual_calls
            == [before.id, outer_in.id, inner_read.id, after_read.id]
                .into_iter()
                .collect()
            && g.instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapSize { .. }))
                .count()
                == inputs.sizes.len(),
        "nested SSA complete actual read and post-loop size sets",
        actual_calls,
    )?;
    // The exact edge set has no OE-to-acquire path and no inner role on outer backedge.
    // Per-role aliases and Uses prove caller/OS stay live through the inner-only prefix.
    let related = [Role::Caller, Role::OuterSource, Role::InnerSource]
        .into_iter()
        .flat_map(|role| witness.aliases.get(&role).into_iter().flatten().copied())
        .collect::<BTreeSet<_>>();
    super::super::query_inventory::check(&g, &[map_type], &related)?;
    witness.finish(&g)
}
