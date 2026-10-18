//! Unit-only qualification: actual declarations, construction obligations and complete points.
use super::*;
use lang_frontend::{
    parser::VariableKind,
    type_checking::{Copyability, NominalKind, UnitConstructionTarget},
};

pub(super) fn shape(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    pair: [UnitStatementId; 2],
    span: Span,
) -> Result<Option<QualifiedNestedTail>, LoweringError> {
    let source = pair[0].source_unit();
    let [outer, inner] = pair.map(UnitStatementId::statement);
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let outer_body = elements(parsed, outer, span)?;
    let inner_body = elements(parsed, inner, span)?;
    let Some((&last, prefix)) = inner_body.split_last() else {
        return Ok(None);
    };
    let node = parsed.ast().statements().get(last).map_err(|_| bad())?;
    let Statement::Expression { expression } = node.payload() else {
        return Ok(None);
    };
    let id = UnitExpressionId::new(source, *expression);
    let node = parsed
        .ast()
        .expressions()
        .get(*expression)
        .map_err(|_| bad())?;
    let (kind, operand) = match node.payload() {
        Expression::Continue { .. } => (UnitIterationExitKind::Continue(id), None),
        Expression::Break { .. } => (UnitIterationExitKind::Break(id), None),
        Expression::Return {
            value: Some(value), ..
        } => (
            UnitIterationExitKind::Return(id),
            Some(UnitExpressionId::new(source, *value)),
        ),
        _ => return Ok(None),
    };
    let (&outer_local, outer_rest) = outer_body.split_first().ok_or_else(unsupported)?;
    let (&inner_local, inner_rest) = prefix.split_first().ok_or_else(unsupported)?;
    let guards = [
        guard(parsed, names, typed, owned, source, outer_local, span)?,
        guard(parsed, names, typed, owned, source, inner_local, span)?,
    ];
    if guards[0].symbol == guards[1].symbol || guards[0].initializer == guards[1].initializer {
        return Err(bad());
    }
    for child in outer_rest {
        if *child != inner {
            straight_body(parsed, *child, span)?;
        }
    }
    for child in inner_rest {
        straight_body(parsed, *child, span)?;
    }
    // No alias, assignment, capture, move or call use may consume either guard.
    if names.names().references().iter().any(|reference| {
        guards
            .iter()
            .any(|guard| reference.target() == &UnitReferenceTarget::Symbol(guard.symbol))
    }) {
        return Err(unsupported());
    }
    let outer_span = parsed
        .ast()
        .statements()
        .get(outer)
        .map_err(|_| bad())?
        .span();
    for (id, node) in parsed.ast().expressions().iter() {
        let id = UnitExpressionId::new(source, id);
        if contains(outer_span, node.span())
            && matches!(node.payload(), Expression::Call { .. })
            && !guards.iter().any(|guard| guard.initializer == id)
        {
            let ty = typed.expression_type(id).ok_or_else(bad)?;
            match typed.is_resource_type(ty) {
                Some(true) => return Err(unsupported()),
                Some(false) => {}
                None => return Err(bad()),
            }
        }
    }
    if let Some(operand) = operand {
        let outer = typed
            .map_iteration(UnitStatementId::new(source, outer))
            .ok_or_else(unsupported)?;
        let inner = typed
            .map_iteration(UnitStatementId::new(source, inner))
            .ok_or_else(unsupported)?;
        operand::validate(parsed, names, typed, owned, [outer, inner], operand, span)?;
    }
    Ok(Some(QualifiedNestedTail {
        kind,
        guards,
        operand,
    }))
}

fn elements(
    parsed: &ParsedFile,
    statement: StatementId,
    span: Span,
) -> Result<&[StatementId], LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let node = parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|_| bad())?;
    let Statement::For { body, .. } = node.payload() else {
        return Err(bad());
    };
    let node = parsed.ast().statements().get(*body).map_err(|_| bad())?;
    let Statement::Block { elements } = node.payload() else {
        return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
    };
    Ok(elements)
}

pub(in super::super) fn guard(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    source: SourceUnitId,
    statement: StatementId,
    span: Span,
) -> Result<QualifiedGuard, LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let node = parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|_| bad())?;
    let Statement::LocalVariable { declaration } = node.payload() else {
        return Err(unsupported());
    };
    let (_, item, _) = unwrap_modified(parsed, *declaration)?;
    let Item::Variable {
        kind: VariableKind::Val,
        name,
        initializer,
        ..
    } = item
    else {
        return Err(unsupported());
    };
    let origin = present_name(name).ok_or_else(bad)?;
    let symbols = names
        .names()
        .source_units()
        .get(source.index())
        .ok_or_else(bad)?
        .resolution()
        .symbols()
        .iter()
        .filter(|symbol| symbol.span() == origin && symbol.kind() == SymbolKind::Variable)
        .collect::<Vec<_>>();
    let [symbol] = symbols.as_slice() else {
        return Err(bad());
    };
    let symbol = typed
        .body_symbol_types()
        .keys()
        .copied()
        .find(|candidate| candidate.source_unit() == source && candidate.symbol() == symbol.id())
        .ok_or_else(bad)?;
    let initializer = UnitExpressionId::new(source, initializer);
    let ty = typed.symbol_type(symbol).ok_or_else(bad)?;
    // Ordinary local owners are not entries in the public parameter/iteration binding table.
    if typed.expression_type(initializer) != Some(ty) {
        return Err(bad());
    }
    if typed.copyability(ty) != Copyability::MoveOnly {
        return Err(unsupported());
    }
    match typed.is_resource_type(ty) {
        Some(true) => {}
        Some(false) => return Err(unsupported()),
        None => return Err(bad()),
    }
    let init = parsed
        .ast()
        .expressions()
        .get(initializer.expression())
        .map_err(|_| bad())?;
    let Expression::Call {
        callee,
        arguments,
        type_arguments,
        ..
    } = init.payload()
    else {
        return Err(unsupported());
    };
    if !arguments.is_empty() || !type_arguments.is_empty() {
        return Err(unsupported());
    }
    let callee = parsed.ast().expressions().get(*callee).map_err(|_| bad())?;
    if callee.payload() != &Expression::Name {
        return Err(unsupported());
    }
    let construction = typed.construction(initializer).ok_or_else(bad)?;
    let UnitConstructionTarget::Nominal(declaration) = construction.target() else {
        return Err(unsupported());
    };
    let nominal = typed
        .signatures()
        .declaration(declaration)
        .and_then(|item| item.nominal())
        .ok_or_else(bad)?;
    if nominal.kind() != NominalKind::Class
        || !nominal.fields().is_empty()
        || !nominal.type_parameters().is_empty()
        || !construction.arguments().is_empty()
    {
        return Err(unsupported());
    }
    let Some(UnitTypeKind::Nominal {
        declaration: actual,
        arguments,
    }) = typed.types().get(ty)
    else {
        return Err(bad());
    };
    if *actual != declaration
        || !arguments.is_empty()
        || !construction.instance().type_arguments().is_empty()
        || !owned.formed_owned_locals().contains(&symbol)
    {
        return Err(bad());
    }
    let references = names
        .names()
        .references()
        .iter()
        .filter(|reference| {
            reference.source_unit() == source
                && reference.namespace() == Some(Namespace::Type)
                && reference.span() == callee.span()
        })
        .collect::<Vec<_>>();
    let [reference] = references.as_slice() else {
        return Err(bad());
    };
    if !matches!(reference.target(), UnitReferenceTarget::Declaration(actual) if *actual == declaration)
        && !matches!(reference.target(), UnitReferenceTarget::Symbol(actual) if *actual == nominal.symbol())
    {
        return Err(bad());
    }
    let deinit = nominal.deinit().ok_or_else(bad)?;
    // This finite slice has the Guard declaration and body in the same source file.
    if deinit.item().source_unit() != source
        || deinit.body().source_unit() != source
        || nominal.symbol().source_unit() != source
    {
        return Err(unsupported());
    }
    let deinit_node = parsed
        .ast()
        .items()
        .get(deinit.item().item())
        .map_err(|_| bad())?;
    if !matches!(deinit_node.payload(), Item::Deinit { body, .. } if *body == deinit.body().statement())
        || !matches!(
            parsed
                .ast()
                .statements()
                .get(deinit.body().statement())
                .map_err(|_| bad())?
                .payload(),
            Statement::Block { .. }
        )
    {
        return Err(bad());
    }
    let nominal_symbol = names
        .names()
        .source_units()
        .get(source.index())
        .ok_or_else(bad)?
        .resolution()
        .symbols()
        .get(nominal.symbol().symbol().index())
        .ok_or_else(bad)?;
    let declarations = parsed
        .ast()
        .items()
        .iter()
        .filter_map(|(_, item)| match item.payload() {
            Item::Classifier(classifier)
                if classifier.name == NameMarker::Present(nominal_symbol.span()) =>
            {
                Some(classifier)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [classifier] = declarations.as_slice() else {
        return Err(bad());
    };
    if !classifier
        .body
        .as_ref()
        .is_some_and(|body| body.members.contains(&deinit.item().item()))
    {
        return Err(bad());
    }
    if nominal.ty() != ty
        || deinit.owner() != declaration
        || deinit.receiver_type() != ty
        || deinit.receiver_mode() != ParameterMode::Borrow
    {
        return Err(bad());
    }
    let plans = owned
        .construction_plans()
        .iter()
        .filter(|plan| plan.construction() == initializer)
        .collect::<Vec<_>>();
    let [plan] = plans.as_slice() else {
        return Err(bad());
    };
    let root = plan.root_obligation().ok_or_else(bad)?;
    if construction.expression() != initializer
        || construction.result_type() != ty
        || plan.target() != construction.target()
        || plan.terminating_operand().is_some()
        || !plan.deliveries().is_empty()
        || root.construction() != initializer
        || root.result_type() != ty
        || root.kind() != lang_frontend::ownership_checking::ConstructionRootKind::HeapOwner
    {
        return Err(bad());
    }
    Ok(QualifiedGuard {
        symbol,
        initializer,
        origin,
    })
}

pub(in super::super) fn validate_cleanup(
    parsed: &ParsedFile,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    plan: &UnitMapIterationOwnershipPlan,
    pair: QualifiedNested,
    root: UnitSymbolId,
    span: Span,
) -> Result<(), LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let tail = pair.tail.ok_or_else(bad)?;
    let outer = typed.map_iteration(pair.outer).ok_or_else(bad)?;
    let inner = typed.map_iteration(pair.inner).ok_or_else(bad)?;
    let point = match tail.kind {
        UnitIterationExitKind::Continue(id)
        | UnitIterationExitKind::Break(id)
        | UnitIterationExitKind::Return(id) => UnitDropPoint::ControlTransfer(id),
        _ => return Err(bad()),
    };
    let outer_node = parsed
        .ast()
        .statements()
        .get(pair.outer.statement())
        .map_err(|_| bad())?;
    let Statement::For { body, .. } = outer_node.payload() else {
        return Err(bad());
    };
    let fallthrough = body_point(pair.outer, *body, UnitIterationExitKind::Fallthrough);
    let returning = matches!(tail.kind, UnitIterationExitKind::Return(_));
    let drops = |guard: QualifiedGuard,
                 points: &[UnitDropPoint]|
     -> Result<Vec<UnitDropFact>, LoweringError> {
        let facts = owned
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.target() == UnitDropTarget::Named(guard.symbol))
            .collect::<Vec<_>>();
        if facts.len() != points.len()
            || facts.iter().any(|fact| fact.value_origin() != guard.origin)
        {
            return Err(bad());
        }
        points
            .iter()
            .map(|point| {
                let at = facts
                    .iter()
                    .copied()
                    .filter(|fact| fact.point() == *point)
                    .collect::<Vec<_>>();
                match at.as_slice() {
                    [fact] => Ok(*fact),
                    _ => Err(bad()),
                }
            })
            .collect()
    };
    let outer_points = if returning {
        vec![fallthrough, point]
    } else {
        vec![fallthrough]
    };
    let outer_drops = drops(tail.guards[0], &outer_points)?;
    let inner_drop = drops(tail.guards[1], &[point])?[0];
    if owned
        .drops()
        .iter()
        .any(|fact| fact.target() == UnitDropTarget::Named(root))
        || plan.source_access() != UnitIterationSourceAccess::Shared
    {
        return Err(bad());
    }
    let bindings = |descriptor: &UnitMapIterationDescriptor| {
        [
            Action::EndBinding {
                statement: descriptor.statement(),
                symbol: descriptor.value_binding(),
            },
            Action::EndBinding {
                statement: descriptor.statement(),
                symbol: descriptor.key_binding(),
            },
        ]
    };
    let terminal = |statement| {
        [
            Action::FinishProvider(statement),
            Action::EndSource(statement),
        ]
    };
    let mut transfer = vec![Action::Drop(inner_drop)];
    transfer.extend(bindings(inner));
    if !matches!(tail.kind, UnitIterationExitKind::Continue(_)) {
        transfer.extend(terminal(pair.inner));
    }
    if returning {
        transfer.push(Action::Drop(outer_drops[1]));
        transfer.extend(bindings(outer));
        transfer.extend(terminal(pair.outer));
    }
    let mut outer_body = vec![Action::Drop(outer_drops[0])];
    outer_body.extend(bindings(outer));
    let statement = plan.descriptor().statement();
    let mut expected = if statement == pair.outer {
        vec![(UnitIterationExitKind::Fallthrough, fallthrough, outer_body)]
    } else if statement == pair.inner {
        vec![(tail.kind, point, transfer.clone())]
    } else {
        return Err(bad());
    };
    if returning && statement == pair.outer {
        expected.push((tail.kind, point, transfer));
    }
    expected.push((
        UnitIterationExitKind::Exhaustion,
        UnitDropPoint::LoopExit(statement),
        terminal(statement).to_vec(),
    ));
    if plan.exits().len() != expected.len() {
        return Err(bad());
    }
    for (kind, point, actions) in expected {
        let exits = plan
            .exits()
            .iter()
            .filter(|exit| exit.kind() == kind && exit.point() == point)
            .collect::<Vec<_>>();
        let [exit] = exits.as_slice() else {
            return Err(bad());
        };
        if exit.actions() != actions || owned.iteration_cleanup_at(point) != Some(exit.actions()) {
            return Err(bad());
        }
    }
    Ok(())
}
