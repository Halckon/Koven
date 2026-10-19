//! The selected ordinary Call delivers real argument owners before its returned Map is used.
mod arguments;
mod control;
mod factory;
mod provider;
use super::{Case, Delivery, ast};
use crate::ssa::{
    map_provider_exits_pipeline_tests::{
        fixture::{Failure, failure, need},
        query_graph::{Graph, Uses, function, loan, result, value},
        query_inputs::one,
    },
    model::*,
};
use lang_frontend::{
    parser::ParsedFile,
    source::{SourceMap, Span},
};
use std::collections::BTreeSet;

fn at<'ir>(
    g: &Graph<'ir>,
    origin: Span,
    predicate: impl Fn(&Operation) -> bool,
) -> Result<&'ir Instruction, Failure> {
    one(
        g.instructions
            .iter()
            .copied()
            .filter(|i| i.origin == Origin::Source(origin) && predicate(&i.operation)),
        "delivery actual original SSA operation",
    )
}

/// Reuse the shared read-only complete-incoming proof for all delivery roles.
fn anchored(g: &Graph<'_>, current: EntityId, anchors: &[EntityId]) -> Result<(), Failure> {
    g.anchored(current, anchors)
}

fn marker<'ir>(g: &Graph<'ir>, span: Span) -> Result<&'ir Instruction, Failure> {
    at(g, span, |operation| match operation {
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
                |i| matches!(&i.operation,Operation::StringLiteral {bytes,..} if bytes==b"after"),
            )
        }
        _ => false,
    })
}

pub(super) fn check(
    case: &Case,
    sources: &SourceMap,
    parsed: &ParsedFile,
    program: &Program,
    control: bool,
) -> Result<(), Failure> {
    let input = ast::input(case, sources, parsed, control)?;
    let module = one(program.modules.iter(), "delivery original module")?;
    let entry = function(module, input.entry.span)?;
    let factory = function(module, input.factory.span)?;
    let g = Graph::with_abort(entry)?;
    need(
        g.block(entry.entry_block().unwrap())?.parameters.is_empty(),
        "delivery entry has no parameter substitutes",
        entry.id,
    )?;
    let mut uses = Uses::default();
    let delivered = arguments::check(&g, module, factory, parsed, &input, &mut uses)?;
    let owner = value(delivered.call)?;
    let map = entry
        .entity(EntityId::Value(owner))
        .unwrap()
        .ty
        .semantic_type();
    let (kind, key, val) = module
        .map_container(map)
        .ok_or_else(|| failure("delivery returned owning Map", map))?;
    need(
        kind == if case.mutable {
            MapContainerKind::MutableMap
        } else {
            MapContainerKind::Map
        } && module.type_kind(key) == Some(&SsaTypeKind::StringOwner)
            && if matches!(case.delivery, Delivery::SameMapBorrow) {
                module.type_kind(val) == Some(&SsaTypeKind::StringOwner)
            } else {
                matches!(module.type_kind(val), Some(SsaTypeKind::HeapOwner { .. }))
            }
            && factory.return_types.as_slice() == [map]
            && factory.receiver.is_none()
            && factory.borrow_return.is_none()
            && factory.carrier_return.is_none(),
        "delivery selected result kind and actual Pure/Resource representation",
        (factory.id, map),
    )?;
    factory::check(case, sources, parsed, module, factory, &input, map)?;
    let after = marker(&g, input.after)?;
    if let Some(site) = &input.site {
        provider::check(
            &g,
            module,
            function(module, input.reader.span)?,
            parsed,
            site,
            (&delivered, after),
            &mut uses,
        )?;
    } else {
        control::check(
            case,
            &g,
            module,
            parsed,
            &input,
            (&delivered, after),
            &mut uses,
        )?;
    }
    need(
        !delivered.argument_owners.contains(&owner),
        "delivery result cannot alias any argument owner",
        owner,
    )?;
    // Same-K/V input Map has its own synchronous Drop; count both distinct families.
    let map_drops=g.instructions.iter().filter(|i|matches!(i.operation,Operation::Drop {owner}
        if module.map_container(entry.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())).count();
    need(
        map_drops == 1 + usize::from(matches!(case.delivery, Delivery::SameMapBorrow)),
        "delivery exact disjoint argument/output Map cleanup count",
        map_drops,
    )?;
    uses.check(&g, None)
}
