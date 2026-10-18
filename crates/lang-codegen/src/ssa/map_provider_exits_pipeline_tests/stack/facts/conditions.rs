//! Read-only truth proof over the original cleanup registry and actual AST paths.
use super::*;
use lang_frontend::ownership_checking::{
    CleanupCondition as C, CleanupConditionId, CleanupConditions, CleanupSelection,
    CleanupSelectorId, CleanupSelectorSource, IterationPhiIncoming, IterationPhiIncomingKind as K,
    OwnershipCheckedFile,
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn path(
    parsed: &ParsedFile,
    site: &AstSite,
    edge: &IterationPhiIncoming,
    owned: &OwnershipCheckedFile,
) -> Result<(), Failure> {
    let table = owned.cleanup_conditions();
    let mut expected = Vec::new();
    let cutoff = match edge.kind() {
        K::Fallthrough => Some(site.body_span.end()),
        K::Break(id) => Some(parsed.ast().expressions().get(id).unwrap().span().start()),
        K::Entry | K::Exhaustion => None,
        _ => return Err(failure("stack original incoming finite path kind", edge)),
    };
    if let Some(cutoff) = cutoff {
        for (id, node) in parsed.ast().expressions().iter() {
            let Expression::If {
                then_branch,
                else_branch: None,
                ..
            } = node.payload()
            else {
                continue;
            };
            if !inside(node.span(), site.body_span) {
                continue;
            }
            // A child loop publishes its own Exit owner; its per-iteration
            // conditional selector is not the enclosing loop's path selector.
            let in_child = parsed.ast().statements().iter().any(|(statement, n)| {
                let Statement::For { body, .. } = n.payload() else {
                    return false;
                };
                statement != site.statement
                    && inside(n.span(), site.body_span)
                    && inside(
                        node.span(),
                        parsed.ast().statements().get(*body).unwrap().span(),
                    )
            });
            if in_child {
                continue;
            }
            let then = parsed.ast().statements().get(*then_branch).unwrap().span();
            let transfers = parsed.ast().expressions().iter().any(|(_, n)| {
                inside(n.span(), then)
                    && matches!(
                        n.payload(),
                        Expression::Return { .. } | Expression::Break { .. }
                    )
            });
            if !transfers {
                continue;
            }
            let selected = if cutoff >= then.start() && cutoff < then.end() {
                0
            } else if node.span().end() <= cutoff {
                1
            } else {
                continue;
            };
            let selector = one(
                table
                    .nodes()
                    .iter()
                    .filter_map(|n| match n {
                        C::Choice { selector, .. } => Some(*selector),
                        _ => None,
                    })
                    .filter(|s| {
                        table.selector(*s).is_some_and(|s| {
                            s.source() == CleanupSelectorSource::Control(id)
                                && s.selection() == CleanupSelection::Branch
                        })
                    })
                    .collect::<BTreeSet<_>>()
                    .into_iter(),
                "stack actual conditional path selector",
            )?;
            expected.push((selector, selected));
        }
    }
    let mut support = BTreeSet::new();
    collect(table, edge.condition(), &mut support)?;
    support.extend(expected.iter().map(|(s, _)| *s));
    registry(parsed, owned, &support)?;
    for choices in assignments(table, &support)? {
        let actual = eval(table, edge.condition(), &choices)?;
        let expected = expected
            .iter()
            .all(|(selector, branch)| choices.get(selector) == Some(branch));
        need(
            actual == expected,
            "stack incoming condition equals actual surviving AST branch path",
            (edge, &choices, actual, expected),
        )?;
    }
    Ok(())
}

pub(super) fn conjunction(
    parsed: &ParsedFile,
    owned: &OwnershipCheckedFile,
    actual: CleanupConditionId,
    path: CleanupConditionId,
    current: CleanupConditionId,
) -> Result<(), Failure> {
    let table = owned.cleanup_conditions();
    let mut support = BTreeSet::new();
    for condition in [actual, path, current] {
        collect(table, condition, &mut support)?;
    }
    registry(parsed, owned, &support)?;
    for choices in assignments(table, &support)? {
        let actual_value = eval(table, actual, &choices)?;
        let expected = eval(table, path, &choices)? && eval(table, current, &choices)?;
        need(
            actual_value == expected,
            "stack current owner availability is exactly path AND current-owner presence",
            (actual, path, current, &choices, actual_value, expected),
        )?;
    }
    Ok(())
}

fn collect(
    table: &CleanupConditions,
    id: CleanupConditionId,
    support: &mut BTreeSet<CleanupSelectorId>,
) -> Result<(), Failure> {
    match table
        .get(id)
        .ok_or_else(|| failure("stack actual registry condition node", id))?
    {
        C::Always | C::Never => {}
        C::Choice { selector, branches } => {
            let layout = table
                .selector(*selector)
                .ok_or_else(|| failure("stack actual registry selector", selector))?;
            need(
                !branches.is_empty()
                    && branches.len() == layout.branch_count()
                    && branches.iter().all(|b| b.index() < id.index()),
                "stack complete original acyclic registry branch layout",
                (id, branches, layout),
            )?;
            support.insert(*selector);
            for &branch in branches {
                collect(table, branch, support)?;
            }
        }
    }
    Ok(())
}
fn registry(
    parsed: &ParsedFile,
    owned: &OwnershipCheckedFile,
    support: &BTreeSet<CleanupSelectorId>,
) -> Result<(), Failure> {
    let table = owned.cleanup_conditions();
    for &id in support {
        let layout = table
            .selector(id)
            .ok_or_else(|| failure("stack original selector layout", id))?;
        let original = match layout.source() {
            CleanupSelectorSource::Control(control) => {
                parsed.ast().expressions().get(control).is_ok_and(|n| {
                    matches!(n.payload(), Expression::If { .. }) && n.span() == layout.origin()
                }) && layout.selection() == CleanupSelection::Branch
                    && layout.branch_count() == 2
            }
            CleanupSelectorSource::IterationPhi {
                statement,
                boundary,
                slot,
            } => {
                owned.map_iteration(statement).is_some_and(|p| {
                    p.closure_phis()
                        .iter()
                        .filter(|phi| phi.boundary() == boundary)
                        .enumerate()
                        .any(|(index, phi)| index == slot && phi.availability_selector() == id)
                }) && parsed.ast().statements().get(statement).is_ok_and(|n| {
                    matches!(n.payload(), Statement::For { .. }) && n.span() == layout.origin()
                }) && layout.selection() == CleanupSelection::IterationPresence
                    && layout.branch_count() == 2
            }
            CleanupSelectorSource::CaptureLoan { .. } => false,
        };
        need(
            original,
            "stack condition selector retains exact original control/For/slot identity",
            (id, layout),
        )?;
    }
    Ok(())
}
fn assignments(
    table: &CleanupConditions,
    support: &BTreeSet<CleanupSelectorId>,
) -> Result<Vec<BTreeMap<CleanupSelectorId, usize>>, Failure> {
    let mut values = vec![BTreeMap::new()];
    for &selector in support {
        let count = table
            .selector(selector)
            .ok_or_else(|| failure("stack finite registry dimension", selector))?
            .branch_count();
        let mut next = Vec::new();
        for value in values {
            for branch in 0..count {
                let mut value = value.clone();
                value.insert(selector, branch);
                next.push(value);
            }
        }
        values = next;
    }
    Ok(values)
}
fn eval(
    table: &CleanupConditions,
    id: CleanupConditionId,
    choices: &BTreeMap<CleanupSelectorId, usize>,
) -> Result<bool, Failure> {
    match table
        .get(id)
        .ok_or_else(|| failure("stack exact condition evaluation", id))?
    {
        C::Always => Ok(true),
        C::Never => Ok(false),
        C::Choice { selector, branches } => {
            let selected = choices
                .get(selector)
                .ok_or_else(|| failure("stack complete selector assignment", selector))?;
            let branch = branches.get(*selected).ok_or_else(|| {
                failure("stack actual selected registry arm", (selector, selected))
            })?;
            eval(table, *branch, choices)
        }
    }
}
