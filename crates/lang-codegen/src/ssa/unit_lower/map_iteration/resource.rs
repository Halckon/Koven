//! Lexical retention of a real owned Map/MutableMap parameter with represented Resource storage.
//! Uses the unit's own complete facts; no single-file owner/selector facts are synthesized.
use super::*;
use lang_frontend::parser::{FunctionBody, FunctionForm};

#[derive(Clone, Copy)]
pub(super) struct Proof {
    pub(super) drop: UnitDropFact,
    pub(super) transfer: Option<UnitDropFact>,
}

pub(super) fn qualify(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    plan: &UnitMapIterationOwnershipPlan,
    root: (
        UnitSymbolId,
        Option<&crate::ssa::map_source::Formation<UnitSymbolId>>,
    ),
    kind: UnitIterationExitKind,
) -> Result<Proof, LoweringError> {
    let (root, formation) = root;
    let d = plan.descriptor();
    let statement = d.statement();
    let span = parsed
        .ast()
        .statements()
        .get(statement.statement())
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?
        .span();
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let origin = names
        .names()
        .source_units()
        .get(root.source_unit().index())
        .and_then(|s| s.resolution().symbols().get(root.symbol().index()))
        .ok_or_else(bad)?
        .span();
    // Keep the unit's real owned Resource facts; representation admission belongs
    // to existing storage planning, not a nominal-shape/key whitelist here.
    if !matches!(
        kind,
        UnitIterationExitKind::Fallthrough
            | UnitIterationExitKind::Continue(_)
            | UnitIterationExitKind::Break(_)
            | UnitIterationExitKind::Return(_)
    ) || plan.source_access() != UnitIterationSourceAccess::Owned
        || typed.is_resource_type(d.source_type()) != Some(true)
    {
        return Err(unsupported());
    }
    let origin = formation.map_or(origin, |f| f.origin);
    let scope = scope(parsed, statement.statement(), origin, span)?;
    let drops = if let Some(formation) = formation {
        formed::drops(parsed, owned, root, formation)?
    } else {
        owned
            .drops()
            .iter()
            .copied()
            .filter(|f| f.target() == UnitDropTarget::Named(root))
            .collect::<Vec<_>>()
    };
    let returning = match kind {
        UnitIterationExitKind::Return(e) => Some(e),
        _ => None,
    };
    let transfer = returning
        .map(|expression| {
            if expression.source_unit() != statement.source_unit() {
                return Err(bad());
            }
            let node = parsed
                .ast()
                .expressions()
                .get(expression.expression())
                .map_err(|_| bad())?;
            if !matches!(node.payload(), Expression::Return { .. })
                || node.span().source_id() != span.source_id()
                || node.span().start() <= span.start()
                || node.span().end() >= span.end()
            {
                return Err(bad());
            }
            let mut found = drops
                .iter()
                .copied()
                .filter(|d| d.point() == UnitDropPoint::ControlTransfer(expression));
            let fact = found.next().ok_or_else(bad)?;
            if found.next().is_some() {
                return Err(bad());
            }
            Ok(fact)
        })
        .transpose()?;
    if drops.len() != 1 + usize::from(returning.is_some()) {
        return Err(unsupported());
    }
    let mut normal = drops.iter().copied().filter(|d| Some(*d) != transfer);
    let drop = normal.next().ok_or_else(bad)?;
    if normal.next().is_some()
        || drops.iter().any(|d| d.value_origin() != origin)
        || (!normal_point(parsed, scope, span, statement.source_unit(), drop.point())
            && !matches!((formation.and_then(|f| f.replacement), drop.point()), (Some(expected), UnitDropPoint::AfterExpression(actual)) if actual.source_unit() == root.source_unit() && actual.expression() == expected))
    {
        return Err(bad());
    }
    let node = parsed
        .ast()
        .statements()
        .get(statement.statement())
        .map_err(|_| bad())?;
    let Statement::For { body, .. } = node.payload() else {
        return Err(bad());
    };
    let [body_plan, exhaustion] = plan.exits() else {
        return Err(bad());
    };
    let body_point = match kind {
        UnitIterationExitKind::Continue(expression)
        | UnitIterationExitKind::Break(expression)
        | UnitIterationExitKind::Return(expression) => UnitDropPoint::ControlTransfer(expression),
        _ => UnitDropPoint::AfterStatement(UnitStatementId::new(statement.source_unit(), *body)),
    };
    let mut body_actions = vec![
        Action::EndBinding {
            statement,
            symbol: d.value_binding(),
        },
        Action::EndBinding {
            statement,
            symbol: d.key_binding(),
        },
    ];
    if matches!(
        kind,
        UnitIterationExitKind::Break(_) | UnitIterationExitKind::Return(_)
    ) {
        body_actions.extend([
            Action::FinishProvider(statement),
            Action::EndSource(statement),
        ]);
    }
    if let Some(fact) = transfer {
        body_actions.push(Action::Drop(fact));
    }
    let exhaustion_actions = [
        Action::FinishProvider(statement),
        Action::EndSource(statement),
    ];
    if body_plan.kind() != kind
        || body_plan.point() != body_point
        || !body_plan.actions().ends_with(&body_actions)
        || exhaustion.kind() != UnitIterationExitKind::Exhaustion
        || exhaustion.point() != UnitDropPoint::LoopExit(statement)
        || exhaustion.actions() != exhaustion_actions
        || owned.iteration_cleanup_at(body_point) != Some(body_plan.actions())
        || owned.iteration_cleanup_at(exhaustion.point()) != Some(exhaustion.actions())
    {
        return Err(bad());
    }
    let prefix = &body_plan.actions()[..body_plan.actions().len() - body_actions.len()];
    if prefix.iter().any(|action| !matches!(action, Action::Drop(fact) if fact.target()!=UnitDropTarget::Named(root))) {
        return Err(bad());
    }
    Ok(Proof { drop, transfer })
}

fn scope(
    parsed: &ParsedFile,
    statement: StatementId,
    origin: Span,
    span: Span,
) -> Result<StatementId, LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let bodies = parsed
        .roots()
        .iter()
        .filter_map(|id| {
            let item = parsed.ast().items().get(*id).ok()?;
            if item.span().source_id() != origin.source_id()
                || item.span().start() > origin.start()
                || item.span().end() < origin.end()
            {
                return None;
            }
            match item.payload() {
                Item::Function {
                    form:
                        FunctionForm::ImplicitUnitBlock(b)
                        | FunctionForm::Explicit {
                            body: FunctionBody::Block(b),
                            ..
                        },
                    ..
                } => Some(*b),
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    let [body] = bodies.as_slice() else {
        return Err(bad());
    };
    // The validated Drop names the actual callable scope. Continuation statements
    // are lowered normally; their AST shape cannot grant or revoke source ownership.
    let node = parsed.ast().statements().get(*body).map_err(|_| bad())?;
    let iteration = parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|_| bad())?;
    if !matches!(node.payload(), Statement::Block { .. })
        || node.span().source_id() != iteration.span().source_id()
        || node.span().start() > iteration.span().start()
        || node.span().end() < iteration.span().end()
    {
        return Err(bad());
    }
    Ok(*body)
}

// A cleanup Return must belong to the original callable, beyond the provider's AST.
fn normal_point(
    parsed: &ParsedFile,
    scope: StatementId,
    iteration: Span,
    source_unit: lang_frontend::name_resolution::SourceUnitId,
    point: UnitDropPoint,
) -> bool {
    if point == UnitDropPoint::AfterStatement(UnitStatementId::new(source_unit, scope)) {
        return true;
    }
    let UnitDropPoint::ControlTransfer(expression) = point else {
        return false;
    };
    if expression.source_unit() != source_unit {
        return false;
    }
    let Ok(node) = parsed.ast().expressions().get(expression.expression()) else {
        return false;
    };
    let Ok(body) = parsed.ast().statements().get(scope) else {
        return false;
    };
    let at = node.span();
    let inside = |outer: Span| {
        outer.source_id() == at.source_id()
            && outer.start() <= at.start()
            && outer.end() >= at.end()
    };
    matches!(node.payload(), Expression::Return { .. })
        && inside(body.span())
        && at.start() >= iteration.end()
        && !parsed
            .ast()
            .expressions()
            .iter()
            .any(|(_, n)| matches!(n.payload(), Expression::Lambda { .. }) && inside(n.span()))
        && !parsed.ast().items().iter().any(|(_, n)| {
            matches!(n.payload(), Item::Function { .. } | Item::Deinit { .. })
                && n.span().start() >= body.span().start()
                && inside(n.span())
        })
}
