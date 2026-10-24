//! Exact original epoch endpoints, paired with the real Unit assignment/current-owner SSA proof.
use super::*;
use lang_frontend::{
    name_resolution::UnitSymbolId,
    ownership_checking::{CompilationUnitOwnership, UnitDropPoint, UnitDropTarget},
    source::Span,
    type_checking::{UnitExpressionId, UnitStatementId},
};
#[derive(Debug)]
pub(in crate::ssa::map_provider_exits_pipeline_tests::scope_value_births::owner_update_observer)
struct Proof
{
    pub old_origin: Option<Span>,
    pub final_origin: Span,
    pub flag_in_provider: bool,
}
fn origin(c: &Case, p: &ParsedFile, i: &Input) -> Span {
    // P10 keeps both identities: old Drop is tied to i.source below, while
    // facts::unit proves this assignment targets that exact qualified source.
    // provider_graph then ties its fresh RHS/current owner to this final Drop.
    if c.code == 11 || c.code == 10 {
        span(p, i.assignment.unwrap().0)
    } else {
        i.source
    }
}
pub(super) fn unit(
    c: &Case,
    p: &ParsedFile,
    i: &Input,
    o: &CompilationUnitOwnership,
    root: UnitSymbolId,
) -> Result<Proof, Failure> {
    let e = |id| UnitExpressionId::new(root.source_unit(), id);
    for (receiver, clone) in clones(p, i)? {
        let drop = one(
            o.drops()
                .iter()
                .filter(|d| d.target() == UnitDropTarget::Temporary(e(receiver))),
            "update qualified clone receiver temporary Drop",
        )?;
        need(
            drop.point() == UnitDropPoint::CallReturn(e(clone))
                && drop.value_origin() == span(p, receiver),
            "update qualified clone receiver exact original CallReturn/origin",
            drop,
        )?;
    }
    let drops = o
        .drops()
        .iter()
        .filter(|d| d.target() == UnitDropTarget::Named(root))
        .collect::<Vec<_>>();
    need(
        drops.len() == if c.code == 9 { 1 } else { 2 },
        "update exact Unit source epoch obligation count",
        &drops,
    )?;
    let last = one(
        drops
            .iter()
            .copied()
            .filter(|d| d.point() == UnitDropPoint::CallReturn(e(i.observes.last().unwrap().0))),
        "update Unit exact last observe CallReturn source Drop",
    )?;
    need(
        last.value_origin() == origin(c, p, i),
        "update Unit current assignment/declaration Drop origin",
        last,
    )?;
    let old = if c.code == 9 {
        None
    } else {
        let point = match c.code {
            7 | 8 => UnitDropPoint::AfterExpression(e(i.births[1].expression)),
            10 => UnitDropPoint::AfterStatement(UnitStatementId::new(
                root.source_unit(),
                super::super::ast::declaration(p, i.source)?,
            )),
            11 => {
                if let Some((statement, _)) = i.site {
                    UnitDropPoint::LoopExit(UnitStatementId::new(root.source_unit(), statement))
                } else {
                    UnitDropPoint::CallReturn(e(i.observes[0].0))
                }
            }
            _ => unreachable!(),
        };
        let old = one(
            drops.iter().copied().filter(|d| d.point() == point),
            "update Unit exact original replacement/old source endpoint",
        )?;
        need(
            old.value_origin() == i.source,
            "update Unit old epoch original declaration marker",
            old,
        )?;
        Some(old)
    };
    Ok(Proof {
        old_origin: old.map(|v| v.value_origin()),
        final_origin: last.value_origin(),
        flag_in_provider: i.flag.is_some(),
    })
}
fn clones(
    p: &ParsedFile,
    i: &Input,
) -> Result<
    Vec<(
        lang_frontend::ast::ExpressionId,
        lang_frontend::ast::ExpressionId,
    )>,
    Failure,
> {
    let Some(put) = i.put else {
        return Ok(vec![]);
    };
    let Expression::Call { arguments, .. } = p.ast().expressions().get(put).unwrap().payload()
    else {
        unreachable!()
    };
    let receivers = super::super::ast::clone_receivers(p, i)?;
    Ok(receivers
        .into_iter()
        .zip(arguments.iter().map(|a| a.value))
        .collect())
}
