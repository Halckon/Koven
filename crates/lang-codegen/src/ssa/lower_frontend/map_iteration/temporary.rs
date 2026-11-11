//! Temporary source exits retain the real Expression owner without parameter closure phis.
mod cleanup;
use super::*;
use lang_frontend::{
    name_resolution::ReferenceTarget,
    ownership_checking::{
        ClosureCaptureSource, IterationSource, MapIterationComponent, OwnershipBindingKind,
    },
    type_checking::{IntrinsicTypeConstructor, MapIterationKind},
};

pub(super) fn qualify<'a>(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &'a OwnershipCheckedFile,
    plan: &'a MapIterationOwnershipPlan,
    body: StatementId,
    span: Span,
) -> Result<qualify::Qualified<'a>, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let descriptor = plan.descriptor();
    let source =
        crate::ssa::map_source::transparent_expression(parsed, descriptor.source(), |id| {
            typed.expression_type(id).zip(typed.expression_category(id))
        })?;
    let node = parsed
        .ast()
        .expressions()
        .get(source)
        .map_err(|_| missing())?;
    let origin = node.span();
    // Named and temporary providers consume the same selected ordinary Call
    // formation; only the source lifetime/cleanup proof differs below.
    local::producer(
        parsed,
        names,
        typed,
        owned,
        source,
        descriptor.source_type(),
        span,
    )?;
    if typed.copyability(descriptor.source_type()) != Some(Copyability::MoveOnly)
        || plan.source_access() != MapIterationSourceAccess::Temporary
        || plan.source_kind() != &IterationSource::BorrowedSource(LoanTarget::Temporary(source))
    {
        return Err(missing());
    }
    let constructor = match descriptor.kind() {
        MapIterationKind::Map => IntrinsicTypeConstructor::Map,
        MapIterationKind::MutableMap => IntrinsicTypeConstructor::MutableMap,
    };
    if !matches!(typed.types().get(descriptor.source_type()),
        Some(TypeKind::Intrinsic { constructor: actual, arguments })
            if *actual == constructor
                && arguments.as_slice() == [descriptor.key_type(), descriptor.value_type()])
        || descriptor.key_binding() == descriptor.value_binding()
    {
        return Err(missing());
    }
    // Storage planning admits the represented key/value layout. Temporary lifetime
    // proof depends on this Call owner and its cleanup, not a String/String syntax gate.
    for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
        .into_iter()
        .enumerate()
    {
        let binding = plan.bindings()[index];
        let loan = &plan.loans()[index];
        let symbol = descriptor.bindings()[index];
        if binding.symbol() != symbol
            || binding.kind() != OwnershipBindingKind::Shared
            || owned.binding_kind(symbol) != Some(OwnershipBindingKind::Shared)
            || loan.binding() != symbol
            || loan.component() != component
            || loan.statement() != descriptor.statement()
            || loan.source() != descriptor.source()
            || loan.origin() != &LoanTarget::Temporary(source)
            || loan.kind() != lang_frontend::ownership_checking::LoanKind::Shared
            || typed.symbol_type(symbol)
                != Some([descriptor.key_type(), descriptor.value_type()][index])
        {
            return Err(missing());
        }
    }
    if qualify::has_enclosing_control(parsed, span) {
        return Err(unsupported());
    }
    let source_identity = SourceIdentity::Temporary(source);
    if let Some(shape) = conditional::shape(
        parsed,
        names,
        typed,
        *descriptor,
        body,
        source_identity,
        span,
    )? {
        let owner = conditional::qualify(parsed, names, typed, owned, plan, shape, span)?;
        return Ok(qualify::Qualified {
            plan,
            source: source_identity,
            body,
            origin,
            owner,
            body_exit: IterationExitKind::Fallthrough,
            conditional: Some(shape),
            nesting: nested::Nesting { parent: None },
            local: None,
        });
    }
    let body_exit = exit_shape::body_exit(parsed, names, typed, *descriptor, body, span)?;
    let guard = if body_exit == IterationExitKind::Fallthrough {
        None
    } else {
        Some(tail_guard(
            parsed, names, typed, owned, body, body_exit, span,
        )?)
    };
    let proof = cleanup::prove(parsed, owned, plan, body, body_exit, guard, span)?;
    Ok(qualify::Qualified {
        plan,
        source: SourceIdentity::Temporary(source),
        body,
        origin,
        owner: Some(proof),
        body_exit,
        conditional: None,
        nesting: nested::Nesting { parent: None },
        local: None,
    })
}

fn tail_guard(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    body: StatementId,
    _kind: IterationExitKind,
    span: Span,
) -> Result<super::guard::Local, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let body_node = parsed.ast().statements().get(body).map_err(|_| missing())?;
    let Statement::Block { elements } = body_node.payload() else {
        return Err(unsupported());
    };
    let Some((_, prefix)) = elements.split_last() else {
        return Err(unsupported());
    };
    let Some((&first, rest)) = prefix.split_first() else {
        return Err(unsupported());
    };
    let guard = super::guard::local(parsed, names, typed, owned, first, span)?;
    for statement in rest {
        let node = parsed
            .ast()
            .statements()
            .get(*statement)
            .map_err(|_| missing())?;
        if !matches!(node.payload(), Statement::Expression { .. }) {
            return Err(unsupported());
        }
    }
    // Formation is historical; only the exact transfer Drop below proves current cleanup.
    // This local is a lexical sentinel, with no aliases, calls, moves or captures.
    if names
        .references()
        .iter()
        .any(|reference| reference.target() == &ReferenceTarget::Symbol(guard.symbol))
        || owned
            .captures()
            .iter()
            .any(|capture| capture.source() == ClosureCaptureSource::Symbol(guard.symbol))
    {
        return Err(unsupported());
    }
    for (expression, node) in parsed.ast().expressions().iter() {
        if contains(body_node.span(), node.span())
            && expression != guard.initializer
            && matches!(node.payload(), Expression::Call { .. })
        {
            let ty = typed.expression_type(expression).ok_or_else(missing)?;
            if typed.is_resource_type(ty).ok_or_else(missing)? {
                return Err(unsupported());
            }
        }
    }
    Ok(guard)
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.source_id() == inner.source_id()
        && outer.start() <= inner.start()
        && inner.end() <= outer.end()
}
