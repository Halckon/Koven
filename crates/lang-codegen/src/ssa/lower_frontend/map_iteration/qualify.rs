//! Pure frontend qualification, before instance planning or SSA type allocation.
use super::*;
use lang_frontend::{
    name_resolution::ReferenceTarget,
    ownership_checking::{IterationSource, MapIterationComponent, OwnershipBindingKind},
    parser::StringPart,
    type_checking::{IntrinsicTypeConstructor, MapIterationDescriptor, MapIterationKind},
};

pub(super) struct Qualified<'a> {
    pub(super) plan: &'a MapIterationOwnershipPlan,
    pub(super) symbol: SymbolId,
    pub(super) body: StatementId,
    pub(super) origin: Span,
    pub(super) owner: Option<source_owner::Proof>,
}

pub(in super::super) fn validate(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), LoweringError> {
    let mut descriptors = typed.map_iterations().iter().collect::<Vec<_>>();
    descriptors.sort_by_key(|descriptor| descriptor.statement().index());
    for descriptor in descriptors {
        qualify(parsed, names, typed, owned, *descriptor)?;
    }
    Ok(())
}

pub(super) fn qualify<'a>(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &'a OwnershipCheckedFile,
    descriptor: MapIterationDescriptor,
) -> Result<Qualified<'a>, LoweringError> {
    let node = parsed
        .ast()
        .statements()
        .get(descriptor.statement())
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
    let span = node.span();
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let Statement::For { source, body, .. } = node.payload() else {
        return Err(missing());
    };
    if *source != descriptor.source() {
        return Err(missing());
    }
    // Complete FE validation permits unacquired/dead-tail descriptors without a plan.
    let plan = owned
        .map_iteration(descriptor.statement())
        .ok_or_else(unsupported)?;
    if plan.descriptor() != &descriptor
        || typed.map_iteration(descriptor.statement()) != Some(plan.descriptor())
    {
        return Err(missing());
    }
    let source_node = parsed
        .ast()
        .expressions()
        .get(*source)
        .map_err(|_| missing())?;
    if source_node.payload() != &Expression::Name {
        return Err(unsupported());
    }
    let source = plan.source().ok_or_else(missing)?;
    let LoanTarget::Place(place) = source else {
        return Err(unsupported());
    };
    if !place.is_root() {
        return Err(unsupported());
    }
    if plan.source_kind() != &IterationSource::BorrowedSource(source.clone()) {
        return Err(missing());
    }
    let symbol = place.root();
    let references = names
        .references()
        .iter()
        .filter(|reference| reference.span() == source_node.span())
        .collect::<Vec<_>>();
    if !matches!(references.as_slice(), [reference] if reference.target() == &ReferenceTarget::Symbol(symbol))
    {
        return Err(missing());
    }
    let data = names.symbols().get(symbol.index()).ok_or_else(missing)?;
    if data.kind() != SymbolKind::ValueParameter {
        return Err(unsupported());
    }
    let mode = typed.parameter_mode(symbol).ok_or_else(missing)?;
    let binding = owned.binding_kind(symbol).ok_or_else(missing)?;
    let access = plan.source_access();
    let expected_access = match mode {
        ParameterMode::Value => (OwnershipBindingKind::Owned, MapIterationSourceAccess::Owned),
        ParameterMode::Borrow => (
            OwnershipBindingKind::Shared,
            MapIterationSourceAccess::Shared,
        ),
        ParameterMode::Inout => (
            OwnershipBindingKind::Exclusive,
            MapIterationSourceAccess::Exclusive,
        ),
    };
    if (binding, access) != expected_access {
        return Err(missing());
    }
    if !matches!(
        (descriptor.kind(), mode),
        (MapIterationKind::Map, ParameterMode::Value)
            | (MapIterationKind::MutableMap, ParameterMode::Borrow)
    ) {
        return Err(unsupported());
    }
    let constructor = match descriptor.kind() {
        MapIterationKind::Map => IntrinsicTypeConstructor::Map,
        MapIterationKind::MutableMap => IntrinsicTypeConstructor::MutableMap,
    };
    if typed.expression_type(descriptor.source()) != Some(descriptor.source_type())
        || typed.symbol_type(symbol) != Some(descriptor.source_type())
        || !matches!(typed.types().get(descriptor.source_type()), Some(TypeKind::Intrinsic { constructor: actual, arguments })
            if *actual == constructor && arguments.as_slice() == [descriptor.key_type(), descriptor.value_type()])
        || descriptor.key_binding() == descriptor.value_binding()
        || descriptor.bindings().contains(&symbol)
    {
        return Err(missing());
    }
    for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
        .into_iter()
        .enumerate()
    {
        let binding = plan.bindings()[index];
        let loan = &plan.loans()[index];
        let symbol = descriptor.bindings()[index];
        let ty = [descriptor.key_type(), descriptor.value_type()][index];
        if binding.symbol() != symbol
            || binding.kind() != OwnershipBindingKind::Shared
            || loan.binding() != symbol
            || loan.component() != component
            || loan.statement() != descriptor.statement()
            || loan.source() != descriptor.source()
            || loan.origin() != source
            || loan.kind() != lang_frontend::ownership_checking::LoanKind::Shared
            || typed.symbol_type(symbol) != Some(ty)
        {
            return Err(missing());
        }
    }
    // AST is consulted only to bound backend shape. It never reconstructs facts.
    if !straight_statement(parsed, typed, *body)? || has_enclosing_control(parsed, span) {
        return Err(unsupported());
    }
    if plan.exits().iter().any(|exit| {
        !matches!(
            exit.kind(),
            IterationExitKind::Fallthrough | IterationExitKind::Exhaustion
        )
    }) {
        return Err(unsupported());
    }
    let fallthrough = unique(
        plan.exits()
            .iter()
            .filter(|exit| exit.kind() == IterationExitKind::Fallthrough),
    )
    .ok_or_else(missing)?;
    let exhaustion = unique(
        plan.exits()
            .iter()
            .filter(|exit| exit.kind() == IterationExitKind::Exhaustion),
    )
    .ok_or_else(missing)?;
    if plan.exits().len() != 2
        || fallthrough.point() != DropPoint::AfterStatement(*body)
        || exhaustion.point() != DropPoint::LoopExit(descriptor.statement())
        || [fallthrough, exhaustion].iter().any(|exit| {
            owned.cleanup_conditions().get(exit.condition()) != Some(&CleanupCondition::Always)
        })
    {
        return Err(missing());
    }
    let expected = [
        IterationCleanupAction::EndBinding {
            statement: descriptor.statement(),
            symbol: descriptor.value_binding(),
        },
        IterationCleanupAction::EndBinding {
            statement: descriptor.statement(),
            symbol: descriptor.key_binding(),
        },
    ];
    if !fallthrough.actions().ends_with(&expected)
        || fallthrough.actions()[..fallthrough.actions().len() - 2]
            .iter()
            .any(|action| !matches!(action, IterationCleanupAction::Drop(_)))
        || !exhaustion.actions().starts_with(&[
            IterationCleanupAction::FinishProvider(descriptor.statement()),
            IterationCleanupAction::EndSource(descriptor.statement()),
        ])
    {
        return Err(missing());
    }
    let owner = if access == MapIterationSourceAccess::Owned {
        Some(source_owner::qualify(
            parsed, names, owned, plan, symbol, *body,
        )?)
    } else {
        if exhaustion.actions().len() != 2 || !plan.closure_phis().is_empty() {
            return Err(unsupported());
        }
        None
    };
    Ok(Qualified {
        plan,
        symbol,
        body: *body,
        origin: data.span(),
        owner,
    })
}

fn has_enclosing_control(parsed: &ParsedFile, span: Span) -> bool {
    let contains = |outer: Span| {
        outer != span
            && outer.source_id() == span.source_id()
            && outer.start() <= span.start()
            && outer.end() >= span.end()
    };
    parsed.ast().statements().iter().any(|(_, node)| {
        contains(node.span())
            && matches!(
                node.payload(),
                Statement::For { .. } | Statement::While { .. } | Statement::Loop { .. }
            )
    }) || parsed.ast().expressions().iter().any(|(_, node)| {
        contains(node.span())
            && matches!(
                node.payload(),
                Expression::If { .. } | Expression::When { .. } | Expression::Lambda { .. }
            )
    })
}

fn straight_statement(
    parsed: &ParsedFile,
    typed: &TypedFile,
    statement: StatementId,
) -> Result<bool, LoweringError> {
    let node = parsed
        .ast()
        .statements()
        .get(statement)
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
    Ok(match node.payload() {
        Statement::Block { elements } => {
            for element in elements {
                if !straight_statement(parsed, typed, *element)? {
                    return Ok(false);
                }
            }
            true
        }
        Statement::Expression { expression } => straight_expression(parsed, typed, *expression)?,
        Statement::LocalVariable { declaration } => {
            let item = parsed
                .ast()
                .items()
                .get(*declaration)
                .map_err(|_| error(LoweringErrorKind::MissingFact, node.span()))?;
            match item.payload() {
                Item::Variable { initializer, .. } => {
                    straight_expression(parsed, typed, *initializer)?
                }
                _ => false,
            }
        }
        _ => false,
    })
}

fn straight_expression(
    parsed: &ParsedFile,
    typed: &TypedFile,
    expression: ExpressionId,
) -> Result<bool, LoweringError> {
    let node = parsed
        .ast()
        .expressions()
        .get(expression)
        .map_err(|_| LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
    Ok(match node.payload() {
        Expression::Name | Expression::Literal(_) => true,
        Expression::String { parts } => {
            parts.iter().all(|part| matches!(part, StringPart::Text(_)))
        }
        Expression::Call {
            callee, arguments, ..
        } => {
            let result = typed
                .expression_type(expression)
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, node.span()))?;
            if builtin_type(typed, result) == Some(BuiltinType::Nothing) {
                return Ok(false);
            }
            let callee = parsed
                .ast()
                .expressions()
                .get(*callee)
                .map_err(|_| error(LoweringErrorKind::MissingFact, node.span()))?;
            if callee.payload() != &Expression::Name {
                return Ok(false);
            }
            for argument in arguments {
                if !straight_expression(parsed, typed, argument.value)? {
                    return Ok(false);
                }
            }
            true
        }
        _ => false,
    })
}
