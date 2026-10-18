//! Future use preserves the real owner until its published ASAP Drop, independently of AST layout.
use super::*;
use lang_frontend::name_resolution::ReferenceTarget;

pub(super) fn qualify(
    parsed: &ParsedFile,
    names: &NameResolution,
    owned: &OwnershipCheckedFile,
    plan: &MapIterationOwnershipPlan,
    symbol: SymbolId,
    body_exit: IterationExitKind,
    span: Span,
) -> Result<Proof, LoweringError> {
    let bad = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    if !matches!(
        body_exit,
        IterationExitKind::Fallthrough
            | IterationExitKind::Continue(_)
            | IterationExitKind::Break(_)
    ) || plan.closure_phis().len() != 2
        || plan.closure_phi_incomings().len() != 3
        || !plan.capture_graph().nodes().is_empty()
    {
        return Err(unsupported());
    }
    let drops = owned
        .drops()
        .iter()
        .copied()
        .filter(|f| f.target() == DropTarget::Named(symbol))
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        return Err(unsupported());
    };
    let point = match drop.point() {
        DropPoint::CallReturn(e)
        | DropPoint::AfterExpression(e)
        | DropPoint::ControlTransfer(e) => {
            parsed.ast().expressions().get(e).map_err(|_| bad())?.span()
        }
        DropPoint::LoopExit(s) | DropPoint::AfterStatement(s) => {
            parsed.ast().statements().get(s).map_err(|_| bad())?.span()
        }
        _ => return Err(unsupported()),
    };
    if !crate::ssa::map_source::future_reference(
        parsed,
        span,
        point,
        names
            .references()
            .iter()
            .filter_map(|r| (r.target() == &ReferenceTarget::Symbol(symbol)).then_some(r.span())),
    ) {
        return Err(unsupported());
    }
    let origin = names.symbols().get(symbol.index()).ok_or_else(bad)?.span();
    let conditions = owned.cleanup_conditions();
    for boundary in [IterationPhiBoundary::Header, IterationPhiBoundary::Exit] {
        let phi = super::phi(conditions, plan, symbol, boundary, span)?;
        super::provenance::parameter(owned, symbol, origin, phi.owner(), span)?;
    }
    let condition =
        super::provenance::parameter(owned, symbol, origin, drop.owner().ok_or_else(bad)?, span)?;
    if drop.value_origin() != origin
        || drop.condition() != Some(condition)
        || drop.capture_slot().is_some()
        || drop.instance_address().is_some()
    {
        return Err(bad());
    }
    if plan.exits().iter().any(|exit| {
        exit.actions().iter().any(|action| {
            matches!(action, IterationCleanupAction::Drop(fact)
                if fact.target() == DropTarget::Named(symbol))
        })
    }) {
        return Err(bad());
    }
    let exhaustion = unique(
        plan.exits()
            .iter()
            .filter(|e| e.kind() == IterationExitKind::Exhaustion),
    )
    .ok_or_else(bad)?;
    if exhaustion.actions()
        != [
            IterationCleanupAction::FinishProvider(plan.descriptor().statement()),
            IterationCleanupAction::EndSource(plan.descriptor().statement()),
        ]
    {
        return Err(bad());
    }
    Ok(Proof {
        drop: *drop,
        transfer: None,
        conditional_return: None,
    })
}
