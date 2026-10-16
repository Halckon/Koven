//! One complete two-version cleanup group, consumed through its actual SSA owner column.
//! A source candidate only passes preflight; it never substitutes for the carrier proof.
mod carrier;
mod source;

use lang_frontend::ownership_checking::{
    CleanupCondition, CleanupConditions, CleanupOwnerValue, CleanupSelection, CleanupSelectorId,
    DropFact, DropTarget, IterationCleanupAction,
};

use super::*;

pub(super) struct GroupPlan {
    members: [DropFact; 2],
    root: SymbolId,
    storage: TypeId,
    first_if: ExpressionId,
    second_if: ExpressionId,
    arm: usize,
    replacement: ExpressionId,
    old_drop: DropFact,
    while_statement: StatementId,
    function_span: Span,
    seed_parameter: usize,
    parameter_count: usize,
}

/// Shared by the global conditional-drop gate and the consumer. No SSA is admitted here.
pub(super) fn candidate(
    parsed: &ParsedFile,
    names: &NameResolution,
    typed: &TypedFile,
    owned: &OwnershipCheckedFile,
    fact: DropFact,
) -> Option<GroupPlan> {
    fact.condition()?;
    let DropTarget::Named(root) = fact.target() else {
        return None;
    };
    let members = owned
        .drops()
        .iter()
        .copied()
        .filter(|other| other.point() == fact.point() && other.target() == fact.target())
        .collect::<Vec<_>>();
    let [old, new] = members.as_slice() else {
        return None;
    };
    let members = [*old, *new];
    // Consume consecutive original facts together, never reorder another obligation.
    if !owned
        .drops()
        .windows(2)
        .any(|pair| pair == members.as_slice())
        || members
            .iter()
            .any(|member| member.capture_slot().is_some() || member.instance_address().is_some())
        || !owned.cleanup_steps().windows(2).any(|steps| {
            steps.iter().zip(members).all(|((point, action), member)| {
                *point == member.point()
                    && matches!(action, IterationCleanupAction::Drop(drop) if *drop == member)
            })
        })
    {
        return None;
    }
    let conditions = owned.cleanup_conditions();
    let CleanupOwnerValue::Parameter {
        symbol: seed,
        origin,
    } = conditions.owner_value(old.owner()?)?
    else {
        return None;
    };
    if names.symbols().get(seed.index())?.span() != *origin {
        return None;
    }
    let CleanupOwnerValue::Expression {
        expression: replacement,
        origin,
    } = conditions.owner_value(new.owner()?)?
    else {
        return None;
    };
    if parsed.ast().expressions().get(*replacement).ok()?.span() != *origin {
        return None;
    }
    let (first_if, second_if, arm) = coverage(parsed, conditions, members)?;
    source::qualify(
        parsed,
        names,
        typed,
        owned,
        members,
        root,
        *seed,
        *replacement,
        first_if,
        second_if,
        arm,
    )
}

/// Exact DAG: (!A && B_arm) OR (A && B_arm). The alternatives are disjoint and
/// exhaustive on B_arm; neither version is individually an unconditional drop.
fn coverage(
    parsed: &ParsedFile,
    conditions: &CleanupConditions,
    members: [DropFact; 2],
) -> Option<(ExpressionId, ExpressionId, usize)> {
    let choice = |condition| match conditions.get(condition)? {
        CleanupCondition::Choice { selector, branches } if branches.len() == 2 => {
            Some((*selector, [branches[0], branches[1]]))
        }
        _ => None,
    };
    let (first, old) = choice(members[0].condition()?)?;
    let (same_first, new) = choice(members[1].condition()?)?;
    if first != same_first
        || old[1] != new[0]
        || !matches!(conditions.get(old[0]), Some(CleanupCondition::Never))
        || !matches!(conditions.get(new[1]), Some(CleanupCondition::Never))
    {
        return None;
    }
    let (second, arms) = choice(old[1])?;
    if second == first {
        return None;
    }
    let arm = match (conditions.get(arms[0])?, conditions.get(arms[1])?) {
        (CleanupCondition::Always, CleanupCondition::Never) => 0,
        (CleanupCondition::Never, CleanupCondition::Always) => 1,
        _ => return None,
    };
    Some((
        control(parsed, conditions, first)?,
        control(parsed, conditions, second)?,
        arm,
    ))
}

fn control(
    parsed: &ParsedFile,
    conditions: &CleanupConditions,
    selector: CleanupSelectorId,
) -> Option<ExpressionId> {
    let selector = conditions.selector(selector)?;
    let expression = selector.control()?;
    (selector.selection() == CleanupSelection::Branch
        && selector.branch_count() == 2
        && selector.origin() == parsed.ast().expressions().get(expression).ok()?.span())
    .then_some(expression)
}

impl ExpressionLowerer<'_> {
    /// Do not let existing point cleanup retire a live child before this new path's
    /// strict qualification. This pass is read-only, including for every other fact.
    pub(super) fn preflight_nullable_if_cleanup_groups(
        &self,
        facts: &[DropFact],
    ) -> Result<(), LoweringError> {
        let mut index = 0;
        while let Some(&fact) = facts.get(index) {
            if let Some(plan) = candidate(self.parsed, self.names, self.typed, self.owned, fact) {
                self.checked_nullable_if_cleanup_owner(&plan, &facts[index..])?;
                index += plan.members.len();
            } else {
                index += 1;
            }
        }
        Ok(())
    }

    fn checked_nullable_if_cleanup_owner(
        &self,
        plan: &GroupPlan,
        remaining: &[DropFact],
    ) -> Result<ValueId, LoweringError> {
        let span = plan.members[0].value_origin();
        if remaining.get(..plan.members.len()) != Some(plan.members.as_slice()) {
            return Err(error(LoweringErrorKind::UnsupportedNode, span));
        }
        let owner = self
            .nullable_if_cleanup_owner(plan)
            .ok_or_else(|| error(LoweringErrorKind::UnsupportedNode, span))?;
        self.ensure_no_unresolved_while_proof_carriers(span)?;
        Ok(owner)
    }

    pub(super) fn emit_nullable_if_cleanup_group(
        &mut self,
        remaining: &[DropFact],
    ) -> Result<Option<usize>, LoweringError> {
        let Some(&fact) = remaining.first() else {
            return Ok(None);
        };
        let Some(plan) = candidate(self.parsed, self.names, self.typed, self.owned, fact) else {
            return Ok(None);
        };
        let span = fact.value_origin();
        // Every identity and both real incoming versions must be checked before mutation.
        let owner = self.checked_nullable_if_cleanup_owner(&plan, remaining)?;
        self.end_non_null_proof(plan.root, span)?;
        if self.bindings.remove(&plan.root) != Some(LoweredValue::Value(owner)) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.append(Operation::Drop { owner }, Vec::new(), span)?;
        self.release_owner_capture_loans(owner, span)?;
        // The caller advances over ALL members; no second version can drop this binding again.
        Ok(Some(plan.members.len()))
    }
}
