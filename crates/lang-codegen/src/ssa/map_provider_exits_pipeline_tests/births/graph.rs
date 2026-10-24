//! Source-born SSA owner anchors, complete edge transport and all protected operand uses.
mod callable;
mod control;
mod intrinsic;
pub(in crate::ssa::map_provider_exits_pipeline_tests) mod provider;
use super::super::{
    fixture::{Failure, failure, need},
    query_graph::{Graph, Uses, function, loan, result, value},
    query_inputs::one,
};
use super::{Case, Kind, ast};
use crate::ssa::model::*;
use lang_frontend::{
    parser::ParsedFile,
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
        "birth exact original SSA operation",
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
        if before == all.len() {
            break;
        }
    }
    for &e in &all {
        g.anchored(e, &[anchor])?;
    }
    Ok(all)
}
fn after<'ir>(g: &Graph<'ir>, span: Span) -> Result<&'ir Instruction, Failure> {
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
    proof: &super::facts::Proof,
    program: &Program,
) -> Result<(), Failure> {
    let module = one(program.modules.iter(), "birth actual original module")?;
    let entry = function(module, input.entry)?;
    let g = Graph::with_abort(entry)?;
    let mut uses = Uses::default();
    need(
        g.block(entry.entry_block().unwrap())?.parameters.is_empty(),
        "birth source entry has no substitute owner parameter",
        entry.id,
    )?;
    let (seed, prior) = if case.kind == Kind::FunctionValue {
        (
            callable::check(case, sources, parsed, (input, proof), module, &g, &mut uses)?,
            BTreeSet::new(),
        )
    } else {
        intrinsic::check(case, parsed, input, module, &g, &mut uses)?
    };
    let map = entry.entity(seed).unwrap().ty.semantic_type();
    let (kind, key, val) = module
        .map_container(map)
        .ok_or_else(|| failure("birth actual represented output owner", seed))?;
    need(
        kind == if case.mutable {
            MapContainerKind::MutableMap
        } else {
            MapContainerKind::Map
        } && [key, val]
            .iter()
            .all(|ty| module.type_kind(*ty) == Some(&SsaTypeKind::StringOwner)),
        "birth exact output kind/KV storage",
        map,
    )?;
    let owners = family(&g, seed)?;
    for &e in &owners {
        uses.protect(e);
    }
    let marker = after(&g, input.after)?;
    let drop=one(g.instructions.iter().copied().filter(|i|matches!(i.operation,Operation::Drop{owner}if owners.contains(&EntityId::Value(owner)))),"birth unique output family cleanup")?;
    let Operation::Drop { owner } = drop.operation else {
        unreachable!()
    };
    need(
        drop.origin == Origin::Source(input.root.unwrap_or(ast::span(parsed, input.birth))),
        "birth actual named/temporary Drop source identity",
        drop,
    )?;
    if let Some(site) = &input.site {
        provider::check(
            &g,
            module,
            function(module, input.reader)?,
            parsed,
            (site, seed, drop, marker),
            &mut uses,
        )?;
    } else {
        let size = at(&g, ast::span(parsed, input.size.unwrap()), |o| {
            matches!(o, Operation::MapSize { .. })
        })?;
        need(
            size.operation == Operation::MapSize { owner: seed }
                && drop.operation
                    == Operation::Drop {
                        owner: match seed {
                            EntityId::Value(v) => v,
                            _ => return Err(failure("birth actual owner Value", seed)),
                        },
                    }
                && size.block == drop.block
                && g.position(drop)? == g.position(size)? + 1
                && !g.instructions.iter().any(|i| {
                    matches!(
                        i.operation,
                        Operation::MapIterationFirst { .. }
                            | Operation::MapIterationHasNext { .. }
                            | Operation::MapIterationNext { .. }
                            | Operation::MapIterationKeyLoan { .. }
                            | Operation::MapIterationValueLoan { .. }
                    )
                }),
            "birth no-For actual owner size/ASAP Drop before after",
            (size, drop, marker),
        )?;
        uses.permit(seed, size);
        control::check(&g, seed, size, drop, marker)?;
    }
    uses.permit(EntityId::Value(owner), drop);
    for (index, data) in entry.values.iter().enumerate() {
        if module.map_container(data.ty.semantic_type()).is_some() {
            let e = EntityId::Value(ValueId {
                function: entry.id,
                index,
            });
            need(
                owners.contains(&e) || prior.contains(&e),
                "birth every Map Value belongs to actual current or consumed predecessor family",
                (e, data),
            )?;
        }
    }
    let all_drops=g.instructions.iter().filter(|i|matches!(i.operation,Operation::Drop{owner}if module.map_container(entry.entity(EntityId::Value(owner)).unwrap().ty.semantic_type()).is_some())).count();
    need(
        all_drops == 1,
        "birth sole Map cleanup, no old construction/put/consume duplicate",
        all_drops,
    )?;
    let births = prior.iter().copied().chain([seed]).collect::<BTreeSet<_>>();
    crate::ssa::map_provider_exits_pipeline_tests::local_exits::owner_flow::check(
        &g, module, &births,
    )?;
    uses.check(&g, None)
}
