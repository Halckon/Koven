//! Unit temporary qualification uses only source-qualified descriptor, plan and Drop facts.
mod cleanup;
use super::*;
use lang_frontend::ownership_checking::UnitClosureCaptureSource;

pub(super) fn qualify(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    descriptor: &UnitMapIterationDescriptor,
    body: StatementId,
    span: Span,
) -> Result<QualifiedMap, LoweringError> {
    let missing = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let statement = descriptor.statement();
    let source = UnitExpressionId::new(
        statement.source_unit(),
        crate::ssa::map_source::transparent_expression(
            parsed,
            descriptor.source().expression(),
            |id| {
                let id = UnitExpressionId::new(statement.source_unit(), id);
                typed.expression_type(id).zip(typed.expression_category(id))
            },
        )?,
    );
    let source_node = parsed
        .ast()
        .expressions()
        .get(source.expression())
        .map_err(|_| missing())?;
    // Formation follows the same ordinary selected Call as a bound local;
    // source-qualified temporary lifetime and cleanup remain separate below.
    local::factory(
        parsed,
        names,
        typed,
        owned,
        source,
        descriptor.source_type(),
        span,
    )?;
    if typed.copyability(descriptor.source_type()) != Copyability::MoveOnly {
        return Err(missing());
    }
    // Unlike named parameters, a factory result has no parameter-origin or unit phi table.
    let plan = owned.map_iteration(statement).ok_or_else(missing)?;
    if plan.descriptor() != descriptor
        || plan.source_access() != UnitIterationSourceAccess::Temporary
        || plan.source_kind() != &IterationSource::BorrowedSource(UnitLoanTarget::Temporary(source))
        || plan.source() != Some(&UnitLoanTarget::Temporary(source))
    {
        return Err(missing());
    }
    let bindings = descriptor.bindings();
    if bindings[0] == bindings[1]
        || bindings
            .iter()
            .any(|symbol| symbol.source_unit() != statement.source_unit())
        || typed.symbol_type(bindings[0]) != Some(descriptor.key_type())
        || typed.symbol_type(bindings[1]) != Some(descriptor.value_type())
    {
        return Err(missing());
    }
    for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
        .into_iter()
        .enumerate()
    {
        let binding = plan.bindings()[index];
        let loan = &plan.loans()[index];
        if binding.symbol() != bindings[index]
            || binding.kind() != OwnershipBindingKind::Shared
            || owned.binding_kind(bindings[index]) != Some(OwnershipBindingKind::Shared)
            || loan.statement() != statement
            || loan.source() != descriptor.source()
            || loan.component() != component
            || loan.binding() != bindings[index]
            || loan.origin() != &UnitLoanTarget::Temporary(source)
            || loan.kind() != lang_frontend::ownership_checking::LoanKind::Shared
        {
            return Err(missing());
        }
    }
    reject_nested(parsed, statement.statement(), span)?;
    if let Some(qualified) = conditional::qualify(
        parsed,
        names,
        typed,
        owned,
        plan,
        (body, SourceIdentity::Temporary(source)),
        span,
    )? {
        return Ok(qualified);
    }
    if let Some(qualified) = qualify_transfer(parsed, names, typed, owned, descriptor, body, span)?
    {
        return Ok(qualified);
    }
    straight_statement(parsed, body, span)?;
    let body_span = parsed
        .ast()
        .statements()
        .get(body)
        .map_err(|_| missing())?
        .span();
    for (&expression, &ty) in typed.expression_types() {
        if expression.source_unit() != statement.source_unit() {
            continue;
        }
        let node = parsed
            .ast()
            .expressions()
            .get(expression.expression())
            .map_err(|_| missing())?;
        if contains(body_span, node.span())
            && matches!(node.payload(), Expression::Call { .. })
            && builtin_type(typed, ty) == Some(BuiltinType::Nothing)
        {
            return Err(unsupported());
        }
    }
    if plan.exits().iter().any(|exit| {
        !matches!(
            exit.kind(),
            UnitIterationExitKind::Fallthrough | UnitIterationExitKind::Exhaustion
        )
    }) {
        return Err(unsupported());
    }
    let [body_plan, exhaustion] = plan.exits() else {
        return Err(missing());
    };
    let body_point =
        UnitDropPoint::AfterStatement(UnitStatementId::new(statement.source_unit(), body));
    let drops = owned
        .drops()
        .iter()
        .copied()
        .filter(|fact| fact.target() == UnitDropTarget::Temporary(source))
        .collect::<Vec<_>>();
    let [source_drop] = drops.as_slice() else {
        return Err(missing());
    };
    if source_drop.point() != UnitDropPoint::LoopExit(statement)
        || source_drop.value_origin() != source_node.span()
        || body_plan.kind() != UnitIterationExitKind::Fallthrough
        || body_plan.point() != body_point
        || exhaustion.kind() != UnitIterationExitKind::Exhaustion
        || exhaustion.point() != source_drop.point()
        || owned.iteration_cleanup_at(body_point) != Some(body_plan.actions())
        || owned.iteration_cleanup_at(exhaustion.point()) != Some(exhaustion.actions())
        || exhaustion.actions()
            != [
                Action::FinishProvider(statement),
                Action::EndSource(statement),
                Action::Drop(*source_drop),
            ]
    {
        return Err(missing());
    }
    let body_suffix = [
        Action::EndBinding {
            statement,
            symbol: descriptor.value_binding(),
        },
        Action::EndBinding {
            statement,
            symbol: descriptor.key_binding(),
        },
    ];
    if !body_plan.actions().ends_with(&body_suffix) {
        return Err(missing());
    }
    let prefix = &body_plan.actions()[..body_plan.actions().len() - body_suffix.len()];
    if prefix.iter().any(|action| {
        !matches!(action, Action::Drop(fact)
            if fact.target() != UnitDropTarget::Temporary(source)
                && fact.point() == body_point
                && owned.drops().contains(fact))
    }) {
        return Err(missing());
    }
    if !prefix.is_empty() {
        // These are valid owner lifetimes, but this slice proves only the two component ends.
        return Err(unsupported());
    }
    Ok(QualifiedMap {
        statement,
        future_transfer: None,
        source: SourceIdentity::Temporary(source),
        source_drop: Some(*source_drop),
        late_drop: None,
        transfer_drop: None,
        body_exit: UnitIterationExitKind::Fallthrough,
        body_point,
        conditional: None,
        nested: None,
    })
}

/// The normal path above keeps its original no-local-drop boundary.
fn qualify_transfer(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    descriptor: &UnitMapIterationDescriptor,
    body: StatementId,
    span: Span,
) -> Result<Option<QualifiedMap>, LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let node = parsed.ast().statements().get(body).map_err(|_| bad())?;
    let body_span = node.span();
    let Statement::Block { elements } = node.payload() else {
        return Ok(None);
    };
    let Some((&last, prefix)) = elements.split_last() else {
        return Ok(None);
    };
    let last = parsed.ast().statements().get(last).map_err(|_| bad())?;
    let Statement::Expression { expression } = last.payload() else {
        return Ok(None);
    };
    let tail = parsed
        .ast()
        .expressions()
        .get(*expression)
        .map_err(|_| bad())?;
    let operand = match tail.payload() {
        Expression::Continue { .. } | Expression::Break { .. } => None,
        Expression::Return { value, .. } => *value,
        _ => return Ok(None),
    };
    let (&local, ordinary) = prefix.split_first().ok_or_else(unsupported)?;
    let statement = descriptor.statement();
    let guard = nested::qualify_guard(
        parsed,
        names,
        typed,
        owned,
        statement.source_unit(),
        local,
        span,
    )?;
    if descriptor.bindings().contains(&guard.symbol)
        || guard.initializer == descriptor.source()
        || typed.expression_category(guard.initializer) != Some(ExpressionCategory::Temporary)
    {
        return Err(bad());
    }
    for child in ordinary {
        let node = parsed.ast().statements().get(*child).map_err(|_| bad())?;
        let Statement::Expression { expression } = node.payload() else {
            return Err(unsupported());
        };
        straight_expression(parsed, *expression, span)?;
    }
    // Constructor identity alone does not prove an unused, uncaptured local owner.
    if names
        .names()
        .references()
        .iter()
        .any(|reference| reference.target() == &UnitReferenceTarget::Symbol(guard.symbol))
        || owned
            .captures()
            .iter()
            .any(|capture| capture.source() == UnitClosureCaptureSource::Symbol(guard.symbol))
    {
        return Err(unsupported());
    }
    // Inspect only the body: the source factory itself is another Resource Call.
    for (id, node) in parsed.ast().expressions().iter() {
        if !contains(body_span, node.span()) || !matches!(node.payload(), Expression::Call { .. }) {
            continue;
        }
        let id = UnitExpressionId::new(statement.source_unit(), id);
        let ty = typed.expression_type(id).ok_or_else(bad)?;
        if builtin_type(typed, ty) == Some(BuiltinType::Nothing) {
            return Err(unsupported());
        }
        if id != guard.initializer
            && !operand.is_some_and(|operand| {
                parsed
                    .ast()
                    .expressions()
                    .get(operand)
                    .is_ok_and(|value| contains(value.span(), node.span()))
            })
        {
            match typed.is_resource_type(ty) {
                Some(true) => return Err(unsupported()),
                Some(false) => {}
                None => return Err(bad()),
            }
        }
    }
    let plan = owned.map_iteration(statement).ok_or_else(bad)?;
    let body_exit = qualify_body(parsed, names, plan, descriptor, body, span)?;
    let body_point = body_point(statement, body, body_exit);
    let IterationSource::BorrowedSource(UnitLoanTarget::Temporary(source)) = plan.source_kind()
    else {
        return Err(bad());
    };
    let source = *source;
    let source_origin = parsed
        .ast()
        .expressions()
        .get(source.expression())
        .map_err(|_| bad())?
        .span();
    let (source_drop, transfer_drop) = cleanup::qualify(
        owned,
        plan,
        body_point,
        body_exit,
        guard,
        source_origin,
        span,
    )?;
    Ok(Some(QualifiedMap {
        statement,
        future_transfer: None,
        source: SourceIdentity::Temporary(source),
        source_drop: Some(source_drop),
        late_drop: None,
        transfer_drop,
        body_exit,
        body_point,
        conditional: None,
        nested: None,
    }))
}
