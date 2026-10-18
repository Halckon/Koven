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
    truth(parsed, owned, edge.condition(), &expected)
}

/// Ground each Return/Break at the actual innermost provider, even when cleanup is
/// also present in enclosing For plans. A child transfer's own If cannot vanish
/// merely because the owner being cleaned belongs to an outer provider.
pub(super) fn transfer(
    parsed: &ParsedFile,
    owned: &OwnershipCheckedFile,
    expression: ExpressionId,
    condition: CleanupConditionId,
) -> Result<(), Failure> {
    let transferred = parsed.ast().expressions().get(expression).unwrap();
    need(
        matches!(
            transferred.payload(),
            Expression::Return { .. } | Expression::Break { .. }
        ),
        "stack exact original finite transfer AST",
        transferred,
    )?;
    let function = one(
        parsed.roots().iter().filter_map(|id| {
            let node = parsed.ast().items().get(*id).ok()?;
            (matches!(node.payload(), Item::Function { .. })
                && inside(transferred.span(), node.span()))
            .then_some(node.span())
        }),
        "stack transfer actual callable scope",
    )?;
    let region = parsed
        .ast()
        .statements()
        .iter()
        .filter_map(|(_, node)| {
            let Statement::For { body, .. } = node.payload() else {
                return None;
            };
            let body = parsed.ast().statements().get(*body).ok()?.span();
            inside(transferred.span(), body).then_some(body)
        })
        .min_by_key(|body| body.end() - body.start())
        .unwrap_or(function);
    let mut expected = Vec::new();
    for (id, node) in parsed.ast().expressions().iter() {
        let Expression::If {
            then_branch,
            else_branch,
            ..
        } = node.payload()
        else {
            continue;
        };
        if !inside(node.span(), function) {
            continue;
        }
        let then = parsed.ast().statements().get(*then_branch).unwrap().span();
        let other = else_branch.map(|s| parsed.ast().statements().get(s).unwrap().span());
        let selected = if inside(transferred.span(), then) {
            Some(0)
        } else if other.is_some_and(|other| inside(transferred.span(), other)) {
            Some(1)
        } else if inside(node.span(), region) && node.span().end() <= transferred.span().start() {
            let in_child = parsed.ast().statements().iter().any(|(_, child)| {
                let Statement::For { body, .. } = child.payload() else {
                    return false;
                };
                let body = parsed.ast().statements().get(*body).unwrap().span();
                inside(child.span(), region) && inside(node.span(), body)
            });
            if in_child {
                continue;
            }
            match (
                transfers(parsed, *then_branch),
                else_branch.is_some_and(|other| transfers(parsed, other)),
            ) {
                (true, false) => Some(1),
                (false, true) => Some(0),
                (false, false) => None,
                (true, true) => {
                    return Err(failure(
                        "stack transfer cannot follow a direct If with two transferring arms",
                        (node, transferred),
                    ));
                }
            }
        } else {
            None
        };
        if let Some(selected) = selected {
            expected.push((control(owned.cleanup_conditions(), id)?, selected));
        }
    }
    truth(parsed, owned, condition, &expected)
}

fn transfers(parsed: &ParsedFile, statement: StatementId) -> bool {
    match parsed.ast().statements().get(statement).unwrap().payload() {
        Statement::Block { elements } | Statement::ControlBody { elements } => {
            elements.last().is_some_and(|s| transfers(parsed, *s))
        }
        Statement::Expression { expression } => match parsed
            .ast()
            .expressions()
            .get(*expression)
            .unwrap()
            .payload()
        {
            Expression::Return { .. } | Expression::Break { .. } | Expression::Continue { .. } => {
                true
            }
            Expression::If {
                then_branch,
                else_branch: Some(other),
                ..
            } => transfers(parsed, *then_branch) && transfers(parsed, *other),
            _ => false,
        },
        _ => false,
    }
}
fn control(
    table: &CleanupConditions,
    expression: ExpressionId,
) -> Result<CleanupSelectorId, Failure> {
    one(
        table
            .nodes()
            .iter()
            .filter_map(|node| match node {
                C::Choice { selector, .. } => Some(*selector),
                _ => None,
            })
            .filter(|id| {
                table.selector(*id).is_some_and(|s| {
                    s.source() == CleanupSelectorSource::Control(expression)
                        && s.selection() == CleanupSelection::Branch
                })
            })
            .collect::<BTreeSet<_>>()
            .into_iter(),
        "stack actual AST control selector identity",
    )
}
fn truth(
    parsed: &ParsedFile,
    owned: &OwnershipCheckedFile,
    condition: CleanupConditionId,
    expected: &[(CleanupSelectorId, usize)],
) -> Result<(), Failure> {
    let table = owned.cleanup_conditions();
    let mut support = BTreeSet::new();
    collect(table, condition, &mut support)?;
    support.extend(expected.iter().map(|(selector, _)| *selector));
    registry(parsed, owned, &support)?;
    for choices in assignments(table, &support)? {
        let actual = eval(table, condition, &choices)?;
        let original = expected
            .iter()
            .all(|(selector, branch)| choices.get(selector) == Some(branch));
        need(
            actual == original,
            "stack condition equals actual original AST branch path",
            (condition, expected, &choices, actual, original),
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
