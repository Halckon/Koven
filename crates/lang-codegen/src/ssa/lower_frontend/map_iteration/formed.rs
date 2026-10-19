//! General ordinary local formation uses original Expression ownership, not a lexical carrier.
pub(super) mod retained;
use super::*;
use crate::ssa::map_source::{BindingDeclaration, Birth, Formation, OwnerUpdate, named_formation};
use lang_frontend::{
    name_resolution::ReferenceTarget,
    ownership_checking::{IterationSource, MapIterationComponent, OwnershipBindingKind},
    parser::{NameMarker, VariableKind},
    type_checking::{ExpressionCategory, IntrinsicTypeConstructor, MapIterationKind},
};

pub(super) fn formation(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    root: SymbolId,
    storage: TypeId,
    span: Span,
) -> Result<Formation<SymbolId>, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    named_formation(
        parsed,
        span,
        root,
        |root| {
            let symbol = names.symbols().get(root.index()).ok_or_else(missing)?;
            if symbol.kind() == SymbolKind::ValueParameter {
                let marker = owned_parameter((parsed, names, typed, owned), root, storage, span)?;
                return Ok(BindingDeclaration::EntryParameter { marker });
            }
            if symbol.kind() != SymbolKind::Variable {
                return Err(unsupported());
            }
            if symbol.is_synthetic()
                || typed.symbol_type(root) != Some(storage)
                || !owned.formed_owned_locals().contains(&root)
            {
                return Err(missing());
            }
            let item =
                unique(
                    parsed
                        .ast()
                        .items()
                        .iter()
                        .filter_map(|(_, node)| match node.payload() {
                            Item::Variable {
                                kind,
                                name: NameMarker::Present(marker),
                                initializer,
                                ..
                            } if *marker == symbol.span() => Some((*kind, *marker, *initializer)),
                            _ => None,
                        }),
                )
                .ok_or_else(missing)?;
            if !matches!(item.0, VariableKind::Val | VariableKind::Var) {
                return Err(unsupported());
            }
            if typed.expression_type(item.2) != Some(storage) {
                return Err(missing());
            }
            Ok(BindingDeclaration::Local {
                marker: item.1,
                initializer: item.2,
            })
        },
        |span| {
            let reference = unique(names.references().iter().filter(|r| r.span() == span))
                .ok_or_else(missing)?;
            match reference.target() {
                ReferenceTarget::Symbol(root) => Ok(*root),
                _ => Err(missing()),
            }
        },
        |id| typed.expression_type(id).zip(typed.expression_category(id)),
        |id| match parsed
            .ast()
            .expressions()
            .get(id)
            .map_err(|_| missing())?
            .payload()
        {
            Expression::Call { .. } => Ok(Birth::Call(id)),
            _ => Err(unsupported()),
        },
        typed
            .map_puts()
            .iter()
            .map(|d| OwnerUpdate {
                expression: d.expression(),
                receiver: d.receiver(),
            })
            .chain(typed.map_removes().iter().map(|d| OwnerUpdate {
                expression: d.expression(),
                receiver: d.receiver(),
            })),
    )
}

/// The parameter belongs to this execution body, rather than an enclosing capture.
pub(super) fn owned_parameter(
    frontend: (
        &ParsedFile,
        &NameResolution,
        &TypedFile,
        &OwnershipCheckedFile,
    ),
    root: SymbolId,
    storage: TypeId,
    span: Span,
) -> Result<Span, LoweringError> {
    let (parsed, names, typed, owned) = frontend;
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let symbol = names.symbols().get(root.index()).ok_or_else(missing)?;
    let function = parsed
        .ast()
        .items()
        .iter()
        .filter(|(_, node)| {
            matches!(node.payload(), Item::Function { .. })
                && node.span().source_id() == span.source_id()
                && node.span().start() <= span.start()
                && span.end() <= node.span().end()
        })
        .min_by_key(|(_, node)| node.span().end() - node.span().start())
        .ok_or_else(unsupported)?;
    let Item::Function { parameters, .. } = function.1.payload() else {
        return Err(missing());
    };
    if parsed.ast().expressions().iter().any(|(_, node)| {
        matches!(node.payload(), Expression::Lambda { .. })
            && node.span().start() <= span.start()
            && span.end() <= node.span().end()
    }) {
        return Err(unsupported());
    }
    if symbol.kind() != SymbolKind::ValueParameter
        || symbol.is_synthetic()
        || !parameters
            .iter()
            .any(|p| p.name == NameMarker::Present(symbol.span()))
        || typed.symbol_type(root) != Some(storage)
        || typed.parameter_mode(root) != Some(ParameterMode::Value)
        || owned.binding_kind(root) != Some(OwnershipBindingKind::Owned)
    {
        return Err(missing());
    }
    Ok(symbol.span())
}

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
    let d = plan.descriptor();
    let Some(LoanTarget::Place(place)) = plan.source() else {
        return Err(unsupported());
    };
    let root = place.root();
    if !place.is_root()
        || plan.source_kind() != &IterationSource::BorrowedSource(LoanTarget::Place(place.clone()))
        || plan.source_access() != MapIterationSourceAccess::Owned
        || typed.copyability(d.source_type()) != Some(Copyability::MoveOnly)
        || typed.expression_category(d.source()) != Some(ExpressionCategory::Place)
        || typed.expression_type(d.source()) != Some(d.source_type())
        || d.key_binding() == d.value_binding()
        || d.bindings().contains(&root)
    {
        return Err(missing());
    }
    let resource = typed
        .is_resource_type(d.source_type())
        .ok_or_else(missing)?;
    if qualify::has_enclosing_control(parsed, span) {
        return Err(unsupported());
    }
    let constructor = match d.kind() {
        MapIterationKind::Map => IntrinsicTypeConstructor::Map,
        MapIterationKind::MutableMap => IntrinsicTypeConstructor::MutableMap,
    };
    if !matches!(typed.types().get(d.source_type()),Some(TypeKind::Intrinsic{constructor:actual,arguments}) if *actual==constructor && arguments.as_slice()==[d.key_type(),d.value_type()])
    {
        return Err(missing());
    }
    for (index, component) in [MapIterationComponent::Key, MapIterationComponent::Value]
        .into_iter()
        .enumerate()
    {
        let binding = plan.bindings()[index];
        let loan = &plan.loans()[index];
        if binding.symbol() != d.bindings()[index]
            || binding.kind() != OwnershipBindingKind::Shared
            || loan.binding() != binding.symbol()
            || loan.component() != component
            || loan.statement() != d.statement()
            || loan.source() != d.source()
            || Some(loan.origin()) != plan.source()
            || loan.kind() != lang_frontend::ownership_checking::LoanKind::Shared
            || typed.symbol_type(binding.symbol()) != Some([d.key_type(), d.value_type()][index])
        {
            return Err(missing());
        }
    }
    let formation = formation(parsed, names, typed, owned, root, d.source_type(), span)?;
    if let Birth::Call(producer) = formation.birth {
        local::producer(parsed, names, typed, owned, producer, d.source_type(), span)?;
    }
    // Other epochs retain ordinary call delivery, not provider capability.
    // Only the current epoch can be an already-proved alias Name.
    for epoch in &formation.history {
        let outer = parsed
            .ast()
            .expressions()
            .get(epoch.rhs)
            .map_err(|_| missing())?;
        if outer.span() == formation.binding {
            continue;
        }
        let call = crate::ssa::map_source::epoch_call(parsed, epoch.rhs)?;
        if typed.map_construction(call).is_some() || typed.map_consume(call).is_some() {
            local::producer(parsed, names, typed, owned, call, d.source_type(), span)?;
            continue;
        }
        if typed.map_with_value(call).is_some() {
            return Err(unsupported());
        }
        let ordinary = typed.call(call).ok_or_else(missing)?;
        if !matches!(
            ordinary.target(),
            lang_frontend::type_checking::CallableTarget::Source(_)
        ) {
            return Err(unsupported());
        }
    }
    let source_identity = SourceIdentity::Formed {
        symbol: root,
        birth: formation.birth,
    };
    if let Some(shape) = conditional::shape(parsed, names, typed, *d, body, source_identity, span)?
    {
        let owner = conditional::qualify(parsed, names, typed, owned, plan, shape, span)?;
        return Ok(qualify::Qualified {
            plan,
            source: source_identity,
            body,
            origin: names
                .symbols()
                .get(root.index())
                .ok_or_else(missing)?
                .span(),
            owner,
            body_exit: IterationExitKind::Fallthrough,
            conditional: Some(shape),
            nesting: nested::Nesting { parent: None },
            local: None,
        });
    }
    let body_exit = exit_shape::body_exit(parsed, names, typed, *d, body, span)?;
    exit_shape::cleanup(owned, plan, body, body_exit, span)?;
    let owner = if resource {
        resource::qualify(
            parsed,
            names,
            typed,
            owned,
            plan,
            (root, Some(&formation)),
            body_exit,
        )?
    } else {
        source_owner::qualify_formed(
            parsed,
            names,
            owned,
            plan,
            (root, &formation),
            body,
            body_exit,
        )?
    };
    Ok(qualify::Qualified {
        plan,
        source: SourceIdentity::Formed {
            symbol: root,
            birth: formation.birth,
        },
        body,
        origin: names
            .symbols()
            .get(root.index())
            .ok_or_else(missing)?
            .span(),
        owner: Some(owner),
        body_exit,
        conditional: None,
        nesting: nested::Nesting { parent: None },
        local: None,
    })
}

/// Finishing the next replacement RHS still cleans the current binding epoch.
/// The enclosing assignment installs the new owner only after this exact event.
pub(super) fn replacement_point(formation: Option<&Formation<SymbolId>>, point: DropPoint) -> bool {
    matches!((formation.and_then(|f|f.replacement),point),
        (Some(expected),DropPoint::AfterExpression(actual)) if expected==actual)
}

/// Partition one named root by real binding/replacement events, never by spelling or adjacency.
pub(super) fn drops(
    parsed: &ParsedFile,
    owned: &OwnershipCheckedFile,
    root: SymbolId,
    formation: &Formation<SymbolId>,
) -> Result<Vec<lang_frontend::ownership_checking::DropFact>, LoweringError> {
    use lang_frontend::ownership_checking::{CleanupCondition, CleanupOwnerValue};
    let mut facts = Vec::new();
    for fact in owned
        .drops()
        .iter()
        .copied()
        .filter(|f| f.target() == lang_frontend::ownership_checking::DropTarget::Named(root))
    {
        let bad = || error(LoweringErrorKind::MissingFact, fact.value_origin());
        let point = crate::ssa::map_source::drop_event(parsed, fact.point())
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, fact.value_origin()))?;
        let epoch = crate::ssa::map_source::epoch_at(parsed, formation, point)?;
        if fact.capture_slot().is_some() || fact.instance_address().is_some() {
            return Err(bad());
        }
        if crate::ssa::map_source::in_formation(parsed, formation, point)? {
            if fact.value_origin() != formation.declaration {
                return Err(bad());
            }
            facts.push(fact);
        } else {
            let producer = crate::ssa::map_source::epoch_call(parsed, epoch.rhs)?;
            let owner = fact.owner().ok_or_else(bad)?;
            let conditions = owned.cleanup_conditions();
            match conditions.owner_value(owner).ok_or_else(bad)? {
                CleanupOwnerValue::Expression { expression, origin } => {
                    let node = parsed
                        .ast()
                        .expressions()
                        .get(producer)
                        .map_err(|_| bad())?;
                    if fact.value_origin() != epoch.origin
                        || *expression != producer
                        || *origin != node.span()
                        || fact
                            .condition()
                            .is_some_and(|c| conditions.get(c) != Some(&CleanupCondition::Always))
                    {
                        return Err(bad());
                    }
                }
                CleanupOwnerValue::IterationPhi { .. } => {
                    let available = source_owner::formed_condition(
                        parsed,
                        owned,
                        (root, Birth::Call(producer)),
                        owner,
                        point.span,
                    )?;
                    if fact.value_origin() != formation.declaration
                        || fact.condition() != Some(available)
                    {
                        return Err(bad());
                    }
                }
                _ => return Err(bad()),
            }
        }
    }
    Ok(facts)
}
