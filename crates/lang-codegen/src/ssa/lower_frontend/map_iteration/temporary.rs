//! The observed temporary source has an Expression owner, not parameter closure phis.
use super::*;
use lang_frontend::{
    name_resolution::ReferenceTarget,
    ownership_checking::{
        CleanupOwnerValue, DropTarget, IterationPhiBoundary, IterationPhiIncomingKind,
        IterationSource, MapIterationComponent, OwnershipBindingKind,
    },
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
    let source = descriptor.source();
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
            || loan.source() != source
            || loan.origin() != &LoanTarget::Temporary(source)
            || loan.kind() != lang_frontend::ownership_checking::LoanKind::Shared
            || typed.symbol_type(symbol)
                != Some([descriptor.key_type(), descriptor.value_type()][index])
        {
            return Err(missing());
        }
    }
    if qualify::has_enclosing_control(parsed, span)
        || !qualify::straight_statement(parsed, typed, body)?
    {
        return Err(unsupported());
    }
    let proof = owner_proof(owned, plan, body, origin, span)?;
    Ok(qualify::Qualified {
        plan,
        source: SourceIdentity::Temporary(source),
        body,
        origin,
        owner: Some(proof),
        body_exit: IterationExitKind::Fallthrough,
        conditional: None,
    })
}

fn owner_proof(
    owned: &OwnershipCheckedFile,
    plan: &MapIterationOwnershipPlan,
    body: StatementId,
    origin: Span,
    span: Span,
) -> Result<source_owner::Proof, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let descriptor = plan.descriptor();
    let statement = descriptor.statement();
    let source = descriptor.source();
    if !plan.closure_phis().is_empty()
        || !plan.closure_flow().header().is_empty()
        || !plan.closure_flow().exit().is_empty()
        || !plan.capture_graph().nodes().is_empty()
    {
        return Err(unsupported());
    }
    let conditions = owned.cleanup_conditions();
    let incoming_shapes = [
        (
            IterationPhiIncomingKind::Entry,
            DropPoint::AfterExpression(source),
            IterationPhiBoundary::Header,
        ),
        (
            IterationPhiIncomingKind::Fallthrough,
            DropPoint::AfterStatement(body),
            IterationPhiBoundary::Header,
        ),
        (
            IterationPhiIncomingKind::Exhaustion,
            DropPoint::LoopExit(statement),
            IterationPhiBoundary::Exit,
        ),
    ];
    for (kind, point, boundary) in incoming_shapes {
        let incoming = unique(
            plan.closure_phi_incomings()
                .iter()
                .filter(|incoming| incoming.kind() == kind),
        )
        .ok_or_else(missing)?;
        if incoming.point() != point
            || incoming.boundary() != boundary
            || conditions.get(incoming.condition()) != Some(&CleanupCondition::Always)
            || !incoming.bindings().is_empty()
        {
            return Err(missing());
        }
    }
    if plan.closure_phi_incomings().len() != incoming_shapes.len() {
        return Err(unsupported());
    }
    let drop = unique(
        owned
            .drops()
            .iter()
            .copied()
            .filter(|fact| fact.target() == DropTarget::Temporary(source)),
    )
    .ok_or_else(missing)?;
    if drop.point() != DropPoint::LoopExit(statement)
        || drop.value_origin() != origin
        || drop.condition().is_some()
        || drop.capture_slot().is_some()
        || drop.instance_address().is_some()
    {
        return Err(missing());
    }
    let owner = drop.owner().ok_or_else(missing)?;
    if conditions.owner_value(owner)
        != Some(&CleanupOwnerValue::Expression {
            expression: source,
            origin,
        })
        || owned
            .drops()
            .iter()
            .any(|fact| fact.owner() == Some(owner) && *fact != drop)
    {
        return Err(missing());
    }
    exit_shape::cleanup(owned, plan, body, IterationExitKind::Fallthrough, span)?;
    let body_plan = unique(
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
    let body_actions = [
        IterationCleanupAction::EndBinding {
            statement,
            symbol: descriptor.value_binding(),
        },
        IterationCleanupAction::EndBinding {
            statement,
            symbol: descriptor.key_binding(),
        },
    ];
    // Body-local owner combinations need their own first evidence. Do not silently
    // extend this temporary source slice merely because generic straight-line AST fits.
    if body_plan.actions() != body_actions {
        return Err(unsupported());
    }
    if exhaustion.actions()
        != [
            IterationCleanupAction::FinishProvider(statement),
            IterationCleanupAction::EndSource(statement),
            IterationCleanupAction::Drop(drop),
        ]
    {
        return Err(missing());
    }
    Ok(source_owner::Proof {
        drop,
        transfer: None,
        conditional_return: None,
    })
}
