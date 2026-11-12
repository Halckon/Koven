//! One complete two-version cleanup group and bounded constant-presence proof.
mod carrier;
mod source;

pub(in crate::ssa::lower_frontend) mod constant_presence {
    mod source;
    mod source_return;

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

    pub(in crate::ssa::lower_frontend) fn constant_drop(
        parsed: &lang_frontend::parser::ParsedFile,
        names: &lang_frontend::name_resolution::NameResolution,
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
        let source = matches!(plan.source(), Some(LoanTarget::Place(path)) if path.is_root() && path.root() == symbol);
        let resource = typed
            .symbol_type(symbol)
            .is_some_and(|ty| typed.is_resource_type(ty) == Some(true));
        if !source && !resource {
            return false;
        }
        prove_graph(owned, owner, symbol, fact.condition())
            || source::prove(parsed, names, typed, owned, fact).is_some()
            || source_return::prove(parsed, names, typed, owned, fact).is_some()
    }

    pub(in crate::ssa::lower_frontend) fn prove_graph(
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
        origins.len() == 1
            && requirements.into_iter().all(|(path, presence)| {
                implies(conditions, path, presence, &proven, &mut BTreeSet::new())
            })
    }
}

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
    pub(super) fn is_proven_nullable_if_field_read(
        &self,
        expression: ExpressionId,
        proof: NonNullProof,
    ) -> bool {
        self.owned.drops().iter().copied().any(|fact| {
            fact.point() == DropPoint::AfterExpression(expression)
                && candidate(self.parsed, self.names, self.typed, self.owned, fact).is_some_and(
                    |plan| {
                        plan.arm == 0
                            && self.non_null_bindings.get(&plan.root) == Some(&proof)
                            && self
                                .checked_nullable_if_cleanup_owner(&plan, &plan.members)
                                .ok()
                                == Some(proof.owner)
                    },
                )
        })
    }

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
        let owner = self.checked_nullable_if_cleanup_owner(&plan, remaining)?;
        self.end_non_null_proof(plan.root, span)?;
        if self.bindings.remove(&plan.root) != Some(LoweredValue::Value(owner)) {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        self.append(Operation::Drop { owner }, Vec::new(), span)?;
        self.release_owner_capture_loans(owner, span)?;
        Ok(Some(plan.members.len()))
    }
}

#[cfg(test)]
mod tests;
