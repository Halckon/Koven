//! Narrow Map capability gate. This runs after the sealed frontend view, before Module creation.
use super::*;
use lang_frontend::{
    ownership_checking::{
        IterationSource, MapIterationComponent, OwnershipBindingKind, UnitDropFact, UnitDropTarget,
        UnitIterationCleanupAction as Action, UnitIterationExitKind, UnitIterationSourceAccess,
        UnitLoanTarget, UnitMapIterationOwnershipPlan,
    },
    parser::{ParsedFile, StringPart},
    type_checking::{IntrinsicTypeConstructor, MapIterationKind, UnitMapIterationDescriptor},
};

#[derive(Clone, Copy)]
pub(super) struct QualifiedMap {
    pub(super) root: UnitSymbolId,
    pub(super) parameter_slot: usize,
    pub(super) source_drop: Option<UnitDropFact>,
}

pub(super) fn validate(
    parsed: &[&ParsedFile],
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
) -> Result<(), LoweringError> {
    let mut descriptors = typed.map_iterations().iter().collect::<Vec<_>>();
    descriptors.sort_by_key(|descriptor| descriptor.statement());
    for descriptor in descriptors {
        let parsed = parsed
            .get(descriptor.statement().source_unit().index())
            .ok_or(LoweringError {
                kind: LoweringErrorKind::MissingFact,
                span: None,
            })?;
        qualify(parsed, names, typed, owned, descriptor)?;
    }
    Ok(())
}

/// Only public UnitMapPlan/Drop facts participate. There is no unit closure-phi table.
pub(super) fn qualify(
    parsed: &ParsedFile,
    names: &ValidatedCompilationUnitNames,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    descriptor: &UnitMapIterationDescriptor,
) -> Result<QualifiedMap, LoweringError> {
    let statement = descriptor.statement();
    let node = parsed
        .ast()
        .statements()
        .get(statement.statement())
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
    let span = node.span();
    let missing = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let Statement::For { source, body, .. } = node.payload() else {
        return Err(missing());
    };
    if descriptor.source() != UnitExpressionId::new(statement.source_unit(), *source)
        || typed.map_iteration(statement) != Some(descriptor)
        || typed.expression_type(descriptor.source()) != Some(descriptor.source_type())
    {
        return Err(missing());
    }
    // Validated source-return/dead-tail rows legitimately have no AcquireProvider plan.
    let plan = owned.map_iteration(statement).ok_or_else(unsupported)?;
    if plan.descriptor() != descriptor {
        return Err(missing());
    }
    let constructor = match descriptor.kind() {
        MapIterationKind::Map => IntrinsicTypeConstructor::Map,
        MapIterationKind::MutableMap => IntrinsicTypeConstructor::MutableMap,
    };
    if !matches!(typed.types().get(descriptor.source_type()),
        Some(UnitTypeKind::Intrinsic { constructor: actual, arguments })
            if *actual == constructor && arguments.as_slice() == [descriptor.key_type(), descriptor.value_type()])
    {
        return Err(missing());
    }
    let source_node = parsed
        .ast()
        .expressions()
        .get(*source)
        .map_err(|_| missing())?;
    if !matches!(source_node.payload(), Expression::Name) {
        return Err(unsupported());
    }
    let IterationSource::BorrowedSource(UnitLoanTarget::Place(place)) = plan.source_kind() else {
        return Err(unsupported());
    };
    if !place.is_root() {
        return Err(unsupported());
    }
    let root = place.root();
    let references = names
        .names()
        .references()
        .iter()
        .filter(|reference| {
            reference.source_unit() == statement.source_unit()
                && reference.namespace() == Some(Namespace::Value)
                && reference.span() == source_node.span()
        })
        .collect::<Vec<_>>();
    if !matches!(references.as_slice(), [reference]
        if reference.target() == &UnitReferenceTarget::Symbol(root))
        || root.source_unit() != statement.source_unit()
        || typed.symbol_type(root) != Some(descriptor.source_type())
    {
        return Err(missing());
    }
    let symbol = names
        .names()
        .source_units()
        .get(root.source_unit().index())
        .and_then(|source| source.resolution().symbols().get(root.symbol().index()))
        .ok_or_else(missing)?;
    if symbol.kind() != SymbolKind::ValueParameter {
        return Err(unsupported());
    }
    let callables = typed
        .signatures()
        .declarations()
        .iter()
        .flat_map(|declaration| {
            declaration.callable().into_iter().chain(
                declaration.nominal().into_iter().flat_map(|nominal| {
                    nominal.members().iter().chain(nominal.companion_members())
                }),
            )
        });
    let parameters = callables
        .flat_map(|callable| {
            callable
                .parameters()
                .iter()
                .enumerate()
                .filter_map(move |(index, parameter)| {
                    (parameter.symbol() == Some(root)).then_some((
                        parameter,
                        index + usize::from(callable.receiver().is_some()),
                    ))
                })
        })
        .collect::<Vec<_>>();
    let [(parameter, parameter_slot)] = parameters.as_slice() else {
        return Err(missing());
    };
    if parameter.ty() != descriptor.source_type() {
        return Err(missing());
    }
    let binding = owned.binding_kind(root).ok_or_else(missing)?;
    let (expected_binding, expected_access) = match parameter.mode() {
        ParameterMode::Value => (
            OwnershipBindingKind::Owned,
            UnitIterationSourceAccess::Owned,
        ),
        ParameterMode::Borrow => (
            OwnershipBindingKind::Shared,
            UnitIterationSourceAccess::Shared,
        ),
        ParameterMode::Inout => (
            OwnershipBindingKind::Exclusive,
            UnitIterationSourceAccess::Exclusive,
        ),
    };
    if binding != expected_binding || plan.source_access() != expected_access {
        return Err(missing());
    }
    // Check whole supported rows, never a cross-product of independently accepted fields.
    let supported = matches!(
        (
            descriptor.kind(),
            parameter.mode(),
            binding,
            plan.source_access()
        ),
        (
            MapIterationKind::Map,
            ParameterMode::Value,
            OwnershipBindingKind::Owned,
            UnitIterationSourceAccess::Owned
        ) | (
            MapIterationKind::MutableMap,
            ParameterMode::Borrow,
            OwnershipBindingKind::Shared,
            UnitIterationSourceAccess::Shared
        )
    );
    if !supported {
        return Err(unsupported());
    }
    let bindings = descriptor.bindings();
    if bindings[0] == bindings[1]
        || bindings.contains(&root)
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
        let loan = plan.loans().get(index).ok_or_else(missing)?;
        if binding.symbol() != bindings[index]
            || binding.kind() != OwnershipBindingKind::Shared
            || loan.component() != component
            || loan.binding() != bindings[index]
            || loan.statement() != statement
            || loan.source() != descriptor.source()
            || Some(loan.origin()) != plan.source()
            || loan.kind() != lang_frontend::ownership_checking::LoanKind::Shared
        {
            return Err(missing());
        }
    }
    reject_nested(parsed, statement.statement(), span)?;
    straight_statement(parsed, *body, span)?;
    let body_span = parsed
        .ast()
        .statements()
        .get(*body)
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
    let source_drop = validate_cleanup(plan, owned, *body, root, symbol.span(), span)?;
    Ok(QualifiedMap {
        root,
        parameter_slot: *parameter_slot,
        source_drop,
    })
}

fn validate_cleanup(
    plan: &UnitMapIterationOwnershipPlan,
    owned: &CompilationUnitOwnership,
    body: StatementId,
    root: UnitSymbolId,
    origin: Span,
    span: Span,
) -> Result<Option<UnitDropFact>, LoweringError> {
    let missing = || lowering_error(LoweringErrorKind::MissingFact, span);
    let unsupported = || lowering_error(LoweringErrorKind::UnsupportedNode, span);
    let descriptor = plan.descriptor();
    let statement = descriptor.statement();
    if plan.exits().iter().any(|exit| {
        !matches!(
            exit.kind(),
            UnitIterationExitKind::Fallthrough | UnitIterationExitKind::Exhaustion
        )
    }) {
        return Err(unsupported());
    }
    let [fallthrough, exhaustion] = plan.exits() else {
        return Err(missing());
    };
    if fallthrough.kind() != UnitIterationExitKind::Fallthrough
        || fallthrough.point()
            != UnitDropPoint::AfterStatement(UnitStatementId::new(statement.source_unit(), body))
        || exhaustion.kind() != UnitIterationExitKind::Exhaustion
        || exhaustion.point() != UnitDropPoint::LoopExit(statement)
        || owned.iteration_cleanup_at(fallthrough.point()) != Some(fallthrough.actions())
        || owned.iteration_cleanup_at(exhaustion.point()) != Some(exhaustion.actions())
    {
        return Err(missing());
    }
    let expected = [
        Action::EndBinding {
            statement,
            symbol: descriptor.value_binding(),
        },
        Action::EndBinding {
            statement,
            symbol: descriptor.key_binding(),
        },
    ];
    if !fallthrough.actions().ends_with(&expected) {
        return Err(missing());
    }
    // Existing straight-line local drops may precede the two components; no other provider can.
    if fallthrough.actions()[..fallthrough.actions().len() - 2].iter().any(|action| {
        !matches!(action, Action::Drop(fact) if fact.target() != UnitDropTarget::Named(root))
    }) { return Err(unsupported()); }
    let drops = owned
        .drops()
        .iter()
        .copied()
        .filter(|fact| fact.target() == UnitDropTarget::Named(root))
        .collect::<Vec<_>>();
    match plan.source_access() {
        UnitIterationSourceAccess::Owned => {
            let [drop] = drops.as_slice() else {
                // Source delivery or a later lifetime boundary needs a separate owner proof.
                // The sealed view has already rejected an actually missing ownership fact.
                return Err(unsupported());
            };
            if drop.value_origin() != origin {
                return Err(missing());
            }
            if drop.point() != UnitDropPoint::LoopExit(statement) {
                // A legal own source kept live after this loop needs a separate owner proof.
                return Err(unsupported());
            }
            if exhaustion.actions()
                != [
                    Action::FinishProvider(statement),
                    Action::EndSource(statement),
                    Action::Drop(*drop),
                ]
            {
                return Err(missing());
            }
            Ok(Some(*drop))
        }
        UnitIterationSourceAccess::Shared => {
            if !drops.is_empty()
                || exhaustion.actions()
                    != [
                        Action::FinishProvider(statement),
                        Action::EndSource(statement),
                    ]
            {
                return Err(missing());
            }
            Ok(None)
        }
        _ => Err(unsupported()),
    }
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}

fn reject_nested(
    parsed: &ParsedFile,
    statement: StatementId,
    span: Span,
) -> Result<(), LoweringError> {
    for (id, node) in parsed.ast().statements().iter() {
        if id != statement
            && contains(node.span(), span)
            && matches!(
                node.payload(),
                Statement::For { .. } | Statement::While { .. } | Statement::Loop { .. }
            )
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
    }
    for (_, node) in parsed.ast().expressions().iter() {
        if contains(node.span(), span)
            && matches!(
                node.payload(),
                Expression::If { .. } | Expression::When { .. } | Expression::Lambda { .. }
            )
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
    }
    Ok(())
}

fn straight_statement(
    parsed: &ParsedFile,
    statement: StatementId,
    span: Span,
) -> Result<(), LoweringError> {
    let node = parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
    match node.payload() {
        Statement::Block { elements } => {
            for child in elements {
                straight_statement(parsed, *child, span)?;
            }
            Ok(())
        }
        Statement::Expression { expression } => straight_expression(parsed, *expression, span),
        Statement::LocalVariable { declaration } => {
            let (_, item, _) = unwrap_modified(parsed, *declaration)?;
            let Item::Variable { initializer, .. } = item else {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            };
            straight_expression(parsed, initializer, span)
        }
        _ => Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
    }
}

fn straight_expression(
    parsed: &ParsedFile,
    expression: ExpressionId,
    span: Span,
) -> Result<(), LoweringError> {
    let node = parsed
        .ast()
        .expressions()
        .get(expression)
        .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
    match node.payload() {
        Expression::Name | Expression::Literal(_) => Ok(()),
        Expression::String { parts }
            if parts.iter().all(|part| matches!(part, StringPart::Text(_))) =>
        {
            Ok(())
        }
        Expression::Call {
            callee, arguments, ..
        } => {
            let callee = parsed
                .ast()
                .expressions()
                .get(*callee)
                .map_err(|_| lowering_error(LoweringErrorKind::MissingFact, span))?;
            if !matches!(callee.payload(), Expression::Name) {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            }
            for argument in arguments {
                straight_expression(parsed, argument.value, span)?;
            }
            Ok(())
        }
        _ => Err(lowering_error(LoweringErrorKind::UnsupportedNode, span)),
    }
}
