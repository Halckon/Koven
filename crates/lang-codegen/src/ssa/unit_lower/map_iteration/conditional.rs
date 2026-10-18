//! Ordinary body CFG consumes the exact source-qualified public cleanup plan.
mod cleanup;
use super::*;
use lang_frontend::ownership_checking::UnitIterationExitPlan;

impl QualifiedConditional {
    /// A missing exit is a missing fact, never evidence that an owner is absent.
    pub(in super::super) fn exit_at(
        self,
        owned: &CompilationUnitOwnership,
        source: SourceIdentity,
        point: UnitDropPoint,
    ) -> Result<&UnitIterationExitPlan, LoweringError> {
        let bad = || LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        };
        let plan = owned.map_iteration(self.statement).ok_or_else(bad)?;
        if source != self.source || self.body.source_unit() != self.statement.source_unit() {
            return Err(bad());
        }
        let matches = match (source, plan.source()) {
            (
                SourceIdentity::Parameter { root, .. }
                | SourceIdentity::Formed { root, .. }
                | SourceIdentity::Local { root, .. },
                Some(UnitLoanTarget::Place(place)),
            ) => place.is_root() && place.root() == root,
            (SourceIdentity::Temporary(expected), Some(UnitLoanTarget::Temporary(actual))) => {
                expected == *actual
            }
            _ => false,
        };
        if !matches {
            return Err(bad());
        }
        let mut exits = plan.exits().iter().filter(|exit| exit.point() == point);
        let exit = exits.next().ok_or_else(bad)?;
        if exits.next().is_some() || owned.iteration_cleanup_at(point) != Some(exit.actions()) {
            return Err(bad());
        }
        Ok(exit)
    }

    /// Read only this plan's exact exit actions; never search a global root or old owner.
    pub(in super::super) fn drop_at(
        self,
        owned: &CompilationUnitOwnership,
        source: SourceIdentity,
        point: UnitDropPoint,
    ) -> Result<Option<UnitDropFact>, LoweringError> {
        let exit = self.exit_at(owned, source, point)?;
        cleanup::source_drop(owned, self.statement, exit)
    }
}

/// Bind an ordinary CFG or nested provider body to its source/epoch and all actual exits.
pub(super) fn qualify(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    plan: &UnitMapIterationOwnershipPlan,
    body_source: (StatementId, SourceIdentity),
    span: Span,
) -> Result<Option<QualifiedMap>, LoweringError> {
    let (body, source) = body_source;
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let body_span = parsed
        .ast()
        .statements()
        .get(body)
        .map_err(|_| bad())?
        .span();
    let has_if = parsed.ast().expressions().iter().any(|(_, node)| {
        contains(body_span, node.span())
            && matches!(node.payload(), Expression::If { .. })
            && !parsed.ast().expressions().iter().any(|(_, enclosing)| {
                matches!(enclosing.payload(), Expression::Lambda { .. })
                    && contains(enclosing.span(), node.span())
            })
    });
    let nesting = nested::relationship(parsed, typed, owned, plan.descriptor().statement())?;
    if !has_if && nesting.is_none() {
        return Ok(None);
    }
    // Mixed non-Map loops and non-ordinary control families retain their existing gate.
    if parsed.ast().statements().iter().any(|(_, node)| {
        contains(body_span, node.span())
            && matches!(
                node.payload(),
                Statement::While { .. } | Statement::Loop { .. }
            )
    }) || parsed.ast().expressions().iter().any(|(_, node)| {
        contains(body_span, node.span())
            && matches!(
                node.payload(),
                Expression::When { .. } | Expression::Lambda { .. }
            )
    }) {
        return Err(unsupported());
    }
    let statement = plan.descriptor().statement();
    let shape = QualifiedConditional {
        statement,
        body: UnitStatementId::new(statement.source_unit(), body),
        source,
    };
    let (facts, origin) = if plan.source_access() == UnitIterationSourceAccess::Shared {
        (Vec::new(), span)
    } else {
        source_facts(parsed, names, typed, owned, plan, source, span)?
    };
    let owns_source = matches!(
        plan.source_access(),
        UnitIterationSourceAccess::Owned | UnitIterationSourceAccess::Temporary
    );
    if owns_source && facts.is_empty() {
        // A validated source transferred after this provider can have no Drop.
        // Its later lifetime requires a separate capability, not a missing fact.
        return Err(if matches!(source, SourceIdentity::Temporary(_)) {
            bad()
        } else {
            unsupported()
        });
    }
    if plan.source_access() == UnitIterationSourceAccess::Shared && !facts.is_empty() {
        return Err(bad());
    }
    if facts.iter().any(|fact| fact.value_origin() != origin)
        || facts
            .iter()
            .enumerate()
            .any(|(index, fact)| facts[..index].contains(fact))
    {
        return Err(bad());
    }
    let mut exhaustion = 0;
    for (index, exit) in plan.exits().iter().enumerate() {
        if plan.exits()[..index]
            .iter()
            .any(|previous| previous.point() == exit.point())
        {
            return Err(bad());
        }
        let point = match exit.kind() {
            UnitIterationExitKind::Fallthrough => UnitDropPoint::AfterStatement(shape.body),
            UnitIterationExitKind::Exhaustion => {
                exhaustion += 1;
                UnitDropPoint::LoopExit(statement)
            }
            UnitIterationExitKind::Continue(expression)
            | UnitIterationExitKind::Break(expression)
            | UnitIterationExitKind::Return(expression) => {
                if expression.source_unit() != statement.source_unit() {
                    return Err(bad());
                }
                let node = parsed
                    .ast()
                    .expressions()
                    .get(expression.expression())
                    .map_err(|_| bad())?;
                let matches = match exit.kind() {
                    UnitIterationExitKind::Continue(_) => {
                        matches!(node.payload(), Expression::Continue { .. })
                    }
                    UnitIterationExitKind::Break(_) => {
                        matches!(node.payload(), Expression::Break { .. })
                    }
                    UnitIterationExitKind::Return(_) => {
                        matches!(node.payload(), Expression::Return { .. })
                    }
                    _ => false,
                };
                if !matches || !contains(body_span, node.span()) {
                    return Err(bad());
                }
                UnitDropPoint::ControlTransfer(expression)
            }
        };
        if exit.point() != point {
            return Err(bad());
        }
        cleanup::validate_exit(parsed, owned, plan, shape, exit, &facts, span)?;
    }
    if exhaustion != 1 {
        return Err(bad());
    }
    let outside = facts
        .iter()
        .copied()
        .filter_map(|fact| {
            let Some(exit) = plan
                .exits()
                .iter()
                .find(|exit| exit.point() == fact.point())
            else {
                return Some(Ok(fact));
            };
            match cleanup::source_drop(owned, statement, exit) {
                Ok(Some(selected)) if selected == fact => None,
                Ok(_) => match cleanup::retained_by_ancestor(parsed, owned, statement, fact) {
                    Ok(true) => None,
                    Ok(false) => Some(Err(bad())),
                    Err(error) => Some(Err(error)),
                },
                Err(error) => Some(Err(error)),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    if outside.len() > 1 {
        return Err(unsupported());
    }
    let late_drop = outside.first().copied();
    if let Some(fact) = late_drop {
        if matches!(source, SourceIdentity::Temporary(_)) {
            return Err(bad());
        }
        let point = crate::ssa::map_source::unit_drop_event(parsed, fact.point())
            .map(|event| event.span)
            .ok_or_else(bad)?;
        let retained = cleanup::retained_by_ancestor(parsed, owned, statement, fact)?;
        if point.source_id() != span.source_id()
            || (!retained && point.end() < span.end())
            || contains(body_span, point)
        {
            return Err(bad());
        }
        let scope = parsed
            .ast()
            .items()
            .iter()
            .filter_map(|(_, node)| {
                let Item::Function { form, .. } = node.payload() else {
                    return None;
                };
                let body = match form {
                    lang_frontend::parser::FunctionForm::ImplicitUnitBlock(body)
                    | lang_frontend::parser::FunctionForm::Explicit {
                        body: lang_frontend::parser::FunctionBody::Block(body),
                        ..
                    } => *body,
                    _ => return None,
                };
                parsed
                    .ast()
                    .statements()
                    .get(body)
                    .ok()
                    .filter(|node| contains(node.span(), span))
                    .map(|node| (body, node.span()))
            })
            .min_by_key(|(_, at)| at.end() - at.start())
            .ok_or_else(bad)?;
        if !contains(scope.1, point)
            || (!retained
                && point.start() < span.end()
                && fact.point()
                    != UnitDropPoint::AfterStatement(UnitStatementId::new(
                        statement.source_unit(),
                        scope.0,
                    )))
        {
            return Err(bad());
        }
        let resource = typed
            .is_resource_type(plan.descriptor().source_type())
            .ok_or_else(bad)?;
        if !resource && !retained {
            let root = match source {
                SourceIdentity::Parameter { root, .. } | SourceIdentity::Formed { root, .. } => {
                    root
                }
                _ => return Err(bad()),
            };
            if !crate::ssa::map_source::future_reference(
                parsed,
                span,
                point,
                names.names().references().iter().filter_map(|r| {
                    (r.target() == &UnitReferenceTarget::Symbol(root)).then_some(r.span())
                }),
            ) {
                return Err(unsupported());
            }
        }
    }
    if owns_source {
        for exit in plan.exits() {
            let mut retained = false;
            for fact in facts
                .iter()
                .copied()
                .filter(|fact| fact.point() == exit.point())
            {
                retained |= cleanup::retained_by_ancestor(parsed, owned, statement, fact)?;
            }
            if matches!(
                exit.kind(),
                UnitIterationExitKind::Break(_)
                    | UnitIterationExitKind::Return(_)
                    | UnitIterationExitKind::Exhaustion
            ) && shape.drop_at(owned, source, exit.point())?.is_none()
                && !retained
                && (late_drop.is_none() || matches!(exit.kind(), UnitIterationExitKind::Return(_)))
            {
                return Err(bad());
            }
        }
    }
    Ok(Some(QualifiedMap {
        statement,
        source,
        source_drop: shape.drop_at(owned, source, UnitDropPoint::LoopExit(statement))?,
        late_drop,
        transfer_drop: None,
        // A marker for the ordinary body continuation, never a claim that this edge exists.
        body_exit: UnitIterationExitKind::Fallthrough,
        body_point: UnitDropPoint::AfterStatement(shape.body),
        conditional: Some(shape),
        nested: nesting,
    }))
}

fn source_facts(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    plan: &UnitMapIterationOwnershipPlan,
    source: SourceIdentity,
    span: Span,
) -> Result<(Vec<UnitDropFact>, Span), LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    if let SourceIdentity::Temporary(expression) = source {
        let origin = parsed
            .ast()
            .expressions()
            .get(expression.expression())
            .map_err(|_| bad())?
            .span();
        return Ok((
            owned
                .drops()
                .iter()
                .copied()
                .filter(|fact| fact.target() == UnitDropTarget::Temporary(expression))
                .collect(),
            origin,
        ));
    }
    let (root, producer) = match source {
        SourceIdentity::Parameter { root, .. } => (root, None),
        SourceIdentity::Formed { root, producer } => (root, Some(producer)),
        _ => return Err(bad()),
    };
    if let Some(producer) = producer {
        let formation = formed::formation(
            parsed,
            names,
            typed,
            owned,
            root,
            plan.descriptor().source_type(),
            span,
        )?;
        if producer.source_unit() != root.source_unit()
            || producer.expression() != formation.producer
        {
            return Err(bad());
        }
        return Ok((
            formed::drops(parsed, owned, root, &formation)?,
            formation.origin,
        ));
    }
    let origin = names
        .names()
        .source_units()
        .get(root.source_unit().index())
        .and_then(|unit| unit.resolution().symbols().get(root.symbol().index()))
        .ok_or_else(bad)?
        .span();
    Ok((
        owned
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.target() == UnitDropTarget::Named(root))
            .collect(),
        origin,
    ))
}
