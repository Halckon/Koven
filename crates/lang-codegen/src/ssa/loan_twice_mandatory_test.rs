//! Test-only witness that the bounded lifecycle facts survive disabling observation.
use lang_frontend::{
    name_resolution::UnitDeclaration,
    ownership_checking::{
        CompilationUnitOwnership, OwnershipBindingKind, UnitDropPoint, UnitDropTarget,
        UnitReceiverOwnershipKind, UnitReceiverOwnershipTarget,
    },
    parser::{
        Expression, FunctionBody, FunctionForm, Item, ParameterModeMarker, ParsedFile, Statement,
    },
    type_checking::{
        BuiltinType, CallableResultSource, CompilationUnitTypes, Copyability, NominalKind,
        ParameterMode, UnitCallReceiverOrigin, UnitCallTarget, UnitExpressionId, UnitTypeKind,
    },
};

pub(super) fn ordered_calls(
    parsed: &ParsedFile,
    run: &UnitDeclaration,
) -> Option<[UnitExpressionId; 2]> {
    let ast = parsed.ast();
    let Item::Function {
        parameters,
        type_parameters,
        extension_receiver: None,
        form,
        ..
    } = ast.items().get(run.root()).ok()?.payload()
    else {
        return None;
    };
    let [parameter] = parameters.as_slice() else {
        return None;
    };
    if !type_parameters.is_empty()
        || !matches!(parameter.mode_marker, Some(ParameterModeMarker::Own(_)))
    {
        return None;
    }
    let body = match form {
        FunctionForm::ImplicitUnitBlock(body)
        | FunctionForm::Explicit {
            body: FunctionBody::Block(body),
            ..
        } => *body,
        _ => return None,
    };
    let Statement::Block { elements } = ast.statements().get(body).ok()?.payload() else {
        return None;
    };
    let [first, second] = elements.as_slice() else {
        return None;
    };
    let call = |id| {
        let Statement::Expression { expression } = ast.statements().get(id).ok()?.payload() else {
            return None;
        };
        let node = ast.expressions().get(*expression).ok()?;
        matches!(node.payload(), Expression::Call { arguments, .. } if arguments.is_empty())
            .then_some(UnitExpressionId::new(run.source_unit(), *expression))
    };
    Some([call(*first)?, call(*second)?])
}

pub(super) fn audit(
    parsed: &ParsedFile,
    run: &UnitDeclaration,
    typed: &CompilationUnitTypes,
    owned: &CompilationUnitOwnership,
    calls: [UnitExpressionId; 2],
) -> Option<()> {
    if calls != ordered_calls(parsed, run)? || calls[0] == calls[1] {
        return None;
    }
    let ast = parsed.ast();
    let mut prior = None;
    for id in calls {
        let node = ast.expressions().get(id.expression()).ok()?;
        let Expression::Call { callee, .. } = node.payload() else {
            return None;
        };
        let Expression::Member {
            receiver,
            safe: false,
            ..
        } = ast.expressions().get(*callee).ok()?.payload()
        else {
            return None;
        };
        let root_node = ast.expressions().get(*receiver).ok()?;
        let root_id = UnitExpressionId::new(run.source_unit(), *receiver);
        let descriptor = typed.call(id)?;
        let receiver = descriptor.receiver()?;
        let UnitCallReceiverOrigin::Expression(receiver_expr) = receiver.origin() else {
            return None;
        };
        let fact = owned.receiver_fact(id)?;
        let UnitReceiverOwnershipTarget::Place(place) = fact.target() else {
            return None;
        };
        if !matches!(root_node.payload(), Expression::Name)
            || !matches!(descriptor.target(), UnitCallTarget::Symbol(_))
            || descriptor.aborts()
            || !descriptor.arguments().is_empty()
            || !descriptor.instance().type_arguments().is_empty()
            || descriptor.result_source() != CallableResultSource::Owned
            || typed.types().get(descriptor.return_type())
                != Some(&UnitTypeKind::Builtin(BuiltinType::Int))
            || receiver.mode() != ParameterMode::Borrow
            || receiver_expr != root_id
            || typed.expression_type(root_id) != Some(receiver.ty())
            || typed.copyability(receiver.ty()) != Copyability::MoveOnly
            || typed.is_resource_type(receiver.ty()) != Some(false)
            || fact.call() != id
            || fact.source() != receiver.origin()
            || fact.receiver_type() != receiver.ty()
            || fact.kind() != UnitReceiverOwnershipKind::SharedLoan
            || fact.is_receiver_reservation()
            || fact.activation_point().is_some()
            || fact.begin_span() != root_node.span()
            || fact.end_span() != node.span()
            || !place.is_root()
            || !owned.bindings().iter().any(|binding| {
                binding.symbol() == place.root() && binding.kind() == OwnershipBindingKind::Owned
            })
        {
            return None;
        }
        let UnitTypeKind::Nominal {
            declaration,
            arguments,
        } = typed.types().get(receiver.ty())?
        else {
            return None;
        };
        if !arguments.is_empty()
            || !typed
                .signatures()
                .declaration(*declaration)
                .and_then(|signature| signature.nominal())
                .is_some_and(|signature| signature.kind() == NominalKind::Class)
        {
            return None;
        }
        let mut ends = owned
            .receiver_call_return_ends()
            .iter()
            .filter(|end| end.point() == UnitDropPoint::CallReturn(id));
        if ends.next()?.receiver() != fact || ends.next().is_some() {
            return None;
        }
        let pair = (place.root(), descriptor.target());
        if prior.is_some_and(|previous| previous != pair) {
            return None;
        }
        prior = Some(pair);
    }
    let (root, _) = prior?;
    let mut drops = owned
        .drops()
        .iter()
        .filter(|drop| drop.target() == UnitDropTarget::Named(root));
    (drops.next()?.point() == UnitDropPoint::CallReturn(calls[1]) && drops.next().is_none())
        .then_some(())
}
