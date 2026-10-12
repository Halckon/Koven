//! Bounded non-Resource Map phi proof. Presence alone does not prove an SSA carrier.
mod carrier;

use lang_frontend::{
    ast::StatementId,
    name_resolution::SymbolId,
    ownership_checking::{
        CleanupCondition, CleanupOwnerValue, CleanupOwnerValueId, DropFact, DropPoint, DropTarget,
        IterationCleanupAction, IterationClosurePhiBinding, IterationExitKind,
        IterationPhiBoundary, IterationPhiIncoming, IterationPhiIncomingBinding,
        IterationPhiIncomingKind, IterationPhiPresenceSource, LoanTarget, OwnershipCheckedFile,
    },
    type_checking::{IntrinsicTypeConstructor, TypeKind, TypedFile},
};

pub(super) use carrier::Carriers;

pub(super) fn validate_drops(
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
) -> Result<(), super::LoweringError> {
    if let Some(fact) = owned
        .drops()
        .iter()
        .find(|fact| fact.condition().is_some() && !supported_drop(typed, owned, **fact))
    {
        return Err(super::error(
            super::LoweringErrorKind::UnsupportedNode,
            fact.value_origin(),
        ));
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct OwnerPlan {
    statement: StatementId,
    symbol: SymbolId,
    seed: CleanupOwnerValueId,
    header: CleanupOwnerValueId,
    exit: CleanupOwnerValueId,
    exit_live: bool,
    drop: DropFact,
}

pub(super) fn supported_drop(
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    fact: DropFact,
) -> bool {
    super::constant_presence::constant_drop(typed, owned, fact)
        || owner_plan(typed, owned, fact).is_some()
}

fn owner_plan(
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    fact: DropFact,
) -> Option<OwnerPlan> {
    let DropTarget::Named(symbol) = fact.target() else {
        return None;
    };
    let ty = typed.symbol_type(symbol)?;
    if typed.is_resource_type(ty) != Some(false)
        || !matches!(
            typed.types().get(ty),
            Some(TypeKind::Intrinsic {
                constructor: IntrinsicTypeConstructor::Map | IntrinsicTypeConstructor::MutableMap,
                ..
            })
        )
    {
        return None;
    }
    let conditions = owned.cleanup_conditions();
    let Some(CleanupOwnerValue::IterationPhi {
        statement,
        symbol: identity,
        ..
    }) = conditions.owner_value(fact.owner()?)
    else {
        return None;
    };
    if *identity != symbol {
        return None;
    }
    let plan = owned.iteration(*statement)?;
    if matches!(plan.source(), LoanTarget::Place(place) if place.root() == symbol)
        || plan.exits().len() != 2
        || plan.closure_phi_incomings().len() != 3
    {
        return None;
    }
    let header =
        unique(plan.closure_phis().iter().filter(|phi| {
            phi.symbol() == symbol && phi.boundary() == IterationPhiBoundary::Header
        }))?;
    let exit = unique(
        plan.closure_phis()
            .iter()
            .filter(|phi| phi.symbol() == symbol && phi.boundary() == IterationPhiBoundary::Exit),
    )?;
    for phi in [header, exit] {
        if !phi.root_nodes().is_empty()
            || !phi.capture_layout().is_empty()
            || !phi.origins().is_empty()
            || !matches!(conditions.owner_value(phi.owner()), Some(CleanupOwnerValue::IterationPhi {
                statement: owner_statement, symbol: owner_symbol, boundary, ..
            }) if owner_statement == statement && *owner_symbol == symbol && *boundary == phi.boundary())
        {
            return None;
        }
    }
    if header.owner() == exit.owner()
        || header.availability_selector() == exit.availability_selector()
    {
        return None;
    }
    let entry_edge = unique(
        plan.closure_phi_incomings()
            .iter()
            .filter(|edge| edge.kind() == IterationPhiIncomingKind::Entry),
    )?;
    let back_edge = unique(
        plan.closure_phi_incomings()
            .iter()
            .filter(|edge| edge.kind() == IterationPhiIncomingKind::Fallthrough),
    )?;
    let exhausted_edge = unique(
        plan.closure_phi_incomings()
            .iter()
            .filter(|edge| edge.kind() == IterationPhiIncomingKind::Exhaustion),
    )?;
    let fallthrough = unique(
        plan.exits()
            .iter()
            .filter(|exit| exit.kind() == IterationExitKind::Fallthrough),
    )?;
    let exhaustion = unique(
        plan.exits()
            .iter()
            .filter(|exit| exit.kind() == IterationExitKind::Exhaustion),
    )?;
    if entry_edge.point() != DropPoint::AfterExpression(plan.descriptor().source())
        || back_edge.point() != fallthrough.point()
        || exhausted_edge.point() != DropPoint::LoopExit(*statement)
        || exhaustion.point() != exhausted_edge.point()
        || !matches!(
            conditions.get(fallthrough.condition()),
            Some(CleanupCondition::Always)
        )
        || !matches!(
            conditions.get(exhaustion.condition()),
            Some(CleanupCondition::Always)
        )
    {
        return None;
    }
    let entry = incoming(owned, entry_edge, header)?;
    let back = incoming(owned, back_edge, header)?;
    let exhausted = incoming(owned, exhausted_edge, exit)?;
    let seed = unique(entry.values().iter())?;
    if !matches!(
        conditions.get(entry.available_when()),
        Some(CleanupCondition::Always)
    ) || !matches!(
        conditions.get(seed.condition()),
        Some(CleanupCondition::Always)
    ) || !matches!(
        conditions.owner_value(seed.source()),
        Some(CleanupOwnerValue::Expression { .. } | CleanupOwnerValue::Parameter { .. })
    ) || !one_value(back, header.owner(), header.availability_condition())
    {
        return None;
    }
    // A real Entry grounds Header's self-backedge. No missing or dynamic incoming is accepted.
    if !super::constant_presence::prove_graph(
        owned,
        header.owner(),
        symbol,
        Some(header.availability_condition()),
    ) {
        return None;
    }
    let exit_live = one_value(exhausted, header.owner(), header.availability_condition());
    if !exit_live
        && (!exhausted.values().is_empty()
            || !matches!(
                conditions.get(exhausted.available_when()),
                Some(CleanupCondition::Never)
            ))
    {
        return None;
    }
    let expected = if exit_live { exit } else { header };
    if fact.owner() != Some(expected.owner())
        || fact.condition() != Some(expected.availability_condition())
        || (!exit_live && fact.point() != exhaustion.point())
        || (exit_live && fact.point() == exhaustion.point())
    {
        return None;
    }
    // The first slice has one terminal cleanup. Conditional moves/replacements are not selections.
    if unique(owned.drops().iter().filter(|candidate| {
        candidate.owner() == Some(header.owner()) || candidate.owner() == Some(exit.owner())
    })) != Some(&fact)
    {
        return None;
    }
    let header_drops = exhaustion
        .actions()
        .iter()
        .filter(|action| {
            matches!(action,
                IterationCleanupAction::Drop(drop) if drop.owner() == Some(header.owner())
            )
        })
        .count();
    if header_drops != usize::from(!exit_live) {
        return None;
    }
    Some(OwnerPlan {
        statement: *statement,
        symbol,
        seed: seed.source(),
        header: header.owner(),
        exit: exit.owner(),
        exit_live,
        drop: fact,
    })
}

fn incoming<'a>(
    owned: &OwnershipCheckedFile,
    edge: &'a IterationPhiIncoming,
    phi: &IterationClosurePhiBinding,
) -> Option<&'a IterationPhiIncomingBinding> {
    if edge.boundary() != phi.boundary()
        || !matches!(
            owned.cleanup_conditions().get(edge.condition()),
            Some(CleanupCondition::Always)
        )
    {
        return None;
    }
    let binding = unique(
        edge.bindings()
            .iter()
            .filter(|binding| binding.target() == phi.owner()),
    )?;
    if binding.availability_selector() != phi.availability_selector()
        || binding.presence_source() != IterationPhiPresenceSource::StaticConditions
        || !binding.capture_slots_to_clear().is_empty()
        || !binding.root_sources().is_empty()
        || !binding.selector_writes().is_empty()
        || !binding.origins().is_empty()
    {
        return None;
    }
    Some(binding)
}

fn one_value(
    binding: &IterationPhiIncomingBinding,
    owner: CleanupOwnerValueId,
    condition: lang_frontend::ownership_checking::CleanupConditionId,
) -> bool {
    binding.available_when() == condition
        && binding.values().len() == 1
        && binding.values()[0].source() == owner
        && binding.values()[0].condition() == condition
}

fn unique<T>(mut items: impl Iterator<Item = T>) -> Option<T> {
    let first = items.next()?;
    items.next().is_none().then_some(first)
}
