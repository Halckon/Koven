//! Temporary source exits retain the real Expression owner without parameter closure phis.
mod cleanup;
mod tail;
use super::*;
use lang_frontend::{
    name_resolution::ReferenceTarget,
    ownership_checking::{IterationSource, MapIterationComponent, OwnershipBindingKind},
    parser::NameMarker,
    type_checking::{
        CallableResultSource, CallableTarget, ExpressionCategory, IntrinsicTypeConstructor,
        MapIterationKind,
    },
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
    let Expression::Call {
        callee, arguments, ..
    } = node.payload()
    else {
        return Err(unsupported());
    };
    let callee_node = parsed
        .ast()
        .expressions()
        .get(*callee)
        .map_err(|_| missing())?;
    if callee_node.payload() != &Expression::Name || !arguments.is_empty() {
        return Err(unsupported());
    }
    let call = typed.call(source).ok_or_else(missing)?;
    let CallableTarget::Source(target) = call.target() else {
        return Err(unsupported());
    };
    let symbol = names.symbols().get(target.index()).ok_or_else(missing)?;
    let signature = unique(
        typed
            .callables()
            .iter()
            .filter(|callable| callable.symbol() == target),
    )
    .ok_or_else(missing)?;
    if signature.result_source() != call.result_source() {
        return Err(missing());
    }
    if symbol.kind() != SymbolKind::Function
        || !parsed.roots().iter().any(|item| {
            parsed.ast().items().get(*item).is_ok_and(|node| {
                matches!(node.payload(), Item::Function { name: NameMarker::Present(name), .. }
                    if *name == symbol.span())
            })
        })
        || call.receiver().is_some()
        || call.aborts()
        || call.result_source() != CallableResultSource::Owned
        || !call.instance().type_arguments().is_empty()
        || signature.owner().is_some()
        || signature.receiver().is_some()
        || !signature.type_parameters().is_empty()
        || !signature.parameters().is_empty()
    {
        return Err(unsupported());
    }
    let reference = unique(
        names
            .references()
            .iter()
            .filter(|reference| reference.span() == callee_node.span()),
    )
    .ok_or_else(missing)?;
    // Name resolution publishes function overload sets even for one declaration;
    // the typed Call already identifies the selected factory.
    let matches_factory = match reference.target() {
        ReferenceTarget::Symbol(symbol) => *symbol == target,
        ReferenceTarget::OverloadSet(symbols) => symbols.contains(&target),
        _ => false,
    };
    if !matches_factory
        || call.expression() != source
        || !call.arguments().is_empty()
        || call.return_type() != descriptor.source_type()
        || signature.return_type() != descriptor.source_type()
        || typed.expression_type(source) != Some(descriptor.source_type())
        || typed.expression_category(source) != Some(ExpressionCategory::Temporary)
        || typed.copyability(descriptor.source_type()) != Some(Copyability::MoveOnly)
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
    if [descriptor.key_type(), descriptor.value_type()]
        .iter()
        .any(|ty| builtin_type(typed, *ty) != Some(BuiltinType::String))
    {
        return Err(unsupported());
    }
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
    let body_exit = exit_shape::body_exit(parsed, names, typed, *descriptor, body, span)?;
    let guard = if body_exit == IterationExitKind::Fallthrough {
        None
    } else {
        Some(tail::guard(
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
        nested: None,
        local: None,
    })
}
