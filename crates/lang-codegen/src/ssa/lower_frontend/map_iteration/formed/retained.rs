//! Other live Map roots keep their own epoch and terminal fact across a provider.
use super::*;
use lang_frontend::ownership_checking::{DropFact, DropTarget, IterationPhiBoundary};

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
) -> Result<Option<Formation<SymbolId>>, LoweringError> {
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
    let formation = formation(parsed, names, typed, owned, root, storage, span)?;
    if !drops(parsed, owned, root, &formation)?.contains(&fact) {
        return Ok(None);
    }
    let flow = source_owner::Flow {
        statement: plan.descriptor().statement(),
        source: SourceIdentity::Formed {
            symbol: root,
            birth: formation.birth,
        },
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
    if fact.value_origin() != formation.declaration
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
    if !future {
        let resource = typed.is_resource_type(storage).ok_or_else(missing)?;
        let scope = resource::scope(
            parsed,
            plan.descriptor().statement(),
            formation.declaration,
            span,
        )?;
        if !resource || !resource::normal_point(parsed, scope, span, fact.point()) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
    }
    Ok(Some(formation))
}
