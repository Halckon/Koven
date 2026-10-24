//! Actual selected SSA Call anchors all current output/argument owner families.
mod arguments;
mod cfg;
mod provider;
mod receiver;
use super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, loan, result, value},
    query_inputs::one,
};
use super::{Case, ast, facts};
use crate::ssa::model::*;
use lang_frontend::{
    parser::{Expression, ParsedFile},
    source::{SourceMap, Span},
};
use std::collections::BTreeSet;
fn at<'ir>(
    g: &Graph<'ir>,
    span: Span,
    predicate: impl Fn(&Operation) -> bool,
) -> Result<&'ir Instruction, Failure> {
    one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.origin == Origin::Source(span) && predicate(&i.operation)),
        "local argument exact original SSA operation",
    )
}
fn family(g: &Graph<'_>, anchor: EntityId) -> Result<BTreeSet<EntityId>, Failure> {
    let mut all = BTreeSet::from([anchor]);
    loop {
        let before = all.len();
        for (_, edge) in &g.edges {
            for (&a, &p) in edge.arguments.iter().zip(&g.block(edge.target)?.parameters) {
                if all.contains(&a) {
                    all.insert(p);
                }
            }
        }
        if all.len() == before {
            break;
        }
    }
    for &e in &all {
        g.anchored(e, &[anchor])?;
    }
    Ok(all)
}
fn anchored(g: &Graph<'_>, current: EntityId, anchors: &[EntityId]) -> Result<(), Failure> {
    g.anchored(current, anchors)
}
fn marker<'ir>(g: &Graph<'ir>, span: Span) -> Result<&'ir Instruction, Failure> {
    at(g, span, |o| match o {
        Operation::PrintLiteral { bytes } => bytes == b"after\n",
        Operation::PrintString { value } => {
            let Ok(begin) = g.definition(EntityId::Loan(*value)) else {
                return false;
            };
            let Operation::BorrowBegin { place, .. } = begin.operation else {
                return false;
            };
            let Ok(root) = g.definition(EntityId::Place(place)) else {
                return false;
            };
            let Operation::RootPlace { owner } = root.operation else {
                return false;
            };
            g.definition(EntityId::Value(owner)).is_ok_and(
                |i| matches!(&i.operation,Operation::StringLiteral{bytes,..}if bytes==b"after"),
            )
        }
        _ => false,
    })
}
pub(super) fn check(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    input: &ast::Input,
    proof: &facts::Proof,
    program: &Program,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "local argument original module")?;
    let entry = function(module, input.scope.span)?;
    let factory = function(module, proof.implementation)?;
    let g = Graph::with_abort(entry)?;
    let mut uses = Uses::default();
    let delivered = if input.arguments.iter().any(|a| {
        matches!(
            parsed
                .ast()
                .expressions()
                .get(a.expression)
                .unwrap()
                .payload(),
            Expression::If { .. }
        )
    }) {
        cfg::check(sources, parsed, input, module, &g, factory, &mut uses)?
    } else {
        arguments::check(sources, &g, module, factory, parsed, input, &mut uses)?
    };
    receiver::check(&g, parsed, delivered.call, input, proof, &mut uses)?;
    let owner = value(delivered.call)?;
    let map = entry
        .entity(EntityId::Value(owner))
        .unwrap()
        .ty
        .semantic_type();
    let (kind, key, val) = module
        .map_container(map)
        .ok_or_else(|| failure("local argument actual returned Map", map))?;
    need(
        kind == if case.mutable {
            MapContainerKind::MutableMap
        } else {
            MapContainerKind::Map
        } && module.type_kind(key) == Some(&SsaTypeKind::StringOwner)
            && if case.resource {
                matches!(module.type_kind(val), Some(SsaTypeKind::HeapOwner { .. }))
            } else {
                module.type_kind(val) == Some(&SsaTypeKind::StringOwner)
            }
            && factory.return_types.as_slice() == [map],
        "local argument exact selected map result ABI",
        factory.id,
    )?;
    if case.label.starts_with("generic-wrapper-") {
        super::generic::wrapper::ssa(case.mutable, sources, parsed, module, factory, map)?;
    }
    let after = input.after.map(|span| marker(&g, span)).transpose()?;
    provider::check(
        case,
        &g,
        module,
        function(module, input.reader.span)?,
        parsed,
        (input, &delivered, after),
        &mut uses,
    )?;
    let owners = family(&g, EntityId::Value(owner))?;
    for &e in &owners {
        uses.protect(e);
    }
    need(
        !delivered.argument_owners.contains(&owner),
        "local argument result owner disjoint from argument owners",
        owner,
    )?;
    let map_drops=g.instructions.iter().filter(|i|matches!(i.operation,Operation::Drop{owner}if module.map_container(entry.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())).count();
    let input_maps = delivered
        .argument_owners
        .iter()
        .filter(|&&owner| {
            module
                .map_container(
                    entry
                        .entity(EntityId::Value(owner))
                        .unwrap()
                        .ty
                        .semantic_type(),
                )
                .is_some()
        })
        .count();
    need(
        map_drops == 1 + input_maps,
        "local argument exact disjoint input/output Map Drop families",
        map_drops,
    )?;
    let mut map_births = BTreeSet::from([EntityId::Value(owner)]);
    map_births.extend(
        delivered
            .argument_owners
            .iter()
            .copied()
            .filter_map(|owner| {
                let entity = EntityId::Value(owner);
                module
                    .map_container(entry.entity(entity).unwrap().ty.semantic_type())
                    .map(|_| entity)
            }),
    );
    crate::ssa::map_provider_exits_pipeline_tests::local_exits::owner_flow::check(
        &g,
        module,
        &map_births,
    )?;
    uses.check(&g, None)
}
