//! Bounded non-Resource Map phi proof. Presence alone does not prove an SSA carrier.
mod carrier;

use lang_frontend::{
    ast::{ExpressionId, StatementId},
    name_resolution::SymbolId,
    ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupConditions, CleanupOwnerValue,
        CleanupOwnerValueId, CleanupSelection, CleanupSelectorId, DropFact, DropPoint, DropTarget,
        IterationCleanupAction, IterationClosurePhiBinding, IterationExitKind,
        IterationPhiBoundary, IterationPhiIncoming, IterationPhiIncomingBinding,
        IterationPhiIncomingKind, IterationPhiPresenceSource, LoanTarget, OwnershipCheckedFile,
    },
    type_checking::{IntrinsicTypeConstructor, IterationProvider, TypeKind, TypedFile},
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
    break_expression: Option<ExpressionId>,
    break_drop: Option<DropFact>,
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
    if fact.capture_slot().is_some() || fact.instance_address().is_some() {
        return None;
    }
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
    if !matches!(
        plan.descriptor().provider(),
        IterationProvider::Sequential(_)
    ) || matches!(plan.source(), LoanTarget::Place(place) if place.root() == symbol)
    {
        return None;
    }
    // Only one conditional break in addition to Entry/Fallthrough/Exhaustion.
    // In particular, Continue and Return never acquire this owner proof.
    let breaks = plan
        .exits()
        .iter()
        .filter_map(|exit| match exit.kind() {
            IterationExitKind::Break(expression) => Some((expression, exit)),
            _ => None,
        })
        .collect::<Vec<_>>();
    if breaks.len() > 1
        || plan.exits().len() != 2 + breaks.len()
        || plan.closure_phi_incomings().len() != 3 + breaks.len()
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
        || exhaustion.condition() != exhausted_edge.condition()
        || back_edge.condition() != fallthrough.condition()
        || !matches!(
            conditions.get(entry_edge.condition()),
            Some(CleanupCondition::Always)
        )
        || !matches!(
            conditions.get(exhaustion.condition()),
            Some(CleanupCondition::Always)
        )
    {
        return None;
    }
    let break_pair = if let Some(&(expression, break_exit)) = breaks.first() {
        let edge = unique(
            plan.closure_phi_incomings()
                .iter()
                .filter(|edge| edge.kind() == IterationPhiIncomingKind::Break(expression)),
        )?;
        if edge.point() != DropPoint::ControlTransfer(expression)
            || break_exit.point() != edge.point()
            || break_exit.condition() != edge.condition()
            || !complementary_paths(conditions, edge.condition(), back_edge.condition())
        {
            return None;
        }
        Some((expression, break_exit, edge))
    } else {
        if !matches!(
            conditions.get(back_edge.condition()),
            Some(CleanupCondition::Always)
        ) {
            return None;
        }
        None
    };
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
    ) || !carried_value(conditions, back, header, back_edge.condition())
    {
        return None;
    }
    // The exact Always Entry and sole presence-and-path self-backedge establish
    // Header presence inductively. Do not widen the old Resource/source proof.
    for phi in [header, exit] {
        if !presence_on_path(
            conditions,
            phi.availability_condition(),
            phi.availability_selector(),
            entry_edge.condition(),
        ) {
            return None;
        }
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
    let break_drop = if let Some((_, break_exit, edge)) = break_pair {
        let binding = incoming(owned, edge, exit)?;
        if exit_live {
            if !carried_value(conditions, binding, header, edge.condition()) {
                return None;
            }
            None
        } else {
            if !binding.values().is_empty()
                || !matches!(
                    conditions.get(binding.available_when()),
                    Some(CleanupCondition::Never)
                )
            {
                return None;
            }
            Some(exit_drop(conditions, break_exit, header)?)
        }
    } else {
        None
    };
    let drop = if exit_live {
        let drop = *unique(
            owned
                .drops()
                .iter()
                .filter(|drop| drop.owner() == Some(exit.owner())),
        )?;
        if drop.target() != DropTarget::Named(symbol)
            || drop.condition() != Some(exit.availability_condition())
            || drop.point() == exhaustion.point()
        {
            return None;
        }
        drop
    } else {
        exit_drop(conditions, exhaustion, header)?
    };
    let drops = owned
        .drops()
        .iter()
        .filter(|candidate| {
            candidate.owner() == Some(header.owner()) || candidate.owner() == Some(exit.owner())
        })
        .collect::<Vec<_>>();
    if drops.len() != 1 + usize::from(break_drop.is_some())
        || !drops.contains(&&drop)
        || break_drop.is_some_and(|drop| !drops.contains(&&drop))
        || drops
            .iter()
            .any(|drop| drop.capture_slot().is_some() || drop.instance_address().is_some())
        || (fact != drop && Some(fact) != break_drop)
    {
        return None;
    }
    // A live owner is not consumed by either exit cleanup.
    if exit_live
        && plan.exits().iter().any(|exit_plan| {
            exit_plan.actions().iter().any(|action| {
                let IterationCleanupAction::Drop(drop) = action else {
                    return false;
                };
                drop.owner() == Some(header.owner()) || drop.owner() == Some(exit.owner())
            })
        })
    {
        return None;
    }
    Some(OwnerPlan {
        statement: *statement,
        symbol,
        seed: seed.source(),
        header: header.owner(),
        exit: exit.owner(),
        exit_live,
        drop,
        break_expression: break_pair.map(|(expression, _, _)| expression),
        break_drop,
    })
}

fn incoming<'a>(
    _owned: &OwnershipCheckedFile,
    edge: &'a IterationPhiIncoming,
    phi: &IterationClosurePhiBinding,
) -> Option<&'a IterationPhiIncomingBinding> {
    if edge.boundary() != phi.boundary() {
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

// Both paths must be the two outcomes of one published ordinary branch selector.
// This does not re-derive any control-flow fact from syntax.
fn complementary_paths(
    conditions: &CleanupConditions,
    left: CleanupConditionId,
    right: CleanupConditionId,
) -> bool {
    let (
        Some(CleanupCondition::Choice {
            selector: a,
            branches: x,
        }),
        Some(CleanupCondition::Choice {
            selector: b,
            branches: y,
        }),
    ) = (conditions.get(left), conditions.get(right))
    else {
        return false;
    };
    a == b
        && conditions.selector(*a).is_some_and(|selector| {
            selector.control().is_some()
                && selector.selection() == CleanupSelection::Branch
                && selector.branch_count() == 2
        })
        && x.len() == 2
        && y.len() == 2
        && x.iter().zip(y).all(|(x, y)| {
            matches!(
                (conditions.get(*x), conditions.get(*y)),
                (
                    Some(CleanupCondition::Always),
                    Some(CleanupCondition::Never)
                ) | (
                    Some(CleanupCondition::Never),
                    Some(CleanupCondition::Always)
                )
            )
        })
        && x[0] != x[1]
}

fn presence_on_path(
    conditions: &CleanupConditions,
    condition: CleanupConditionId,
    selector: CleanupSelectorId,
    path: CleanupConditionId,
) -> bool {
    let Some(CleanupCondition::Choice {
        selector: actual,
        branches,
    }) = conditions.get(condition)
    else {
        return false;
    };
    *actual == selector
        && branches.len() == 2
        && matches!(conditions.get(branches[0]), Some(CleanupCondition::Never))
        && branches[1] == path
}

fn carried_value(
    conditions: &CleanupConditions,
    binding: &IterationPhiIncomingBinding,
    header: &IterationClosurePhiBinding,
    path: CleanupConditionId,
) -> bool {
    binding.values().len() == 1
        && binding.values()[0].source() == header.owner()
        && binding.values()[0].condition() == binding.available_when()
        && presence_on_path(
            conditions,
            binding.available_when(),
            header.availability_selector(),
            path,
        )
}

fn exit_drop(
    conditions: &CleanupConditions,
    exit: &lang_frontend::ownership_checking::IterationExitPlan,
    header: &IterationClosurePhiBinding,
) -> Option<DropFact> {
    let drop = unique(exit.actions().iter().filter_map(|action| match action {
        IterationCleanupAction::Drop(drop) if drop.owner() == Some(header.owner()) => Some(*drop),
        _ => None,
    }))?;
    (drop.point() == exit.point()
        && drop.target() == DropTarget::Named(header.symbol())
        && presence_on_path(
            conditions,
            drop.condition()?,
            header.availability_selector(),
            exit.condition(),
        ))
    .then_some(drop)
}
