//! A separate, source-qualified capability for three terminal no-else If statements.
//! Unit ownership publishes plans and Drops, not the single-file condition/phi tables.
use super::*;
use lang_frontend::{parser::BinaryOperator, type_checking::Copyability};

pub(super) fn qualify_body(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    descriptor: &UnitMapIterationDescriptor,
    body: StatementId,
    span: Span,
) -> Result<Option<QualifiedConditional>, LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let node = parsed.ast().statements().get(body).map_err(|_| bad())?;
    let Statement::Block { elements } = node.payload() else {
        return Ok(None);
    };
    let mut branches = Vec::new();
    for (index, statement) in elements.iter().enumerate() {
        let node = parsed
            .ast()
            .statements()
            .get(*statement)
            .map_err(|_| bad())?;
        let Statement::Expression { expression } = node.payload() else {
            continue;
        };
        let node = parsed
            .ast()
            .expressions()
            .get(*expression)
            .map_err(|_| bad())?;
        if let Expression::If {
            condition,
            then_branch,
            else_branch,
            else_span,
            ..
        } = node.payload()
        {
            if else_branch.is_some() || else_span.is_some() {
                return Err(unsupported());
            }
            branches.push((index, *expression, *condition, *then_branch));
        }
    }
    if branches.is_empty() {
        return Ok(None);
    }
    let [first, second, third] = branches.as_slice() else {
        return Err(unsupported());
    };
    if second.0 != first.0 + 1
        || third.0 != second.0 + 1
        || builtin_type(typed, descriptor.key_type()) != Some(BuiltinType::String)
        || builtin_type(typed, descriptor.value_type()) != Some(BuiltinType::String)
    {
        return Err(unsupported());
    }
    let mut controls = Vec::new();
    let mut transfers = Vec::new();
    for (_, expression, condition, branch) in &branches {
        let control = UnitExpressionId::new(descriptor.statement().source_unit(), *expression);
        let ty = typed.expression_type(control).ok_or_else(bad)?;
        if builtin_type(typed, ty) != Some(BuiltinType::Unit) {
            return Err(unsupported());
        }
        qualify_condition(parsed, names, typed, descriptor, *condition, span)?;
        let node = parsed.ast().statements().get(*branch).map_err(|_| bad())?;
        let Statement::ControlBody { elements } = node.payload() else {
            return Err(unsupported());
        };
        let Some((last, prefix)) = elements.split_last() else {
            return Err(unsupported());
        };
        for statement in prefix {
            let node = parsed
                .ast()
                .statements()
                .get(*statement)
                .map_err(|_| bad())?;
            let Statement::Expression { expression } = node.payload() else {
                return Err(unsupported());
            };
            straight_expression(parsed, *expression, span)?;
        }
        let node = parsed.ast().statements().get(*last).map_err(|_| bad())?;
        let Statement::Expression { expression } = node.payload() else {
            return Err(unsupported());
        };
        let node = parsed
            .ast()
            .expressions()
            .get(*expression)
            .map_err(|_| bad())?;
        let id = UnitExpressionId::new(descriptor.statement().source_unit(), *expression);
        let kind = match (transfers.len(), node.payload()) {
            (0, Expression::Continue { .. }) => UnitIterationExitKind::Continue(id),
            (1, Expression::Break { .. }) => UnitIterationExitKind::Break(id),
            (
                2,
                Expression::Return {
                    value: Some(value), ..
                },
            ) => {
                qualify_return_operand(parsed, names, typed, descriptor, *value, span)?;
                UnitIterationExitKind::Return(id)
            }
            _ => return Err(unsupported()),
        };
        controls.push(control);
        transfers.push(kind);
    }
    for (index, statement) in elements.iter().enumerate() {
        if !(first.0..=third.0).contains(&index) {
            straight_statement(parsed, *statement, span)?;
            let node = parsed
                .ast()
                .statements()
                .get(*statement)
                .map_err(|_| bad())?;
            if !matches!(node.payload(), Statement::Expression { .. })
                && !(index < first.0 && matches!(node.payload(), Statement::LocalVariable { .. }))
            {
                return Err(unsupported());
            }
        }
    }
    let controls = controls.try_into().map_err(|_| bad())?;
    let transfers = transfers.try_into().map_err(|_| bad())?;
    Ok(Some(QualifiedConditional {
        controls,
        transfers,
        drops: [None; 3],
        local: None,
    }))
}

fn qualify_condition(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    descriptor: &UnitMapIterationDescriptor,
    condition: ExpressionId,
    span: Span,
) -> Result<(), LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let source = descriptor.statement().source_unit();
    let node = parsed
        .ast()
        .expressions()
        .get(condition)
        .map_err(|_| bad())?;
    let Expression::Binary {
        left,
        operator: BinaryOperator::Equal,
        right,
        ..
    } = node.payload()
    else {
        return Err(unsupported());
    };
    let left_node = parsed.ast().expressions().get(*left).map_err(|_| bad())?;
    let right_node = parsed.ast().expressions().get(*right).map_err(|_| bad())?;
    if !matches!(left_node.payload(), Expression::Name)
        || !matches!(right_node.payload(), Expression::String { parts }
            if parts.iter().all(|part| matches!(part, StringPart::Text(_))))
    {
        return Err(unsupported());
    }
    let references = names
        .names()
        .references()
        .iter()
        .filter(|reference| {
            reference.source_unit() == source
                && reference.namespace() == Some(Namespace::Value)
                && reference.span() == left_node.span()
        })
        .collect::<Vec<_>>();
    let [reference] = references.as_slice() else {
        return Err(bad());
    };
    if reference.target() != &UnitReferenceTarget::Symbol(descriptor.key_binding()) {
        return Err(unsupported());
    }
    for (expression, expected) in [
        (condition, BuiltinType::Boolean),
        (*left, BuiltinType::String),
        (*right, BuiltinType::String),
    ] {
        let ty = typed
            .expression_type(UnitExpressionId::new(source, expression))
            .ok_or_else(bad)?;
        if builtin_type(typed, ty) != Some(expected) {
            return Err(bad());
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_cleanup(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    plan: &UnitMapIterationOwnershipPlan,
    owned: &CompilationUnitOwnership,
    body: StatementId,
    root: UnitSymbolId,
    origin: Span,
    conditional: &mut QualifiedConditional,
    span: Span,
) -> Result<Option<UnitDropFact>, LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let descriptor = plan.descriptor();
    let statement = descriptor.statement();
    let kinds = [
        conditional.transfers[0],
        conditional.transfers[1],
        conditional.transfers[2],
        UnitIterationExitKind::Fallthrough,
        UnitIterationExitKind::Exhaustion,
    ];
    let points = kinds.map(|kind| {
        if kind == UnitIterationExitKind::Exhaustion {
            UnitDropPoint::LoopExit(statement)
        } else {
            body_point(statement, body, kind)
        }
    });
    if plan.exits().len() != kinds.len() {
        return Err(unsupported());
    }
    for (index, exit) in plan.exits().iter().enumerate() {
        if exit.kind() != kinds[index]
            || exit.point() != points[index]
            || owned.iteration_cleanup_at(exit.point()) != Some(exit.actions())
        {
            return Err(bad());
        }
    }
    let source_drops = owned
        .drops()
        .iter()
        .copied()
        .filter(|fact| fact.target() == UnitDropTarget::Named(root))
        .collect::<Vec<_>>();
    let mut drops = [None; 5];
    match plan.source_access() {
        UnitIterationSourceAccess::Owned => {
            if source_drops
                .iter()
                .any(|fact| fact.value_origin() != origin)
            {
                return Err(bad());
            }
            if source_drops.len() != 3
                || source_drops
                    .iter()
                    .any(|fact| ![points[1], points[2], points[4]].contains(&fact.point()))
            {
                return Err(unsupported());
            }
            for index in [1, 2, 4] {
                let facts = source_drops
                    .iter()
                    .copied()
                    .filter(|fact| fact.point() == points[index])
                    .collect::<Vec<_>>();
                let [fact] = facts.as_slice() else {
                    return Err(bad());
                };
                drops[index] = Some(*fact);
            }
        }
        UnitIterationSourceAccess::Shared if source_drops.is_empty() => {}
        UnitIterationSourceAccess::Shared => return Err(bad()),
        _ => return Err(unsupported()),
    }
    let locals = local_drops(
        parsed,
        names,
        typed,
        owned,
        descriptor,
        body,
        conditional,
        &points,
        span,
    )?;
    for (index, exit) in plan.exits().iter().enumerate() {
        let mut actions = locals[index]
            .iter()
            .copied()
            .map(Action::Drop)
            .collect::<Vec<_>>();
        if index < 4 {
            actions.extend([
                Action::EndBinding {
                    statement,
                    symbol: descriptor.value_binding(),
                },
                Action::EndBinding {
                    statement,
                    symbol: descriptor.key_binding(),
                },
            ]);
        }
        if [1, 2, 4].contains(&index) {
            actions.extend([
                Action::FinishProvider(statement),
                Action::EndSource(statement),
            ]);
            if let Some(drop) = drops[index] {
                actions.push(Action::Drop(drop));
            }
        }
        if exit.actions() != actions {
            return Err(bad());
        }
    }
    conditional.drops.copy_from_slice(&drops[..3]);
    Ok(drops[4])
}

#[allow(clippy::too_many_arguments)]
fn local_drops(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    descriptor: &UnitMapIterationDescriptor,
    body: StatementId,
    conditional: &mut QualifiedConditional,
    points: &[UnitDropPoint; 5],
    span: Span,
) -> Result<[Vec<UnitDropFact>; 5], LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let source = descriptor.statement().source_unit();
    let first_span = parsed
        .ast()
        .expressions()
        .get(conditional.controls[0].expression())
        .map_err(|_| bad())?
        .span();
    let node = parsed.ast().statements().get(body).map_err(|_| bad())?;
    let Statement::Block { elements } = node.payload() else {
        return Err(bad());
    };
    let mut declarations = Vec::new();
    collect_locals(parsed, elements, &mut declarations, span)?;
    if declarations.len() != 1 {
        return Err(unsupported());
    }
    let mut result: [Vec<UnitDropFact>; 5] = Default::default();
    for declaration in declarations.into_iter().rev() {
        let (_, item, _) = unwrap_modified(parsed, declaration)?;
        let Item::Variable {
            name, initializer, ..
        } = item
        else {
            return Err(bad());
        };
        let name_span = present_name(name).ok_or_else(bad)?;
        let symbols = names
            .names()
            .source_units()
            .get(source.index())
            .ok_or_else(bad)?
            .resolution()
            .symbols()
            .iter()
            .filter(|symbol| symbol.span() == name_span && symbol.kind() == SymbolKind::Variable)
            .collect::<Vec<_>>();
        let [symbol] = symbols.as_slice() else {
            return Err(bad());
        };
        let symbol = typed
            .body_symbol_types()
            .keys()
            .copied()
            .find(|candidate| {
                candidate.source_unit() == source && candidate.symbol() == symbol.id()
            })
            .ok_or_else(bad)?;
        let expression = UnitExpressionId::new(source, initializer);
        if conditional.local.replace((symbol, expression)).is_some() {
            return Err(bad());
        }
        let ty = typed.symbol_type(symbol).ok_or_else(bad)?;
        if typed.expression_type(expression) != Some(ty) {
            return Err(bad());
        }
        let facts = owned
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.target() == UnitDropTarget::Named(symbol))
            .collect::<Vec<_>>();
        if typed.copyability(ty) != Copyability::MoveOnly {
            return Err(unsupported());
        }
        match typed.is_resource_type(ty) {
            Some(true) => {}
            Some(false) => return Err(unsupported()),
            None => return Err(bad()),
        }
        let construction = typed.construction(expression).ok_or_else(unsupported)?;
        let plans = owned
            .construction_plans()
            .iter()
            .filter(|plan| plan.construction() == expression)
            .collect::<Vec<_>>();
        let [plan] = plans.as_slice() else {
            return Err(bad());
        };
        let obligation = plan.root_obligation().ok_or_else(bad)?;
        if construction.expression() != expression
            || construction.result_type() != ty
            || plan.target() != construction.target()
            || plan.terminating_operand().is_some()
            || obligation.construction() != expression
            || obligation.result_type() != ty
            || plan.deliveries().len() != construction.arguments().len()
        {
            return Err(bad());
        }
        if name_span.start() >= first_span.start() {
            return Err(unsupported());
        }
        let active = [0, 1, 2, 3];
        if facts.len() != active.len() {
            return Err(unsupported());
        }
        for index in active {
            let matches = facts
                .iter()
                .copied()
                .filter(|fact| fact.point() == points[index])
                .collect::<Vec<_>>();
            let [fact] = matches.as_slice() else {
                return Err(bad());
            };
            if fact.value_origin() != name_span {
                return Err(bad());
            }
            result[index].push(*fact);
        }
    }
    Ok(result)
}

fn collect_locals(
    parsed: &ParsedFile,
    statements: &[StatementId],
    declarations: &mut Vec<ItemId>,
    span: Span,
) -> Result<(), LoweringError> {
    for statement in statements {
        let node = parsed
            .ast()
            .statements()
            .get(*statement)
            .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
        match node.payload() {
            Statement::LocalVariable { declaration } => declarations.push(*declaration),
            Statement::Block { elements } => collect_locals(parsed, elements, declarations, span)?,
            _ => {}
        }
    }
    Ok(())
}
