//! Bounded lexical retention of a real owned Map<String, empty Resource class> parameter.
//! Uses the unit's own complete facts; no single-file owner/selector facts are synthesized.
use super::*;
use lang_frontend::{
    parser::{FunctionBody, FunctionForm},
    type_checking::NominalKind,
};

pub(super) fn qualify(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    plan: &UnitMapIterationOwnershipPlan,
    root: UnitSymbolId,
    kind: UnitIterationExitKind,
) -> Result<UnitDropFact, LoweringError> {
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
    // Reuse the existing nullable Map payload ABI; never replace the descriptor/loan type.
    let value_type = match typed.types().get(d.value_type()) {
        Some(UnitTypeKind::Nullable(inner)) => *inner,
        _ => d.value_type(),
    };
    let Some(UnitTypeKind::Nominal {
        declaration,
        arguments,
    }) = typed.types().get(value_type)
    else {
        return Err(unsupported());
    };
    let nominal = typed
        .signatures()
        .declaration(*declaration)
        .and_then(|s| s.nominal())
        .ok_or_else(bad)?;
    if d.kind() != MapIterationKind::Map
        || !matches!(
            kind,
            UnitIterationExitKind::Fallthrough
                | UnitIterationExitKind::Continue(_)
                | UnitIterationExitKind::Break(_)
        )
        || plan.source_access() != UnitIterationSourceAccess::Owned
        || typed.is_resource_type(d.source_type()) != Some(true)
        || builtin_type(typed, d.key_type()) != Some(BuiltinType::String)
        || !arguments.is_empty()
        || nominal.kind() != NominalKind::Class
        || !nominal.has_deinit()
        || !nominal.fields().is_empty()
        || !nominal.type_parameters().is_empty()
        || !nominal.interfaces().is_empty()
    {
        return Err(unsupported());
    }
    let scope = scope(parsed, statement.statement(), origin, span)?;
    let drops = owned
        .drops()
        .iter()
        .copied()
        .filter(|f| f.target() == UnitDropTarget::Named(root))
        .collect::<Vec<_>>();
    let [drop] = drops.as_slice() else {
        return Err(unsupported());
    };
    if drop.point()
        != UnitDropPoint::AfterStatement(UnitStatementId::new(statement.source_unit(), scope))
        || drop.value_origin() != origin
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
        UnitIterationExitKind::Continue(expression) | UnitIterationExitKind::Break(expression) => {
            UnitDropPoint::ControlTransfer(expression)
        }
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
    if matches!(kind, UnitIterationExitKind::Break(_)) {
        body_actions.extend([
            Action::FinishProvider(statement),
            Action::EndSource(statement),
        ]);
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
    Ok(*drop)
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
