//! Exact published cleanup endpoints and current owner identities, from the same analysis.
use super::*;
use lang_frontend::{
    name_resolution::ValidatedCompilationUnitNames,
    ownership_checking::{CompilationUnitOwnership, UnitDropPoint, UnitDropTarget},
    parser::Expression,
    type_checking::UnitExpressionId,
};
#[derive(Debug)]
pub(in crate::ssa::map_provider_exits_pipeline_tests::scope_future_observer) struct Proof {
    pub final_origin: Option<Span>,
    pub caller_origin: Option<Span>,
}
fn final_marker(c: &Case, i: &Input) -> Result<Option<Span>, Failure> {
    match c.code {
        19 => Ok(None),
        20 => Ok(Some(
            i.successor
                .ok_or_else(|| failure("D actual successor marker", i))?
                .0,
        )),
        21 => Ok(Some(i.source)),
        _ => unreachable!(),
    }
}
pub(super) fn clones(
    p: &ParsedFile,
    i: &Input,
) -> Result<Vec<(ExpressionId, ExpressionId)>, Failure> {
    let Some(put) = i.put else {
        return Ok(vec![]);
    };
    let Expression::Call { arguments, .. } = p.ast().expressions().get(put).unwrap().payload()
    else {
        return Err(failure("D original put Call", put));
    };
    need(
        arguments.len() == 2,
        "D put exact owned key/value slots",
        arguments,
    )?;
    arguments
        .iter()
        .map(|arg| {
            let Expression::Call {
                callee, arguments, ..
            } = p.ast().expressions().get(arg.value).unwrap().payload()
            else {
                return Err(failure("D original clone Call", arg));
            };
            let Expression::Member { receiver, .. } =
                p.ast().expressions().get(*callee).unwrap().payload()
            else {
                return Err(failure("D actual clone receiver", callee));
            };
            need(
                arguments.is_empty(),
                "D clone zero actual arguments",
                arguments,
            )?;
            Ok((*receiver, arg.value))
        })
        .collect()
}
pub(super) fn unit(
    c: &Case,
    p: &ParsedFile,
    i: &Input,
    n: &ValidatedCompilationUnitNames,
    o: &CompilationUnitOwnership,
) -> Result<Proof, Failure> {
    let root = super::unit::symbol(n, i.source)?;
    let e = |id| UnitExpressionId::new(root.source_unit(), id);
    for (receiver, clone) in clones(p, i)? {
        let effect = o
            .string_effect(e(clone))
            .ok_or_else(|| failure("D qualified actual clone effect", e(clone)))?;
        let d = one(
            o.drops()
                .iter()
                .filter(|d| d.target() == UnitDropTarget::Temporary(e(receiver))),
            "D qualified independent clone receiver Drop",
        )?;
        need(
            effect.receiver() == e(receiver)
                && d.point() == UnitDropPoint::CallReturn(e(clone))
                && d.value_origin() == ast::span(p, receiver),
            "D qualified clone receiver exact effect/CallReturn/origin",
            (effect, d),
        )?;
    }
    let final_origin = final_marker(c, i)?;
    for (marker, call) in final_origin
        .map(|m| (m, i.observes.last().unwrap().0))
        .into_iter()
        .chain(i.result.map(|m| (m, i.entry_observe.unwrap().0)))
    {
        let target = super::unit::symbol(n, marker)?;
        let d = one(
            o.drops()
                .iter()
                .filter(|d| d.target() == UnitDropTarget::Named(target)),
            "D qualified sole final named Drop",
        )?;
        need(
            d.point() == UnitDropPoint::CallReturn(e(call)) && d.value_origin() == marker,
            "D qualified last actual observe exact Drop point and declaration origin",
            d,
        )?;
    }
    Ok(Proof {
        final_origin,
        caller_origin: i.result,
    })
}
