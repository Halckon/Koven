//! Current local owners, their one final consumption, and two-selector condition checks.
use super::resource::evidence;
use super::*;
use crate::ssa::model::InstructionId;
use lang_frontend::ownership_checking::{DropFact, DropTarget};

pub(in crate::ssa::lower_frontend::map_iteration) mod conditions {
    use super::super::*;
    use super::local::{Path, Proof, identity};
    use lang_frontend::ownership_checking::{
        CleanupConditionId, CleanupConditions, CleanupSelection, CleanupSelectorId,
        CleanupSelectorSource,
    };

    #[derive(Clone, Copy)]
    pub(in crate::ssa::lower_frontend::map_iteration) enum Expected {
        Path,
        Presence,
        Conjunction,
    }

    pub(in crate::ssa::lower_frontend::map_iteration) fn path(
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

    pub(in crate::ssa::lower_frontend::map_iteration) fn check(
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
pub(in crate::ssa::lower_frontend::map_iteration) struct Consumption {
    pub(in crate::ssa::lower_frontend::map_iteration) block: BlockId,
    pub(in crate::ssa::lower_frontend::map_iteration) owner: ValueId,
    pub(in crate::ssa::lower_frontend::map_iteration) instruction: InstructionId,
}

#[derive(Clone)]
pub(super) struct Carrier {
    pub(in crate::ssa::lower_frontend::map_iteration) proof: local::Proof,
    pub(in crate::ssa::lower_frontend::map_iteration) statement: StatementId,
    pub(in crate::ssa::lower_frontend::map_iteration) birth: ValueId,
    pub(in crate::ssa::lower_frontend::map_iteration) current: ValueId,
    pub(in crate::ssa::lower_frontend::map_iteration) continuation: BlockId,
    pub(in crate::ssa::lower_frontend::map_iteration) source: LoanId,
    pub(in crate::ssa::lower_frontend::map_iteration) consumed: Option<Consumption>,
    pub(in crate::ssa::lower_frontend::map_iteration) size: Option<ValueId>,
}

impl Carrier {
    pub(super) fn root(&self) -> SymbolId {
        self.proof.root
    }
}

impl ExpressionLowerer<'_> {
    pub(super) fn local_map_seed(
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

    pub(super) fn validate_local_map_entry(
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

    pub(super) fn local_map_completion(
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
        let instructions = carrier::instructions(self.function, self.block).ok_or_else(missing)?;
        let source_end = unique(instructions.iter().enumerate().filter_map(
            |(index, instruction)| {
                (instruction.operation == Operation::BorrowEnd { loan: source }).then_some(index)
            },
        ))
        .ok_or_else(missing)?;
        let drops = instructions
            .iter()
            .enumerate()
            .filter(|(_, instruction)| instruction.operation == Operation::Drop { owner: current })
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

    pub(in super::super) fn emit_local_map_drop(
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
        let Some(dead) = unique(proof.dead.iter().filter(|dead| dead.drop == fact)).copied() else {
            return Ok(false);
        };
        self.validate_dead_local_map(dead, span)?;
        Ok(true)
    }

    pub(in super::super) fn validate_local_map_carriers(&self) -> Result<(), LoweringError> {
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

pub(in crate::ssa::lower_frontend::map_iteration) fn validate_consumption(
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
