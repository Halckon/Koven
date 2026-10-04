//! Bounded presence proof from published incoming edges and a unique real entry owner.
use lang_frontend::ownership_checking::{
    CleanupCondition, CleanupConditionId, CleanupConditions, CleanupOwnerValue, DropFact,
    DropTarget, IterationPhiBoundary, IterationPhiIncomingKind, IterationPhiPresenceSource,
    LoanTarget, OwnershipCheckedFile,
};
use lang_frontend::type_checking::TypedFile;
use std::collections::BTreeSet;

fn implies(
    conditions: &CleanupConditions,
    path: CleanupConditionId,
    presence: CleanupConditionId,
    proven: &BTreeSet<CleanupConditionId>,
    visiting: &mut BTreeSet<(CleanupConditionId, CleanupConditionId)>,
) -> bool {
    if path == presence || proven.contains(&presence) {
        return true;
    }
    if !visiting.insert((path, presence)) {
        return false;
    }
    let always = CleanupCondition::Always;
    let path_node = if proven.contains(&path) {
        Some(&always)
    } else {
        conditions.get(path)
    };
    let result = match (path_node, conditions.get(presence)) {
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
            .all(|(x, y)| implies(conditions, *x, *y, proven, visiting)),
        (Some(CleanupCondition::Choice { branches, .. }), _) => branches
            .iter()
            .all(|branch| implies(conditions, *branch, presence, proven, visiting)),
        (_, Some(CleanupCondition::Choice { branches, .. })) => branches
            .iter()
            .all(|branch| implies(conditions, path, *branch, proven, visiting)),
        _ => false,
    };
    visiting.remove(&(path, presence));
    result
}

pub(super) fn constant_drop(
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    fact: DropFact,
) -> bool {
    let DropTarget::Named(symbol) = fact.target() else {
        return false;
    };
    let Some(owner) = fact.owner() else {
        return false;
    };
    let conditions = owned.cleanup_conditions();
    let Some(CleanupOwnerValue::IterationPhi {
        statement,
        symbol: identity,
        ..
    }) = conditions.owner_value(owner)
    else {
        return false;
    };
    if *identity != symbol {
        return false;
    }
    let Some(plan) = owned.iteration(*statement) else {
        return false;
    };
    let source =
        matches!(plan.source(), LoanTarget::Place(path) if path.is_root() && path.root() == symbol);
    let resource = typed
        .symbol_type(symbol)
        .is_some_and(|ty| typed.is_resource_type(ty) == Some(true));
    if !source && !resource {
        return false;
    }
    prove_graph(owned, owner, symbol, fact.condition())
}

fn prove_graph(
    owned: &OwnershipCheckedFile,
    owner: lang_frontend::ownership_checking::CleanupOwnerValueId,
    symbol: lang_frontend::name_resolution::SymbolId,
    expected: Option<CleanupConditionId>,
) -> bool {
    let conditions = owned.cleanup_conditions();
    let mut pending = vec![owner];
    let mut visited = BTreeSet::new();
    let mut origins = BTreeSet::new();
    let mut proven = BTreeSet::new();
    let mut requirements = Vec::new();
    while let Some(current) = pending.pop() {
        if !visited.insert(current) {
            continue;
        }
        let Some(CleanupOwnerValue::IterationPhi {
            statement,
            boundary,
            symbol: identity,
            ..
        }) = conditions.owner_value(current)
        else {
            return false;
        };
        if *identity != symbol {
            return false;
        }
        let Some(plan) = owned.iteration(*statement) else {
            return false;
        };
        let Some(target) = plan.closure_phis().iter().find(|phi| {
            phi.owner() == current && phi.boundary() == *boundary && phi.root_nodes().is_empty()
        }) else {
            return false;
        };
        if current == owner && expected != Some(target.availability_condition()) {
            return false;
        }
        let Some(header) = plan.closure_phis().iter().find(|phi| {
            phi.symbol() == symbol
                && phi.boundary() == IterationPhiBoundary::Header
                && phi.root_nodes().is_empty()
        }) else {
            return false;
        };
        pending.push(header.owner());
        proven.insert(header.availability_condition());
        proven.insert(target.availability_condition());
        let edges = plan
            .closure_phi_incomings()
            .iter()
            .filter(|edge| edge.boundary() == *boundary)
            .collect::<Vec<_>>();
        if edges.is_empty()
            || (*boundary == IterationPhiBoundary::Header
                && edges
                    .iter()
                    .filter(|edge| edge.kind() == IterationPhiIncomingKind::Entry)
                    .count()
                    != 1)
        {
            return false;
        }
        for edge in edges {
            let Some(binding) = edge
                .bindings()
                .iter()
                .find(|binding| binding.target() == current)
            else {
                return false;
            };
            if binding.presence_source() != IterationPhiPresenceSource::StaticConditions
                || binding.values().len() != 1
            {
                return false;
            }
            let value = binding.values()[0];
            for condition in [binding.available_when(), value.condition()] {
                requirements.push((edge.condition(), condition));
            }
            match conditions.owner_value(value.source()) {
                Some(CleanupOwnerValue::IterationPhi {
                    symbol: source_symbol,
                    ..
                }) if *source_symbol == symbol => pending.push(value.source()),
                Some(
                    CleanupOwnerValue::Expression { .. } | CleanupOwnerValue::Parameter { .. },
                ) => {
                    // Cycles and nested transports are justified only by one real Always Entry.
                    if *boundary != IterationPhiBoundary::Header
                        || edge.kind() != IterationPhiIncomingKind::Entry
                        || !matches!(
                            conditions.get(binding.available_when()),
                            Some(CleanupCondition::Always)
                        )
                        || !matches!(
                            conditions.get(value.condition()),
                            Some(CleanupCondition::Always)
                        )
                    {
                        return false;
                    }
                    origins.insert(value.source());
                }
                _ => return false,
            }
        }
    }
    // The complete graph has no absent transport and one grounded seed. Its own
    // presence slots are therefore true; independent control selectors still need implication.
    origins.len() == 1
        && requirements.into_iter().all(|(path, presence)| {
            implies(conditions, path, presence, &proven, &mut BTreeSet::new())
        })
}
