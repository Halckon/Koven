//! A Pure parameter may remain live until its actual later use; no lexical-lifetime substitution.
use super::*;
use lang_frontend::name_resolution::UnitReferenceTarget;

pub(super) fn qualify(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    plan: &UnitMapIterationOwnershipPlan,
    kind: UnitIterationExitKind,
    span: Span,
) -> Result<Option<UnitDropFact>, LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    if plan.source_access() != UnitIterationSourceAccess::Owned
        || typed.is_resource_type(plan.descriptor().source_type()) != Some(false)
    {
        return Ok(None);
    }
    let Some(UnitLoanTarget::Place(place)) = plan.source() else {
        return Err(bad());
    };
    if !place.is_root() {
        return Err(bad());
    }
    let root = place.root();
    let drops = owned
        .drops()
        .iter()
        .copied()
        .filter(|f| f.target() == UnitDropTarget::Named(root))
        .collect::<Vec<_>>();
    let local = |point| plan.exits().iter().any(|e| e.point() == point);
    if drops.iter().all(|drop| local(drop.point())) {
        return Ok(None);
    }
    if !matches!(
        kind,
        UnitIterationExitKind::Fallthrough
            | UnitIterationExitKind::Continue(_)
            | UnitIterationExitKind::Break(_)
    ) {
        return Err(unsupported());
    }
    let [drop] = drops.as_slice() else {
        return Err(unsupported());
    };
    let point = match drop.point() {
        UnitDropPoint::CallReturn(e)
        | UnitDropPoint::AfterExpression(e)
        | UnitDropPoint::ControlTransfer(e)
            if e.source_unit() == root.source_unit() =>
        {
            parsed
                .ast()
                .expressions()
                .get(e.expression())
                .map_err(|_| bad())?
                .span()
        }
        UnitDropPoint::LoopExit(s) | UnitDropPoint::AfterStatement(s)
            if s.source_unit() == root.source_unit() =>
        {
            parsed
                .ast()
                .statements()
                .get(s.statement())
                .map_err(|_| bad())?
                .span()
        }
        _ => return Err(unsupported()),
    };
    let origin = names
        .names()
        .source_units()
        .get(root.source_unit().index())
        .and_then(|s| s.resolution().symbols().get(root.symbol().index()))
        .ok_or_else(bad)?
        .span();
    if drop.value_origin() != origin {
        return Err(bad());
    }
    if !crate::ssa::map_source::future_reference(
        parsed,
        span,
        point,
        names
            .names()
            .references()
            .iter()
            .filter_map(|r| (r.target() == &UnitReferenceTarget::Symbol(root)).then_some(r.span())),
    ) {
        return Err(unsupported());
    }
    Ok(Some(*drop))
}
