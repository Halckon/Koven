//! Other live Map roots keep their own epoch and terminal fact across a provider.
use super::*;
use lang_frontend::ownership_checking::{DropFact, DropTarget, IterationPhiBoundary};

pub(in crate::ssa::lower_frontend) struct Epoch {
    pub root: SymbolId,
    pub birth: Birth<SymbolId>,
    pub formation: Option<Formation<SymbolId>>,
    pub at: Span,
}

pub(in crate::ssa::lower_frontend) fn resolve(
    frontend: (
        &ParsedFile,
        &NameResolution,
        &TypedFile,
        &OwnershipCheckedFile,
    ),
    root: SymbolId,
    storage: TypeId,
    rhs: ExpressionId,
    span: Span,
) -> Result<Epoch, LoweringError> {
    let (parsed, names, typed, owned) = frontend;
    let bad = || error(LoweringErrorKind::MissingFact, span);
    let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
    let rhs = crate::ssa::map_source::transparent_expression(parsed, rhs, |id| {
        typed.expression_type(id).zip(typed.expression_category(id))
    })?;
    let node = parsed.ast().expressions().get(rhs).map_err(|_| bad())?;
    if typed.expression_type(rhs) != Some(storage) {
        return Err(bad());
    }
    match node.payload() {
        Expression::Call { .. } => Ok(Epoch {
            root,
            birth: Birth::Call(rhs),
            formation: None,
            at: node.span(),
        }),
        Expression::Name => {
            if typed.expression_category(rhs) != Some(ExpressionCategory::Place) {
                return Err(bad());
            }
            let reference = unique(
                names
                    .references()
                    .iter()
                    .filter(|r| r.span() == node.span()),
            )
            .ok_or_else(bad)?;
            let ReferenceTarget::Symbol(source) = reference.target() else {
                return Err(bad());
            };
            if names
                .symbols()
                .get(source.index())
                .is_some_and(|s| s.kind() == SymbolKind::ValueParameter)
            {
                let origin = super::owned_parameter(frontend, *source, storage, span)?;
                return Ok(Epoch {
                    root: *source,
                    birth: Birth::EntryParameter {
                        root: *source,
                        reference: rhs,
                        origin,
                    },
                    formation: None,
                    at: node.span(),
                });
            }
            let formation =
                super::raw_formation(parsed, names, typed, owned, *source, storage, node.span())?;
            Ok(Epoch {
                root: *source,
                birth: formation.birth,
                formation: Some(formation),
                at: node.span(),
            })
        }
        _ => Err(unsupported()),
    }
}

/// Formation is present only for a real local epoch; parameters retain their entry slot.
pub(in super::super) struct Retained {
    pub(in super::super) source: SourceIdentity,
    pub(in super::super) formation: Option<Formation<SymbolId>>,
}

pub(in super::super) fn proof(
    frontend: (
        &ParsedFile,
        &NameResolution,
        &TypedFile,
        &OwnershipCheckedFile,
    ),
    plan: &MapIterationOwnershipPlan,
    fact: DropFact,
    span: Span,
) -> Result<Option<Retained>, LoweringError> {
    let (parsed, names, typed, owned) = frontend;
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let DropTarget::Named(root) = fact.target() else {
        return Ok(None);
    };
    if !plan
        .closure_flow()
        .exit()
        .iter()
        .any(|binding| binding.symbol() == root)
        || matches!(plan.source(),Some(LoanTarget::Place(place)) if place.root()==root)
    {
        return Ok(None);
    }
    let storage = typed.symbol_type(root).ok_or_else(missing)?;
    if !matches!(
        typed.types().get(storage),
        Some(lang_frontend::type_checking::TypeKind::Intrinsic {
            constructor: IntrinsicTypeConstructor::Map | IntrinsicTypeConstructor::MutableMap,
            ..
        })
    ) {
        return Ok(None);
    }
    let event = crate::ssa::map_source::drop_event(parsed, fact.point())
        .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
    if event.span.end() <= span.end() {
        return Ok(None);
    }
    let symbol = names.symbols().get(root.index()).ok_or_else(missing)?;
    let (source, formation, origin) = if symbol.kind() == SymbolKind::ValueParameter {
        let origin = owned_parameter((parsed, names, typed, owned), root, storage, span)?;
        (SourceIdentity::Parameter(root), None, origin)
    } else {
        let formation = formation(parsed, names, typed, owned, root, storage, span)?;
        if !drops(parsed, owned, root, &formation)?.contains(&fact) {
            return Ok(None);
        }
        let origin = formation.declaration;
        (
            SourceIdentity::Formed {
                symbol: root,
                birth: formation.birth,
            },
            Some(formation),
            origin,
        )
    };
    let flow = source_owner::Flow {
        statement: plan.descriptor().statement(),
        source,
        span,
        late: Some(fact),
    };
    let exit = source_owner::phi(
        owned.cleanup_conditions(),
        plan,
        root,
        IterationPhiBoundary::Exit,
        span,
    )?;
    source_owner::flow_condition(parsed, names, owned, flow, exit.owner(), span)?;
    let live = source_owner::flow_condition(
        parsed,
        names,
        owned,
        flow,
        fact.owner().ok_or_else(missing)?,
        span,
    )?;
    if fact.value_origin() != origin
        || fact.capture_slot().is_some()
        || fact.instance_address().is_some()
        || fact.condition() != Some(live)
    {
        return Err(missing());
    }
    let future = crate::ssa::map_source::future_reference(
        parsed,
        span,
        event.span,
        names
            .references()
            .iter()
            .filter_map(|r| (r.target() == &ReferenceTarget::Symbol(root)).then_some(r.span())),
    );
    if !future && !ancestor_fact((parsed, names, typed, owned), flow, fact, storage, span)? {
        let resource = typed.is_resource_type(storage).ok_or_else(missing)?;
        let scope = resource::scope(parsed, plan.descriptor().statement(), origin, span)?;
        if !resource
            || (!resource::normal_point(parsed, scope, span, fact.point())
                && !replacement_point(formation.as_ref(), fact.point()))
        {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
    }
    Ok(Some(Retained { source, formation }))
}

/// An active ancestor owns this other root, independently of the inner source.
fn ancestor_fact(
    frontend: (
        &ParsedFile,
        &NameResolution,
        &TypedFile,
        &OwnershipCheckedFile,
    ),
    flow: source_owner::Flow,
    fact: DropFact,
    storage: TypeId,
    span: Span,
) -> Result<bool, LoweringError> {
    use lang_frontend::ownership_checking::CleanupOwnerValue;
    let (parsed, names, typed, owned) = frontend;
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let root = match flow.source {
        SourceIdentity::Parameter(root) | SourceIdentity::Formed { symbol: root, .. } => root,
        _ => return Err(missing()),
    };
    for ancestor in owned.map_iterations() {
        if ancestor.descriptor().statement() == flow.statement
            || ancestor.source_access() != MapIterationSourceAccess::Owned
            || !matches!(ancestor.source(),Some(LoanTarget::Place(place)) if place.is_root()&&place.root()==root)
        {
            continue;
        }
        let node = parsed
            .ast()
            .statements()
            .get(ancestor.descriptor().statement())
            .map_err(|_| missing())?;
        let Statement::For { body, .. } = node.payload() else {
            return Err(missing());
        };
        let body = parsed
            .ast()
            .statements()
            .get(*body)
            .map_err(|_| missing())?
            .span();
        if body.source_id() != span.source_id()
            || body.start() > span.start()
            || span.end() > body.end()
        {
            continue;
        }
        if ancestor.descriptor().source_type() != storage {
            return Err(missing());
        }
        let parent_flow = source_owner::Flow {
            statement: ancestor.descriptor().statement(),
            source: flow.source,
            span: node.span(),
            late: None,
        };
        if ancestor
            .exits()
            .iter()
            .any(|exit| exit.point() == fact.point())
            && conditional::source_drop(owned, parent_flow, fact.point())? == Some(fact)
        {
            source_owner::flow_condition(
                parsed,
                names,
                owned,
                parent_flow,
                fact.owner().ok_or_else(missing)?,
                span,
            )?;
            return Ok(true);
        }
        if typed.is_resource_type(storage) != Some(true) {
            continue;
        }
        let exit = source_owner::phi(
            owned.cleanup_conditions(),
            ancestor,
            root,
            IterationPhiBoundary::Exit,
            node.span(),
        )?;
        let live =
            source_owner::flow_condition(parsed, names, owned, parent_flow, exit.owner(), span)?;
        let origin = names
            .symbols()
            .get(root.index())
            .ok_or_else(missing)?
            .span();
        let scope = resource::scope(
            parsed,
            ancestor.descriptor().statement(),
            origin,
            node.span(),
        )?;
        if fact.owner() != Some(exit.owner())
            || fact.condition() != Some(live)
            || !resource::normal_point(parsed, scope, node.span(), fact.point())
        {
            continue;
        }
        if !matches!(owned.cleanup_conditions().owner_value(exit.owner()),Some(CleanupOwnerValue::IterationPhi{statement,boundary:IterationPhiBoundary::Exit,symbol,..})
            if *statement==ancestor.descriptor().statement()&&*symbol==root)
        {
            return Err(missing());
        }
        let cleanup = owned
            .cleanup_steps()
            .iter()
            .filter_map(|(point, action)| (*point == fact.point()).then_some(*action))
            .collect::<Vec<_>>();
        conditional::validate_drop_ledger(owned, fact.point(), &cleanup, span)?;
        return Ok(true);
    }
    Ok(false)
}
