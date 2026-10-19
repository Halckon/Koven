//! A two-selector truth check, not a general CFG or owner-presence solver.
use super::*;
use lang_frontend::ownership_checking::{
    CleanupConditions, CleanupSelection, CleanupSelectorSource,
};

#[derive(Clone, Copy)]
pub(super) enum Expected {
    Path,
    Presence,
    Conjunction,
}

pub(super) fn path(
    parsed: &ParsedFile,
    conditions: &CleanupConditions,
    condition: CleanupConditionId,
    previous: Option<(&identity::Site<'_>, &Proof)>,
    span: Span,
) -> Result<Option<Path>, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let Some((site, proof)) = previous.filter(|(site, _)| site.guard.is_some()) else {
        if conditions.get(condition) != Some(&CleanupCondition::Always) {
            return Err(missing());
        }
        return Ok(None);
    };
    let guard = site.guard.ok_or_else(missing)?;
    if proof.late_size != Some(guard.size)
        || proof.drop.point() != DropPoint::CallReturn(guard.size)
    {
        return Err(missing());
    }
    let Some(CleanupCondition::Choice { selector, branches }) = conditions.get(condition) else {
        return Err(missing());
    };
    let [success, failure] = branches.as_slice() else {
        return Err(missing());
    };
    let control = parsed
        .ast()
        .expressions()
        .get(guard.control)
        .map_err(|_| missing())?;
    let layout = conditions.selector(*selector).ok_or_else(missing)?;
    if layout.source() != CleanupSelectorSource::Control(guard.control)
        || layout.selection() != CleanupSelection::Branch
        || layout.origin() != control.span()
        || layout.branch_count() != 2
        || conditions.get(*success) != Some(&CleanupCondition::Always)
        || conditions.get(*failure) != Some(&CleanupCondition::Never)
    {
        return Err(missing());
    }
    Ok(Some(Path {
        control: guard.control,
        condition: guard.condition,
        selector: *selector,
        branch: 0,
        success: guard.success,
        failure: guard.failure,
        size: guard.size,
        previous: site.plan.descriptor().statement(),
    }))
}

pub(super) fn check(
    conditions: &CleanupConditions,
    condition: CleanupConditionId,
    path: Option<Path>,
    presence: Option<CleanupSelectorId>,
    expected: Expected,
    span: Span,
) -> Result<(), LoweringError> {
    validate(
        conditions,
        condition,
        path,
        presence,
        &mut Vec::new(),
        &mut Vec::new(),
        span,
    )?;
    // Both arms of every permitted selector are inspected. A path proof does not
    // make a condition containing an unknown/corrupted selector harmless.
    for path_live in [false, true] {
        for owner_live in [false, true] {
            let actual = evaluate(
                conditions,
                condition,
                path,
                presence,
                (path_live, owner_live),
                conditions.nodes().len(),
                span,
            )?;
            let reached = path.is_none() || path_live;
            let expected = match expected {
                Expected::Path => reached,
                Expected::Presence => owner_live,
                Expected::Conjunction => reached && owner_live,
            };
            if actual != expected {
                return Err(error(LoweringErrorKind::MissingFact, span));
            }
        }
    }
    Ok(())
}

fn evaluate(
    conditions: &CleanupConditions,
    condition: CleanupConditionId,
    path: Option<Path>,
    presence: Option<CleanupSelectorId>,
    assignment: (bool, bool),
    remaining: usize,
    span: Span,
) -> Result<bool, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    if remaining == 0 {
        return Err(missing());
    }
    let (path_live, owner_live) = assignment;
    match conditions.get(condition).ok_or_else(missing)? {
        CleanupCondition::Always => Ok(true),
        CleanupCondition::Never => Ok(false),
        CleanupCondition::Choice { selector, branches } => {
            let layout = conditions.selector(*selector).ok_or_else(missing)?;
            if branches.len() != 2 || layout.branch_count() != 2 {
                return Err(missing());
            }
            let branch = if Some(*selector) == presence {
                if layout.selection() != CleanupSelection::IterationPresence {
                    return Err(missing());
                }
                usize::from(owner_live)
            } else if let Some(path) = path.filter(|path| path.selector == *selector) {
                if layout.source() != CleanupSelectorSource::Control(path.control)
                    || layout.selection() != CleanupSelection::Branch
                {
                    return Err(missing());
                }
                if path_live {
                    path.branch
                } else {
                    1 - path.branch
                }
            } else {
                return Err(missing());
            };
            // Check both children for foreign selectors, not just the selected
            // child under one assignment. All four assignments are evaluated.
            evaluate(
                conditions,
                branches[branch],
                path,
                presence,
                assignment,
                remaining - 1,
                span,
            )
        }
    }
}

/// Inspect even a structurally unreachable repeated-selector child. Evaluation
/// alone could otherwise hide a foreign selector or a cycle in that child.
fn validate(
    conditions: &CleanupConditions,
    condition: CleanupConditionId,
    path: Option<Path>,
    presence: Option<CleanupSelectorId>,
    active: &mut Vec<CleanupConditionId>,
    done: &mut Vec<CleanupConditionId>,
    span: Span,
) -> Result<(), LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    if active.len() >= 3 || active.contains(&condition) {
        return Err(missing());
    }
    if done.contains(&condition) {
        return Ok(());
    }
    active.push(condition);
    match conditions.get(condition).ok_or_else(missing)? {
        CleanupCondition::Always | CleanupCondition::Never => {}
        CleanupCondition::Choice { selector, branches } => {
            let layout = conditions.selector(*selector).ok_or_else(missing)?;
            let valid = if Some(*selector) == presence {
                layout.selection() == CleanupSelection::IterationPresence
            } else {
                path.is_some_and(|path| {
                    path.selector == *selector
                        && layout.source() == CleanupSelectorSource::Control(path.control)
                        && layout.selection() == CleanupSelection::Branch
                })
            };
            if !valid || branches.len() != 2 || layout.branch_count() != 2 {
                return Err(missing());
            }
            for branch in branches {
                validate(conditions, *branch, path, presence, active, done, span)?;
            }
        }
    }
    active.pop();
    done.push(condition);
    Ok(())
}
