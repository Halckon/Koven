//! Read-only proof for the empty-Map factory's ordinary argument CFG and hidden owner.
pub(super) mod argument;
mod ast;
mod provider;
mod wrapper;
use super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, loan, result, value},
    query_inputs::one,
};
use crate::ssa::model::*;
use lang_frontend::{
    parser::{Expression, ParsedFile},
    source::{SourceMap, Span},
};
use std::collections::BTreeSet;

fn at<'p>(
    g: &Graph<'p>,
    span: Span,
    predicate: impl Fn(&Operation) -> bool,
) -> Result<&'p Instruction, Failure> {
    one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.origin == Origin::Source(span) && predicate(&i.operation)),
        "argument CFG original operation",
    )
}
fn family(g: &Graph<'_>, anchor: EntityId) -> Result<BTreeSet<EntityId>, Failure> {
    let mut entities = BTreeSet::from([anchor]);
    loop {
        let before = entities.len();
        for (_, edge) in &g.edges {
            for (&arg, &p) in edge.arguments.iter().zip(&g.block(edge.target)?.parameters) {
                if entities.contains(&arg) {
                    entities.insert(p);
                }
            }
        }
        if before == entities.len() {
            break;
        }
    }
    for &e in &entities {
        g.anchored(e, &[anchor])?;
    }
    Ok(entities)
}

pub(super) fn check(
    sources: &SourceMap,
    parsed: &ParsedFile,
    program: &Program,
) -> Result<(), Failure> {
    let input = ast::input(sources, parsed)?;
    let module = one(program.modules.iter(), "argument CFG actual module")?;
    let scan = function(module, input.scan)?;
    let factory = function(module, input.factory)?;
    wrapper::check(sources, parsed, module, scan)?;
    let g = Graph::with_abort(scan)?;
    let mut uses = Uses::default();
    let argument_input = argument::Input {
        call: input.call,
        argument: input.argument,
        flag: input.flag,
        arms: input.arms,
    };
    let (call, argument_end) = argument::check(
        sources,
        parsed,
        &argument_input,
        module,
        &g,
        factory,
        &mut uses,
    )?;
    let owner = value(call)?;
    let map = scan
        .entity(EntityId::Value(owner))
        .unwrap()
        .ty
        .semantic_type();
    let (kind, key, val) = module
        .map_container(map)
        .ok_or_else(|| failure("argument CFG actual Map result", call))?;
    need(
        kind == if input.mutable {
            MapContainerKind::MutableMap
        } else {
            MapContainerKind::Map
        } && [key, val]
            .iter()
            .all(|t| module.type_kind(*t) == Some(&SsaTypeKind::StringOwner))
            && factory.return_types.as_slice() == [map],
        "selected factory returns exact empty Map String storage",
        call,
    )?;
    empty_factory(&input, module, factory, map)?;
    let owners = family(&g, EntityId::Value(owner))?;
    for &e in &owners {
        uses.protect(e);
    }
    for (index, data) in scan.values.iter().enumerate() {
        if module.map_container(data.ty.semantic_type()).is_some() {
            let e = EntityId::Value(ValueId {
                function: scan.id,
                index,
            });
            need(
                data.ty == EntityType::Value(map) && owners.contains(&e),
                "all Map Value nodes belong to sole actual Call lineage",
                (e, data),
            )?;
            g.anchored(e, &[EntityId::Value(owner)])?;
        }
    }
    let drops = g
        .instructions
        .iter()
        .copied()
        .filter(|i| {
            let Operation::Drop { owner } = i.operation else {
                return false;
            };
            let ty = scan
                .entity(EntityId::Value(owner))
                .unwrap()
                .ty
                .semantic_type();
            module.map_container(ty).is_some()
        })
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        return Err(failure("argument CFG unique Map Drop", &drops));
    };
    let source_end = if let Some(site) = &input.loop_site {
        Some(provider::check(
            sources,
            parsed,
            &input,
            site,
            module,
            &g,
            call,
            argument_end,
            drop,
            &owners,
            &mut uses,
        )?)
    } else {
        let size = at(&g, input.size.unwrap(), |o| {
            matches!(o, Operation::MapSize { .. })
        })?;
        need(
            size.operation
                == Operation::MapSize {
                    owner: EntityId::Value(owner),
                }
                && size.block == call.block
                && argument_end.block == size.block
                && g.position(argument_end)? < g.position(size)?
                && drop.operation == Operation::Drop { owner }
                && drop.origin == Origin::Source(input.drop_origin)
                && drop.block == size.block
                && g.position(drop)? == g.position(size)? + 1,
            "ordinary control argument End/actual owner size/ASAP Drop",
            (argument_end, size, drop),
        )?;
        need(
            !g.instructions.iter().any(|i| iteration(&i.operation)),
            "noFor ordinary control contains no iteration operations",
            scan.id,
        )?;
        uses.permit(EntityId::Value(owner), size);
        uses.permit(EntityId::Value(owner), drop);
        None
    };
    uses.check(&g, None)?;
    paths(&g, call.id, drop.id, source_end.map(|i| i.id))
}
fn iteration(o: &Operation) -> bool {
    matches!(
        o,
        Operation::MapIterationFirst { .. }
            | Operation::MapIterationHasNext { .. }
            | Operation::MapIterationNext { .. }
            | Operation::MapIterationKeyLoan { .. }
            | Operation::MapIterationValueLoan { .. }
    )
}
fn empty_factory(
    input: &ast::Input,
    module: &Module,
    f: &Function,
    map: SsaTypeId,
) -> Result<(), Failure> {
    let fg = Graph::with_abort(f)?;
    let construct = at(&fg, input.construct, |o| {
        matches!(o, Operation::MapConstruct { .. })
    })?;
    need(
        construct.operation == Operation::MapConstruct { map_type: map }
            && fg
                .instructions
                .iter()
                .filter(|i| matches!(i.operation, Operation::MapConstruct { .. }))
                .count()
                == 1
            && !fg.instructions.iter().any(|i| {
                matches!(
                    i.operation,
                    Operation::MapPut { .. }
                        | Operation::MapConsume { .. }
                        | Operation::DirectCall { .. }
                        | Operation::HeapAllocate { .. }
                )
            }),
        "factory creates only actual original zero-entry Map",
        construct,
    )?;
    let returns = fg
        .blocks
        .iter()
        .map(|&block| fg.term(block))
        .collect::<Result<Vec<_>, _>>()?;
    let returned = one(
        returns
            .iter()
            .copied()
            .filter(|t| matches!(t.kind, TerminatorKind::Return { .. })),
        "factory unique normal Return",
    )?;
    need(
        matches!(&returned.kind, TerminatorKind::Return { values }
            if values.as_slice() == [value(construct)?])
            && returned.origin == Origin::Source(input.factory_return)
            && returned == fg.term(construct.block)?,
        "factory sole normal Return delivers original empty construct",
        returned,
    )?;
    let map_values = f
        .values
        .iter()
        .filter(|d| module.map_container(d.ty.semantic_type()).is_some())
        .count();
    need(
        map_values == 1,
        "empty factory has no additional owner transport/producer",
        f.id,
    )
}
// Cycles are folded only after their complete live cleanup state has been visited.
fn paths(
    g: &Graph<'_>,
    call: InstructionId,
    drop: InstructionId,
    end: Option<InstructionId>,
) -> Result<(), Failure> {
    let mut pending = vec![(g.function.entry_block().unwrap(), 0u8, 0u8, 0u8)];
    let mut seen = BTreeSet::new();
    let mut returned = 0;
    while let Some((block, mut made, mut ended, mut dropped)) = pending.pop() {
        if !seen.insert((block, made, ended, dropped)) {
            continue;
        }
        for i in g.ops(block)? {
            if i.id == call {
                made += 1;
                need(
                    made == 1 && dropped == 0,
                    "argument CFG selected factory called once",
                    i,
                )?;
            }
            if Some(i.id) == end {
                ended += 1;
                need(
                    ended == 1 && made == 1 && dropped == 0,
                    "source ends once while hidden owner live",
                    i,
                )?;
            }
            if i.id == drop {
                dropped += 1;
                need(
                    dropped == 1 && made == 1 && ended == u8::from(end.is_some()),
                    "unique Drop follows actual source End",
                    i,
                )?;
            }
            if iteration(&i.operation) {
                need(
                    made == 1 && dropped == 0,
                    "provider operations only while original owner live",
                    i,
                )?;
            }
        }
        match &g.term(block)?.kind {
            TerminatorKind::Return { values } => {
                returned += 1;
                need(
                    values.is_empty()
                        && made == 1
                        && dropped == 1
                        && ended == u8::from(end.is_some()),
                    "every normal Return has one actual hidden-owner cleanup",
                    g.term(block)?,
                )?;
            }
            TerminatorKind::Abort => need(
                made == 1 && dropped == 1,
                "noFor failure branch starts after Pure source cleanup",
                g.term(block)?,
            )?,
            _ => {}
        }
        for (_, edge) in g.edges.iter().filter(|(from, _)| *from == block) {
            pending.push((edge.target, made, ended, dropped));
        }
    }
    need(
        returned > 0,
        "normal empty-Map CFG path exists",
        g.function.id,
    )
}
