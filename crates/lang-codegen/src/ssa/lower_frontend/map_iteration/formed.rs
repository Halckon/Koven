//! General ordinary local formation uses original Expression ownership, not a lexical carrier.
use super::*;
use crate::ssa::map_source::{Formation, named_formation};
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
            Ok((item.1, item.2))
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
        typed
            .map_puts()
            .iter()
            .map(|d| d.receiver())
            .chain(typed.map_removes().iter().map(|d| d.receiver())),
    )
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
    if qualify::has_enclosing_control(parsed, span)
        || (!resource
            && [d.key_type(), d.value_type()]
                .iter()
                .any(|t| builtin_type(typed, *t) != Some(BuiltinType::String)))
    {
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
    local::producer(
        parsed,
        names,
        typed,
        owned,
        formation.producer,
        d.source_type(),
        span,
    )?;
    // Every other epoch is validated before lowering as well. Unmeasured calls
    // keep their original Unsupported boundary instead of failing a later birth check.
    for epoch in &formation.history {
        match crate::ssa::map_source::epoch_call(parsed, epoch.rhs) {
            Ok(producer) => {
                local::producer(parsed, names, typed, owned, producer, d.source_type(), span)?
            }
            Err(failure) if failure.kind == LoweringErrorKind::UnsupportedNode => {}
            Err(failure) => return Err(failure),
        }
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
            producer: formation.producer,
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
        nested: None,
        local: None,
    })
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
        let point = crate::ssa::map_source::drop_span(parsed, fact.point())
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, fact.value_origin()))?;
        let epoch = crate::ssa::map_source::epoch_at(parsed, formation, point)?;
        if fact.value_origin() != formation.declaration
            || fact.capture_slot().is_some()
            || fact.instance_address().is_some()
        {
            return Err(bad());
        }
        if crate::ssa::map_source::in_formation(parsed, formation, point)? {
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
                    if *expression != producer
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
                        (root, producer),
                        owner,
                        point,
                    )?;
                    if fact.condition() != Some(available) {
                        return Err(bad());
                    }
                }
                _ => return Err(bad()),
            }
        }
    }
    Ok(facts)
}
