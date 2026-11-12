//! A source-only induction. No condition is rewritten and no Resource gains this proof.
use lang_frontend::{
    name_resolution::{NameResolution, ScopeKind, SymbolKind},
    ownership_checking::{
        CleanupCondition, CleanupConditionId, CleanupConditions, CleanupOwnerValue,
        CleanupSelection, CleanupSelectorId, CleanupSelectorSource, DropFact, DropPoint,
        DropTarget, IterationCleanupAction, IterationClosurePhiBinding, IterationExitKind,
        IterationOwnershipPlan, IterationPhiBoundary, IterationPhiIncoming,
        IterationPhiIncomingBinding, IterationPhiIncomingKind, IterationPhiPresenceSource,
        LoanTarget, OwnershipBindingKind, OwnershipCheckedFile,
    },
    type_checking::{IterationProvider, ParameterMode, TypedFile},
};
use std::collections::BTreeSet;

pub(super) fn prove(
    parsed: &lang_frontend::parser::ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    fact: DropFact,
) -> Option<()> {
    let DropTarget::Named(symbol) = fact.target() else {
        return None;
    };
    if fact.capture_slot().is_some() || fact.instance_address().is_some() {
        return None;
    }
    let conditions = owned.cleanup_conditions();
    let owner = fact.owner()?;
    let CleanupOwnerValue::IterationPhi {
        statement,
        symbol: identity,
        boundary,
        ..
    } = conditions.owner_value(owner)?
    else {
        return None;
    };
    if *identity != symbol {
        return None;
    }
    let plan = owned.iteration(*statement)?;
    let ty = typed.symbol_type(symbol)?;
    if !matches!(
        plan.descriptor().provider(),
        IterationProvider::Sequential(_)
    ) || plan.descriptor().source_type() != ty
        || typed.is_resource_type(ty) != Some(false)
        || !matches!(plan.source(), Some(LoanTarget::Place(place)) if place.is_root() && place.root() == symbol)
        || owned
            .borrow_results()
            .bindings()
            .iter()
            .any(|binding| binding.binding() == symbol)
    {
        return None;
    }
    let named = names.symbols().get(symbol.index())?;
    let local = named.kind() == SymbolKind::Variable
        && matches!(
            names.scopes().get(named.scope().index())?.kind(),
            ScopeKind::Function
                | ScopeKind::Lambda
                | ScopeKind::Block
                | ScopeKind::ControlBody
                | ScopeKind::Loop
        );
    let parameter = named.kind() == SymbolKind::ValueParameter
        && typed.parameter_mode(symbol) == Some(ParameterMode::Value)
        && unique(
            owned
                .bindings()
                .iter()
                .filter(|binding| binding.symbol() == symbol),
        )
        .is_some_and(|binding| binding.kind() == OwnershipBindingKind::Owned);
    if !local && !parameter {
        return None;
    }

    let header =
        unique(plan.closure_phis().iter().filter(|phi| {
            phi.symbol() == symbol && phi.boundary() == IterationPhiBoundary::Header
        }))?;
    let target = unique(
        plan.closure_phis()
            .iter()
            .filter(|phi| phi.owner() == owner),
    )?;
    if target.symbol() != symbol || target.boundary() != *boundary {
        return None;
    }
    canonical_phi(conditions, plan, header)?;
    canonical_phi(conditions, plan, target)?;
    if fact.condition() != Some(target.availability_condition()) {
        return None;
    }

    // A finite single-provider control shape; every real incoming is checked below.
    if plan.exits().len() != 3 || plan.closure_phi_incomings().len() != 4 {
        return None;
    }
    let continuing = unique(
        plan.exits()
            .iter()
            .filter(|exit| matches!(exit.kind(), IterationExitKind::Continue(_))),
    )?;
    let IterationExitKind::Continue(expression) = continuing.kind() else {
        return None;
    };
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
    let entry = unique(
        plan.closure_phi_incomings()
            .iter()
            .filter(|edge| edge.kind() == IterationPhiIncomingKind::Entry),
    )?;
    if entry.point() != DropPoint::AfterExpression(plan.descriptor().source())
        || !always(conditions, entry.condition())
    {
        return None;
    }
    let entry_binding = incoming(entry, header)?;
    let seed = unique(entry_binding.values().iter())?;
    if !always(conditions, entry_binding.available_when()) || !always(conditions, seed.condition())
    {
        return None;
    }
    match conditions.owner_value(seed.source())? {
        CleanupOwnerValue::Expression { expression, .. } if local => {
            let initializer = unique(parsed.ast().items().iter().filter_map(|(_, item)| {
                let lang_frontend::parser::Item::Variable {
                    name, initializer, ..
                } = item.payload()
                else {
                    return None;
                };
                (crate::ssa::lowering_support::present_name(*name) == Some(named.span()))
                    .then_some(*initializer)
            }))?;
            // Only the existing transparent Group normalization is used. An assignment
            // or alias seed cannot pretend to be this declaration's initializer.
            let initializer =
                crate::ssa::borrow_result_support::ungroup(parsed, initializer, named.span())
                    .ok()?;
            if initializer != *expression {
                return None;
            }
        }
        CleanupOwnerValue::Parameter {
            symbol: seed_symbol,
            ..
        } if parameter && *seed_symbol == symbol => {}
        _ => return None,
    }
    // Base: the first Header contains the real seed. Induction: only this Header
    // selector is present on its outgoing edges, never an Exit selector.
    for (kind, exit) in [
        (IterationPhiIncomingKind::Continue(expression), continuing),
        (IterationPhiIncomingKind::Fallthrough, fallthrough),
    ] {
        let edge = unique(
            plan.closure_phi_incomings()
                .iter()
                .filter(|edge| edge.kind() == kind),
        )?;
        if edge.point() != exit.point()
            || edge.condition() != exit.condition()
            || (kind == IterationPhiIncomingKind::Continue(expression)
                && edge.point() != DropPoint::ControlTransfer(expression))
        {
            return None;
        }
        carried(conditions, edge, header, header)?;
    }
    // Require the new shape to be conditional, with actual reachable normal fallthrough.
    if always(conditions, continuing.condition())
        || never(conditions, continuing.condition())
        || never(conditions, fallthrough.condition())
    {
        return None;
    }
    let exhausted = unique(
        plan.closure_phi_incomings()
            .iter()
            .filter(|edge| edge.kind() == IterationPhiIncomingKind::Exhaustion),
    )?;
    if exhausted.boundary() != IterationPhiBoundary::Exit
        || exhausted.point() != DropPoint::LoopExit(*statement)
        || exhaustion.point() != exhausted.point()
        || exhaustion.condition() != exhausted.condition()
        || !always(conditions, exhausted.condition())
    {
        return None;
    }

    match boundary {
        IterationPhiBoundary::Exit => {
            // Header was proved first; Exit may not assume its own presence.
            carried(conditions, exhausted, target, header)?;
            if fact.point() == exhaustion.point() {
                return None;
            }
        }
        IterationPhiBoundary::Header => {
            // A dead source is dropped after EndSource; an absent Exit is legitimate.
            if fact.point() != exhaustion.point() {
                return None;
            }
            let finish = unique(
                exhaustion
                    .actions()
                    .iter()
                    .enumerate()
                    .filter(|(_, action)| {
                        **action == IterationCleanupAction::FinishProvider(*statement)
                    }),
            )?
            .0;
            let end = unique(
                exhaustion
                    .actions()
                    .iter()
                    .enumerate()
                    .filter(|(_, action)| {
                        **action == IterationCleanupAction::EndSource(*statement)
                    }),
            )?
            .0;
            let drop = unique(
                exhaustion
                    .actions()
                    .iter()
                    .enumerate()
                    .filter(|(_, action)| **action == IterationCleanupAction::Drop(fact)),
            )?
            .0;
            if !(finish < end && end < drop) {
                return None;
            }
            let exit = unique(plan.closure_phis().iter().filter(|phi| {
                phi.symbol() == symbol && phi.boundary() == IterationPhiBoundary::Exit
            }))?;
            canonical_phi(conditions, plan, exit)?;
            let absent = incoming(exhausted, exit)?;
            if !absent.values().is_empty() || !never(conditions, absent.available_when()) {
                return None;
            }
        }
    }
    Some(())
}

pub(super) fn unique<T>(mut items: impl Iterator<Item = T>) -> Option<T> {
    let first = items.next()?;
    items.next().is_none().then_some(first)
}
pub(super) fn always(table: &CleanupConditions, id: CleanupConditionId) -> bool {
    matches!(table.get(id), Some(CleanupCondition::Always))
}
pub(super) fn never(table: &CleanupConditions, id: CleanupConditionId) -> bool {
    matches!(table.get(id), Some(CleanupCondition::Never))
}

pub(super) fn canonical_phi(
    table: &CleanupConditions,
    plan: &IterationOwnershipPlan,
    phi: &IterationClosurePhiBinding,
) -> Option<()> {
    if !phi.root_nodes().is_empty() || !phi.capture_layout().is_empty() || !phi.origins().is_empty()
    {
        return None;
    }
    let statement = plan.descriptor().statement();
    if !matches!(table.owner_value(phi.owner()),
        Some(CleanupOwnerValue::IterationPhi { statement: s, symbol, boundary, .. })
        if *s == statement && *symbol == phi.symbol() && *boundary == phi.boundary())
    {
        return None;
    }
    // Slots include prior capture origins; require their layout to be empty rather
    // than guessing how many runtime capture slots such a prior binding would own.
    let mut slot = 0;
    for preceding in plan
        .closure_phis()
        .iter()
        .filter(|p| p.boundary() == phi.boundary())
    {
        if preceding.owner() == phi.owner() {
            break;
        }
        if !preceding.origins().is_empty() {
            return None;
        }
        slot += 1;
    }
    let selector = table.selector(phi.availability_selector())?;
    if selector.source()
        != (CleanupSelectorSource::IterationPhi {
            statement,
            boundary: phi.boundary(),
            slot,
        })
        || selector.selection() != CleanupSelection::IterationPresence
        || selector.branch_count() != 2
    {
        return None;
    }
    match table.get(phi.availability_condition())? {
        CleanupCondition::Choice { selector, branches }
            if *selector == phi.availability_selector()
                && branches.len() == 2
                && never(table, branches[0])
                && always(table, branches[1]) =>
        {
            Some(())
        }
        _ => None,
    }
}

pub(super) fn incoming<'a>(
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
pub(super) fn carried(
    table: &CleanupConditions,
    edge: &IterationPhiIncoming,
    target: &IterationClosurePhiBinding,
    header: &IterationClosurePhiBinding,
) -> Option<()> {
    let binding = incoming(edge, target)?;
    let value = unique(binding.values().iter())?;
    if value.source() != header.owner() {
        return None;
    }
    [binding.available_when(), value.condition()]
        .into_iter()
        .all(|required| {
            implies_present(
                table,
                edge.condition(),
                required,
                header.availability_selector(),
                &mut BTreeSet::new(),
            )
        })
        .then_some(())
}

// Structural implication in a fixed, source-only induction environment. Recursion
// is finite and cycle-rejecting; other selectors are neither evaluated nor assumed.
fn implies_present(
    table: &CleanupConditions,
    path: CleanupConditionId,
    required: CleanupConditionId,
    header: CleanupSelectorId,
    visiting: &mut BTreeSet<(CleanupConditionId, CleanupConditionId)>,
) -> bool {
    if !visiting.insert((path, required)) {
        return false;
    }
    let result = match (table.get(path), table.get(required)) {
        (Some(CleanupCondition::Choice { selector, branches }), _)
            if *selector == header && branches.len() == 2 =>
        {
            implies_present(table, branches[1], required, header, visiting)
        }
        (_, Some(CleanupCondition::Choice { selector, branches }))
            if *selector == header && branches.len() == 2 =>
        {
            implies_present(table, path, branches[1], header, visiting)
        }
        (Some(_), Some(_)) if path == required => true,
        (_, Some(CleanupCondition::Always)) | (Some(CleanupCondition::Never), _) => true,
        (
            Some(CleanupCondition::Choice {
                selector: a,
                branches: x,
            }),
            Some(CleanupCondition::Choice {
                selector: b,
                branches: y,
            }),
        ) if a == b && x.len() == y.len() => x
            .iter()
            .zip(y)
            .all(|(x, y)| implies_present(table, *x, *y, header, visiting)),
        (Some(CleanupCondition::Choice { branches, .. }), _) => branches
            .iter()
            .all(|branch| implies_present(table, *branch, required, header, visiting)),
        (_, Some(CleanupCondition::Choice { branches, .. })) => branches
            .iter()
            .all(|branch| implies_present(table, path, *branch, header, visiting)),
        _ => false,
    };
    visiting.remove(&(path, required));
    result
}

pub(in crate::ssa::lower_frontend::nullable_if_cleanup::constant_presence) mod source_return {
    //! A distinct Return proof: terminating paths never become Header or Exit incoming edges.
    use super::{always, canonical_phi, carried, incoming, never, unique};
    use lang_frontend::{
        name_resolution::{NameResolution, ScopeKind, SymbolKind},
        ownership_checking::{
            CleanupCondition, CleanupConditionId, CleanupConditions, CleanupOwnerValue,
            CleanupSelection, DropFact, DropPoint, DropTarget, IterationCleanupAction,
            IterationExitKind, IterationExitPlan, IterationPhiBoundary, IterationPhiIncomingKind,
            LoanTarget, OwnershipBindingKind, OwnershipCheckedFile,
        },
        type_checking::{IterationProvider, ParameterMode, TypedFile},
    };

    pub(in crate::ssa::lower_frontend::nullable_if_cleanup::constant_presence) fn prove(
        parsed: &lang_frontend::parser::ParsedFile,
        names: &NameResolution,
        typed: &TypedFile,
        owned: &OwnershipCheckedFile,
        fact: DropFact,
    ) -> Option<()> {
        let DropTarget::Named(symbol) = fact.target() else {
            return None;
        };
        if fact.capture_slot().is_some() || fact.instance_address().is_some() {
            return None;
        };
        let conditions = owned.cleanup_conditions();
        let owner = fact.owner()?;
        let CleanupOwnerValue::IterationPhi {
            statement,
            symbol: identity,
            boundary,
            ..
        } = conditions.owner_value(owner)?
        else {
            return None;
        };
        if *identity != symbol {
            return None;
        }
        let plan = owned.iteration(*statement)?;
        let ty = typed.symbol_type(symbol)?;
        if !matches!(
            plan.descriptor().provider(),
            IterationProvider::Sequential(_)
        ) || plan.descriptor().source_type() != ty
            || typed.is_resource_type(ty) != Some(false)
            || !matches!(plan.source(), Some(LoanTarget::Place(place)) if place.is_root() && place.root() == symbol)
            || owned
                .borrow_results()
                .bindings()
                .iter()
                .any(|binding| binding.binding() == symbol)
        {
            return None;
        }
        let named = names.symbols().get(symbol.index())?;
        let local = named.kind() == SymbolKind::Variable
            && matches!(
                names.scopes().get(named.scope().index())?.kind(),
                ScopeKind::Function
                    | ScopeKind::Lambda
                    | ScopeKind::Block
                    | ScopeKind::ControlBody
                    | ScopeKind::Loop
            );
        let parameter = named.kind() == SymbolKind::ValueParameter
            && typed.parameter_mode(symbol) == Some(ParameterMode::Value)
            && unique(
                owned
                    .bindings()
                    .iter()
                    .filter(|binding| binding.symbol() == symbol),
            )
            .is_some_and(|binding| binding.kind() == OwnershipBindingKind::Owned);
        if !local && !parameter {
            return None;
        }

        let header = unique(plan.closure_phis().iter().filter(|phi| {
            phi.symbol() == symbol && phi.boundary() == IterationPhiBoundary::Header
        }))?;
        let target = unique(
            plan.closure_phis()
                .iter()
                .filter(|phi| phi.owner() == owner),
        )?;
        if target.symbol() != symbol || target.boundary() != *boundary {
            return None;
        }
        canonical_phi(conditions, plan, header)?;
        canonical_phi(conditions, plan, target)?;
        let exit =
            unique(plan.closure_phis().iter().filter(|phi| {
                phi.symbol() == symbol && phi.boundary() == IterationPhiBoundary::Exit
            }))?;
        canonical_phi(conditions, plan, exit)?;
        if header.owner() == exit.owner()
            || header.availability_selector() == exit.availability_selector()
            || plan.exits().len() != 3
            || plan.closure_phi_incomings().len() != 3
        {
            return None;
        }
        let returning = unique(
            plan.exits()
                .iter()
                .filter(|exit| matches!(exit.kind(), IterationExitKind::Return(_))),
        )?;
        let IterationExitKind::Return(expression) = returning.kind() else {
            return None;
        };
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
        if returning.point() != DropPoint::ControlTransfer(expression)
            || !complementary_paths(conditions, returning.condition(), fallthrough.condition())
        {
            return None;
        }
        let entry = unique(
            plan.closure_phi_incomings()
                .iter()
                .filter(|edge| edge.kind() == IterationPhiIncomingKind::Entry),
        )?;
        if entry.point() != DropPoint::AfterExpression(plan.descriptor().source())
            || !always(conditions, entry.condition())
        {
            return None;
        }
        let entry_binding = incoming(entry, header)?;
        let seed = unique(entry_binding.values().iter())?;
        if !always(conditions, entry_binding.available_when())
            || !always(conditions, seed.condition())
        {
            return None;
        }
        match conditions.owner_value(seed.source())? {
            CleanupOwnerValue::Expression { expression, .. } if local => {
                let initializer = unique(parsed.ast().items().iter().filter_map(|(_, item)| {
                    let lang_frontend::parser::Item::Variable {
                        name, initializer, ..
                    } = item.payload()
                    else {
                        return None;
                    };
                    (crate::ssa::lowering_support::present_name(*name) == Some(named.span()))
                        .then_some(*initializer)
                }))?;
                // Only the existing transparent Group normalization is used. An assignment
                // or alias seed cannot pretend to be this declaration's initializer.
                let initializer =
                    crate::ssa::borrow_result_support::ungroup(parsed, initializer, named.span())
                        .ok()?;
                if initializer != *expression {
                    return None;
                }
            }
            CleanupOwnerValue::Parameter {
                symbol: seed_symbol,
                ..
            } if parameter && *seed_symbol == symbol => {}
            _ => return None,
        }
        // Entry establishes Header presence; the sole actual backedge preserves it.
        let back = unique(
            plan.closure_phi_incomings()
                .iter()
                .filter(|edge| edge.kind() == IterationPhiIncomingKind::Fallthrough),
        )?;
        if back.point() != fallthrough.point() || back.condition() != fallthrough.condition() {
            return None;
        }
        carried(conditions, back, header, header)?;
        let exhausted = unique(
            plan.closure_phi_incomings()
                .iter()
                .filter(|edge| edge.kind() == IterationPhiIncomingKind::Exhaustion),
        )?;
        if exhausted.boundary() != IterationPhiBoundary::Exit
            || exhausted.point() != DropPoint::LoopExit(*statement)
            || exhaustion.point() != exhausted.point()
            || exhaustion.condition() != exhausted.condition()
            || !always(conditions, exhausted.condition())
        {
            return None;
        }

        // Return is independently closed over its published full cleanup sequence.
        let return_drop = unique(
            returning
                .actions()
                .iter()
                .filter_map(|action| match action {
                    IterationCleanupAction::Drop(drop)
                        if drop.target() == DropTarget::Named(symbol) =>
                    {
                        Some(*drop)
                    }
                    _ => None,
                }),
        )?;
        let Some(CleanupCondition::Choice { selector, branches }) =
            conditions.get(return_drop.condition()?)
        else {
            return None;
        };
        if return_drop.owner() != Some(header.owner())
            || return_drop.point() != returning.point()
            || return_drop.capture_slot().is_some()
            || return_drop.instance_address().is_some()
            || *selector != header.availability_selector()
            || branches.len() != 2
            || !never(conditions, branches[0])
            || branches[1] != returning.condition()
        {
            return None;
        }
        ordered_source_drop(returning, *statement, return_drop)?;

        // Exhaustion is separate from Return: either carry live Header into Exit,
        // or finish the source and leave the Exit explicitly absent.
        let exhausted_binding = incoming(exhausted, exit)?;
        let dead = exhausted_binding.values().is_empty()
            && never(conditions, exhausted_binding.available_when());
        if dead {
            let exhausted_drop = unique(exhaustion.actions().iter().filter_map(
                |action| match action {
                    IterationCleanupAction::Drop(drop)
                        if drop.target() == DropTarget::Named(symbol) =>
                    {
                        Some(*drop)
                    }
                    _ => None,
                },
            ))?;
            if exhausted_drop.owner() != Some(header.owner())
                || exhausted_drop.point() != exhaustion.point()
                || exhausted_drop.condition() != Some(header.availability_condition())
                || exhausted_drop.capture_slot().is_some()
                || exhausted_drop.instance_address().is_some()
            {
                return None;
            }
            ordered_source_drop(exhaustion, *statement, exhausted_drop)?;
            if fact != return_drop && fact != exhausted_drop {
                return None;
            }
        } else {
            carried(conditions, exhausted, exit, header)?;
            if exhaustion.actions().iter().any(|action| matches!(action, IterationCleanupAction::Drop(drop) if drop.target() == DropTarget::Named(symbol))) {
                return None;
            }
            if fact != return_drop
                && (owner != exit.owner()
                    || fact.condition() != Some(exit.availability_condition())
                    || fact.point() == exhaustion.point())
            {
                return None;
            }
        }
        Some(())
    }

    fn ordered_source_drop(
        exit: &IterationExitPlan,
        statement: lang_frontend::ast::StatementId,
        fact: DropFact,
    ) -> Option<()> {
        let position = |expected| {
            unique(
                exit.actions()
                    .iter()
                    .enumerate()
                    .filter_map(|(index, action)| (*action == expected).then_some(index)),
            )
        };
        let finish = position(IterationCleanupAction::FinishProvider(statement))?;
        let end = position(IterationCleanupAction::EndSource(statement))?;
        let drop = position(IterationCleanupAction::Drop(fact))?;
        (finish < end && end < drop).then_some(())
    }

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
}
