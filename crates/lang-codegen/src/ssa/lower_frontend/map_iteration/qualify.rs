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
    pub(super) source: SourceIdentity,
    pub(super) body: StatementId,
    pub(super) origin: Span,
    pub(super) owner: Option<source_owner::Proof>,
    pub(super) body_exit: IterationExitKind,
    pub(super) conditional: Option<conditional::Shape>,
    pub(super) nested: Option<nested::Pair>,
    pub(super) local: Option<local::Proof>,
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
    let nested = nested::qualify::pair(parsed, names, typed, owned, descriptor)?;
    let qualified = qualify_one(parsed, names, typed, owned, descriptor, nested)?;
    if let Some(pair) = nested {
        let other = if descriptor.statement() == pair.outer {
            pair.inner
        } else {
            pair.outer
        };
        let other = typed
            .map_iteration(other)
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, qualified.origin))?;
        // The sorted descriptor is the capability admission unit for the pair.
        // Preserve corruption diagnostics, but do not leak the partner's later U span.
        let current_span = parsed
            .ast()
            .statements()
            .get(descriptor.statement())
            .map_err(|_| error(LoweringErrorKind::MissingFact, qualified.origin))?
            .span();
        qualify_one(parsed, names, typed, owned, *other, nested).map_err(|failure| {
            if failure.kind == LoweringErrorKind::UnsupportedNode {
                error(LoweringErrorKind::UnsupportedNode, current_span)
            } else {
                failure
            }
        })?;
    }
    Ok(qualified)
}

fn qualify_one<'a>(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &'a OwnershipCheckedFile,
    descriptor: MapIterationDescriptor,
    nested: Option<nested::Pair>,
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
    if plan.source_access() == MapIterationSourceAccess::Temporary {
        return temporary::qualify(parsed, names, typed, owned, plan, *body, span);
    }
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
    if data.kind() == SymbolKind::Variable {
        if nested.is_some() {
            return Err(unsupported());
        }
        return local::qualify(parsed, names, typed, owned, plan, *body, span);
    }
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
        (
            MapIterationKind::Map,
            ParameterMode::Value | ParameterMode::Borrow
        ) | (MapIterationKind::MutableMap, ParameterMode::Borrow)
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
    // AST only bounds capability. Phase 3 remains the source of exit/cleanup facts.
    if nested.is_none() && has_enclosing_control(parsed, span) {
        return Err(unsupported());
    }
    let conditional = if nested.is_some() {
        None
    } else {
        conditional::shape(parsed, names, typed, descriptor, *body, span)?
    };
    let tail = nested.and_then(|pair| pair.tail);
    let body_exit = if let Some(pair) = nested {
        pair.tail
            .filter(|_| descriptor.statement() == pair.inner)
            .map_or(IterationExitKind::Fallthrough, |shape| shape.kind)
    } else if conditional.is_some() {
        IterationExitKind::Fallthrough
    } else {
        exit_shape::body_exit(parsed, names, typed, descriptor, *body, span)?
    };
    if conditional.is_some()
        || matches!(body_exit, IterationExitKind::Return(_))
        || tail.is_some_and(|shape| matches!(shape.kind, IterationExitKind::Return(_)))
    {
        let callable = unique(
            typed
                .callables()
                .iter()
                .filter(|callable| callable.parameter_symbols().contains(&Some(symbol))),
        )
        .ok_or_else(missing)?;
        if builtin_type(typed, callable.return_type()) != Some(BuiltinType::Int) {
            return Err(unsupported());
        }
    }
    if let Some(shape) = conditional {
        let owner = conditional::qualify(parsed, names, owned, plan, symbol, shape, span)?;
        return Ok(Qualified {
            plan,
            source: SourceIdentity::Parameter(symbol),
            body: *body,
            origin: data.span(),
            owner,
            body_exit,
            conditional,
            nested,
            local: None,
        });
    }
    if let Some(pair) = nested.filter(|pair| pair.tail.is_some()) {
        nested::tail::validate_cleanup(owned, pair, pair.tail.ok_or_else(missing)?)?;
    } else {
        exit_shape::cleanup(owned, plan, *body, body_exit, span)?;
        if nested.is_some() {
            nested::qualify::cleanup(owned, plan, *body, span)?;
        }
    }
    let exhaustion = unique(
        plan.exits()
            .iter()
            .filter(|exit| exit.kind() == IterationExitKind::Exhaustion),
    )
    .ok_or_else(missing)?;
    let owner = if access == MapIterationSourceAccess::Owned {
        Some(source_owner::qualify(
            parsed, names, owned, plan, symbol, *body, body_exit,
        )?)
    } else {
        if exhaustion.actions().len() != 2 || (tail.is_none() && !plan.closure_phis().is_empty()) {
            return Err(unsupported());
        }
        None
    };
    Ok(Qualified {
        plan,
        source: SourceIdentity::Parameter(symbol),
        body: *body,
        origin: data.span(),
        owner,
        body_exit,
        conditional: None,
        nested,
        local: None,
    })
}

pub(super) fn has_enclosing_control(parsed: &ParsedFile, span: Span) -> bool {
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

pub(super) fn straight_statement(
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
