//! One complete two-version cleanup group and bounded constant-presence proof.
pub(in crate::ssa) use crate::ssa::map_promotion_contract_tests::nullable_if_carrier as carrier;
use carrier::source;

pub(in crate::ssa) mod constant_presence {
    pub(in crate::ssa) use crate::ssa::deinit_tests::constant_presence_source as source;
    use source::source_return;

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

    pub(in crate::ssa) fn constant_drop(
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

    pub(in crate::ssa) fn prove_graph(
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

pub(in crate::ssa) struct GroupPlan {
    pub(in crate::ssa) members: [DropFact; 2],
    pub(in crate::ssa) root: SymbolId,
    pub(in crate::ssa) storage: TypeId,
    pub(in crate::ssa) first_if: ExpressionId,
    pub(in crate::ssa) second_if: ExpressionId,
    pub(in crate::ssa) arm: usize,
    pub(in crate::ssa) replacement: ExpressionId,
    pub(in crate::ssa) old_drop: DropFact,
    pub(in crate::ssa) while_statement: StatementId,
    pub(in crate::ssa) function_span: Span,
    pub(in crate::ssa) seed_parameter: usize,
    pub(in crate::ssa) parameter_count: usize,
}

pub(in crate::ssa) fn candidate(
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

pub(in crate::ssa) fn coverage(
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
    pub(in crate::ssa) fn is_proven_nullable_if_field_read(
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

    pub(in crate::ssa) fn preflight_nullable_if_cleanup_groups(
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

    pub(in crate::ssa) fn checked_nullable_if_cleanup_owner(
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

    pub(in crate::ssa) fn emit_nullable_if_cleanup_group(
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

pub(in crate::ssa) mod local_runtime {
    use crate::ssa::lower_frontend::map_iteration::resource::evidence;
    use crate::ssa::lower_frontend::map_iteration::*;
    use crate::ssa::model::InstructionId;
    use lang_frontend::ownership_checking::{DropFact, DropTarget};

    pub(in crate::ssa) mod conditions {
        use crate::ssa::lower_frontend::map_iteration::local::{Path, Proof, identity};
        use crate::ssa::lower_frontend::map_iteration::*;
        use lang_frontend::ownership_checking::{
            CleanupConditionId, CleanupConditions, CleanupSelection, CleanupSelectorId,
            CleanupSelectorSource,
        };

        #[derive(Clone, Copy)]
        pub(in crate::ssa) enum Expected {
            Path,
            Presence,
            Conjunction,
        }

        pub(in crate::ssa) fn path(
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
            let Some(CleanupCondition::Choice { selector, branches }) = conditions.get(condition)
            else {
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

        pub(in crate::ssa) fn check(
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
    }

    #[derive(Clone, Copy)]
    pub(in crate::ssa) struct Consumption {
        pub(in crate::ssa) block: BlockId,
        pub(in crate::ssa) owner: ValueId,
        pub(in crate::ssa) instruction: InstructionId,
    }

    #[derive(Clone)]
    pub(in crate::ssa) struct Carrier {
        pub(in crate::ssa) proof: local::Proof,
        pub(in crate::ssa) statement: StatementId,
        pub(in crate::ssa) birth: ValueId,
        pub(in crate::ssa) current: ValueId,
        pub(in crate::ssa) continuation: BlockId,
        pub(in crate::ssa) source: LoanId,
        pub(in crate::ssa) consumed: Option<Consumption>,
        pub(in crate::ssa) size: Option<ValueId>,
    }

    impl Carrier {
        pub(in crate::ssa) fn root(&self) -> SymbolId {
            self.proof.root
        }
    }

    impl ExpressionLowerer<'_> {
        pub(in crate::ssa) fn local_map_seed(
            &self,
            symbol: SymbolId,
            initializer: ExpressionId,
            container: SsaTypeId,
            span: Span,
        ) -> Result<EntityId, LoweringError> {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let Some(LoweredValue::Value(owner)) = self.bindings.get(&symbol).copied() else {
                return Err(missing());
            };
            if self.borrow_bindings.contains_key(&symbol)
                || self.temporaries.values().any(|value| *value == owner)
                || self
                    .bindings
                    .iter()
                    .any(|(other, value)| *other != symbol && *value == LoweredValue::Value(owner))
                || self.map_iteration_state.locals.contains_key(&symbol)
            {
                return Err(missing());
            }
            let seed = EntityId::Value(owner);
            let instruction = carrier::definition(self.function, seed).ok_or_else(missing)?;
            if instruction.block != self.block
                || self.map_call_birth(initializer, span)? != owner
                || self.function.entity(seed).map(|entity| entity.ty)
                    != Some(EntityType::Value(container))
            {
                return Err(missing());
            }
            Ok(seed)
        }

        pub(in crate::ssa) fn validate_local_map_entry(
            &self,
            proof: &local::Proof,
            span: Span,
        ) -> Result<(), LoweringError> {
            for dead in &proof.dead {
                self.validate_dead_local_map(*dead, span)?;
            }
            if let Some(path) = proof.path {
                evidence::path(self, path, span)?;
            }
            Ok(())
        }

        fn validate_dead_local_map(
            &self,
            dead: local::DeadColumn,
            span: Span,
        ) -> Result<(), LoweringError> {
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let previous = self
                .map_iteration_state
                .locals
                .get(&dead.root)
                .ok_or_else(missing)?;
            if previous.statement != dead.previous
                || previous.proof.initializer != dead.initializer
                || previous.proof.drop != dead.consumed
                || previous.consumed.is_none()
                || self.bindings.contains_key(&dead.root)
                || self.borrow_bindings.contains_key(&dead.root)
                || self
                    .temporaries
                    .values()
                    .any(|owner| *owner == previous.birth || *owner == previous.current)
            {
                return Err(missing());
            }
            validate_consumption(self.function, previous, span)
        }

        pub(in crate::ssa) fn local_map_completion(
            &self,
            proof: local::Proof,
            evidence: carrier::Evidence<'_>,
        ) -> Result<Carrier, LoweringError> {
            let span = evidence.span;
            let missing = || error(LoweringErrorKind::MissingFact, span);
            evidence::edges(evidence, &proof)?;
            evidence::consumer(self, evidence, &proof)?;
            let Some((
                owner_proof,
                SourceIdentity::Local {
                    symbol,
                    initializer,
                },
                current,
            )) = evidence.owner
            else {
                return Err(missing());
            };
            let EntityId::Value(birth) = evidence.seed else {
                return Err(missing());
            };
            if symbol != proof.root
                || initializer != proof.initializer
                || owner_proof.drop() != Some(proof.drop)
                || evidence.continuation != evidence.blocks[3]
                || self.block != evidence.continuation
            {
                return Err(missing());
            }
            let parameters = &self
                .function
                .block(self.block)
                .ok_or_else(missing)?
                .parameters;
            let (source, _) = roles(parameters, evidence.source_slot, evidence.cursor_slot, span)?;
            let instructions =
                carrier::instructions(self.function, self.block).ok_or_else(missing)?;
            let source_end = unique(instructions.iter().enumerate().filter_map(
                |(index, instruction)| {
                    (instruction.operation == Operation::BorrowEnd { loan: source })
                        .then_some(index)
                },
            ))
            .ok_or_else(missing)?;
            let drops = instructions
                .iter()
                .enumerate()
                .filter(|(_, instruction)| {
                    instruction.operation == Operation::Drop { owner: current }
                })
                .collect::<Vec<_>>();
            let consumed = if proof.late_size.is_some() {
                if self.bindings.get(&symbol) != Some(&LoweredValue::Value(current))
                    || !drops.is_empty()
                {
                    return Err(missing());
                }
                None
            } else {
                let [(index, instruction)] = drops.as_slice() else {
                    return Err(missing());
                };
                if *index <= source_end
                    || self.bindings.contains_key(&symbol)
                    || instruction.origin != Origin::Source(proof.drop.value_origin())
                {
                    return Err(missing());
                }
                Some(Consumption {
                    block: self.block,
                    owner: current,
                    instruction: instruction.id,
                })
            };
            let statement = match self.owned.cleanup_conditions().owner_value(proof.header) {
                Some(lang_frontend::ownership_checking::CleanupOwnerValue::IterationPhi {
                    statement,
                    symbol: actual,
                    ..
                }) if *actual == symbol => *statement,
                _ => return Err(missing()),
            };
            Ok(Carrier {
                proof,
                statement,
                birth,
                current,
                continuation: self.block,
                source,
                consumed,
                size: None,
            })
        }

        pub(in crate::ssa) fn emit_local_map_drop(
            &mut self,
            fact: DropFact,
        ) -> Result<bool, LoweringError> {
            let span = fact.value_origin();
            let missing = || error(LoweringErrorKind::MissingFact, span);
            let DropTarget::Named(root) = fact.target() else {
                return Ok(false);
            };
            if let Some(state) = self.map_iteration_state.locals.get(&root).cloned()
                && state.proof.drop == fact
            {
                let size = state.proof.late_size.ok_or_else(missing)?;
                if state.consumed.is_some()
                    || fact.point() != DropPoint::CallReturn(size)
                    || self.block != state.continuation
                    || self.bindings.get(&root) != Some(&LoweredValue::Value(state.current))
                    || self.borrow_bindings.contains_key(&root)
                    || self
                        .temporaries
                        .values()
                        .any(|value| *value == state.current)
                {
                    return Err(missing());
                }
                nested::transport(
                    self.function,
                    EntityId::Value(state.current),
                    EntityId::Value(state.birth),
                    span,
                )?;
                let instructions =
                    carrier::instructions(self.function, self.block).ok_or_else(missing)?;
                let read = instructions.last().copied().ok_or_else(missing)?;
                let [EntityId::Value(result)] = read.results.as_slice() else {
                    return Err(missing());
                };
                if read.operation
                    != (Operation::MapSize {
                        owner: EntityId::Value(state.current),
                    })
                    || read.origin != Origin::Source(self.expression_span(size)?)
                    || !instructions.iter().any(|instruction| {
                        instruction.operation == Operation::BorrowEnd { loan: state.source }
                    })
                {
                    return Err(missing());
                }
                let result = *result;
                self.bindings.remove(&root);
                let (instruction, _) = self.append(
                    Operation::Drop {
                        owner: state.current,
                    },
                    Vec::new(),
                    span,
                )?;
                let state = self
                    .map_iteration_state
                    .locals
                    .get_mut(&root)
                    .ok_or_else(missing)?;
                state.consumed = Some(Consumption {
                    block: self.block,
                    owner: state.current,
                    instruction,
                });
                state.size = Some(result);
                return Ok(true);
            }
            let DropPoint::LoopExit(statement) = fact.point() else {
                return Ok(false);
            };
            let Some(descriptor) = self.typed.map_iteration(statement).copied() else {
                return Ok(false);
            };
            let qualified =
                qualify::qualify(self.parsed, self.names, self.typed, self.owned, descriptor)?;
            let Some(proof) = qualified.local else {
                return Ok(false);
            };
            let Some(dead) = unique(proof.dead.iter().filter(|dead| dead.drop == fact)).copied()
            else {
                return Ok(false);
            };
            self.validate_dead_local_map(dead, span)?;
            Ok(true)
        }

        pub(in crate::ssa) fn validate_local_map_carriers(&self) -> Result<(), LoweringError> {
            for state in self.map_iteration_state.locals.values() {
                let span = state.proof.drop.value_origin();
                let missing = || error(LoweringErrorKind::MissingFact, span);
                if self.bindings.contains_key(&state.proof.root)
                    || self.borrow_bindings.contains_key(&state.proof.root)
                    || state.proof.late_size.is_some() != state.size.is_some()
                {
                    return Err(missing());
                }
                validate_consumption(self.function, state, span)?;
                if let Some(size) = state.size {
                    evidence::size_use(self, state, size, span)?;
                }
            }
            Ok(())
        }
    }

    pub(in crate::ssa) fn validate_consumption(
        function: &Function,
        state: &Carrier,
        span: Span,
    ) -> Result<(), LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let consumed = state.consumed.ok_or_else(missing)?;
        let drop = function
            .instruction(consumed.instruction)
            .ok_or_else(missing)?;
        if consumed.owner != state.current
            || drop.block != consumed.block
            || drop.operation
                != (Operation::Drop {
                    owner: consumed.owner,
                })
            || drop.origin != Origin::Source(state.proof.drop.value_origin())
            || unique(function.instructions.iter().filter(|instruction| {
                instruction.operation
                    == Operation::Drop {
                        owner: consumed.owner,
                    }
            }))
            .map(|instruction| instruction.id)
                != Some(consumed.instruction)
        {
            return Err(missing());
        }
        nested::transport(
            function,
            EntityId::Value(consumed.owner),
            EntityId::Value(state.birth),
            span,
        )
    }
}
