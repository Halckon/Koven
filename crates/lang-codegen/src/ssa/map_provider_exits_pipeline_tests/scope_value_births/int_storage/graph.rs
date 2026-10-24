//! Concrete Int storage, actual Copy reads, and complete current-owner transport.
mod factory;
mod provider;
mod reader;
use super::super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, loan, result, value},
    query_inputs::one,
};
use super::{Case, ast, facts::Proof};
use crate::ssa::model::*;
use lang_frontend::{parser::ParsedFile, source::Span};
use std::collections::{BTreeMap, BTreeSet};
fn at<'ir>(
    g: &Graph<'ir>,
    span: Span,
    predicate: impl Fn(&Operation) -> bool,
) -> Result<&'ir Instruction, Failure> {
    one(
        g.instructions
            .iter()
            .copied()
            .filter(|op| op.origin == Origin::Source(span) && predicate(&op.operation)),
        "Int exact original SSA operation",
    )
}
fn family(g: &Graph<'_>, seed: EntityId) -> Result<BTreeSet<EntityId>, Failure> {
    let mut all = BTreeSet::from([seed]);
    loop {
        let before = all.len();
        for (_, e) in &g.edges {
            for (&a, &p) in e.arguments.iter().zip(&g.block(e.target)?.parameters) {
                if all.contains(&a) {
                    all.insert(p);
                }
            }
        }
        if before == all.len() {
            break;
        }
    }
    for &e in &all {
        g.anchored(e, &[seed])?;
    }
    Ok(all)
}
pub(super) fn check(
    c: &Case,
    p: &ParsedFile,
    i: &ast::Input,
    proof: &Proof,
    program: &Program,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "Int actual original module")?;
    let scan = function(module, i.scan)?;
    let g = Graph::with_abort(scan)?;
    let entry = function(module, i.entry)?;
    let eg = Graph::new(entry)?;
    let invoke = at(&eg, ast::span(p, i.scanner_call), |op| {
        matches!(op, Operation::DirectCall { .. })
    })?;
    need(
        invoke.operation
            == Operation::DirectCall {
                callee: scan.id,
                receiver: None,
                arguments: vec![],
            }
            && invoke.results.is_empty()
            && eg.blocks.len() == 1
            && eg
                .block(entry.entry_block().unwrap())?
                .parameters
                .is_empty(),
        "Int original entry actual zeroarg scan call",
        invoke,
    )?;
    let birth = at(&g, ast::span(p, i.birth), |op| {
        matches!(op, Operation::DirectCall { .. })
    })?;
    let factory = function(module, i.make)?;
    need(
        birth.operation
            == Operation::DirectCall {
                callee: factory.id,
                receiver: None,
                arguments: vec![],
            }
            && g.block(scan.entry_block().unwrap())?.parameters.is_empty(),
        "Int original named source birth actual makeInt call",
        birth,
    )?;
    let seed = result(birth)?;
    let owner = value(birth)?;
    let map = scan.entity(seed).unwrap().ty.semantic_type();
    let (kind, key, val) = module
        .map_container(map)
        .ok_or_else(|| failure("Int actual Map output storage", map))?;
    need(
        kind == MapContainerKind::Map
            && key == val
            && module.type_kind(key)
                == Some(&SsaTypeKind::Integer {
                    bits: 32,
                    signed: true,
                })
            && factory.return_types.as_slice() == [map],
        "Int actual concrete Copy Int32 key/value map storage",
        (map, key, val),
    )?;
    factory::check(p, i, module, map)?;
    let owners = family(&g, seed)?;
    let mut uses = Uses::default();
    for &e in &owners {
        uses.protect(e);
    }
    let drop=one(g.instructions.iter().copied().filter(|op|matches!(op.operation,Operation::Drop{owner}if owners.contains(&EntityId::Value(owner)))),"Int unique actual current Map cleanup")?;
    need(
        drop.origin == Origin::Source(proof.drop) && proof.drop == i.source,
        "Int same-analysis original named source Drop provenance",
        drop,
    )?;
    if let Some(site) = &i.site {
        reader::check(p, i, module, key)?;
        provider::check(
            &g,
            module,
            function(module, i.reader)?,
            p,
            (site, seed, drop),
            &mut uses,
        )?;
    } else {
        let size = at(&g, ast::span(p, i.size.unwrap()), |op| {
            matches!(op, Operation::MapSize { .. })
        })?;
        need(
            size.operation == Operation::MapSize { owner: seed }
                && drop.operation == Operation::Drop { owner }
                && size.block == drop.block
                && birth.block == size.block
                && g.position(birth)? < g.position(size)?
                && g.position(drop)? == g.position(size)? + 1
                && !g.instructions.iter().any(|op| {
                    matches!(
                        op.operation,
                        Operation::MapIterationFirst { .. }
                            | Operation::MapIterationHasNext { .. }
                            | Operation::MapIterationNext { .. }
                            | Operation::MapIterationKeyLoan { .. }
                            | Operation::MapIterationValueLoan { .. }
                    )
                }),
            "Int control real current MapSize then immediate unique owner Drop",
            (birth, size, drop),
        )?;
        uses.permit(seed, size);
    }
    let Operation::Drop { owner: current } = drop.operation else {
        unreachable!()
    };
    uses.permit(EntityId::Value(current), drop);
    for (index, data) in scan.values.iter().enumerate() {
        if module.map_container(data.ty.semantic_type()).is_some() {
            need(
                owners.contains(&EntityId::Value(ValueId {
                    function: scan.id,
                    index,
                })),
                "Int every scanner Map Value is the exact current source family",
                (index, data),
            )?;
        }
    }
    need(c.provider==i.site.is_some()&&g.instructions.iter().filter(|op|matches!(op.operation,Operation::Drop{owner}if module.map_container(scan.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())).count()==1&&!g.instructions.iter().any(|op|matches!(op.operation,Operation::MapConstruct{..}|Operation::MapPut{..}|Operation::MapConsume{..}))&&g.blocks.iter().all(|&block|!matches!(&g.term(block).unwrap().kind,TerminatorKind::Return{values}if !values.is_empty())),"Int one current cleanup and void terminal, no alternate scanner Map birth",scan.id)?;
    crate::ssa::map_provider_exits_pipeline_tests::local_exits::owner_flow::check(
        &g,
        module,
        &BTreeSet::from([seed]),
    )?;
    uses.check(&g, None)
}
