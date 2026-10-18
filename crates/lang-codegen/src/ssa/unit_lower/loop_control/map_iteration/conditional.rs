//! Path-local Map checkpoints. Cumulative transports and exits never participate in restore.
mod validate;
use super::*;
use crate::ssa::model::{Definition, Edge};
use lang_frontend::ownership_checking::UnitIterationCleanupAction as Action;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Roles {
    block: BlockId,
    seed: EntityId,
    source: LoanId,
    cursor: ValueId,
    key: Option<LoanId>,
    value: Option<LoanId>,
    local: Option<ValueId>,
}

#[derive(Clone, Copy)]
struct MapBranchCheckpoint {
    control: UnitExpressionId,
    index: usize,
    roles: Roles,
    phase: MapPhase,
}

#[derive(Clone, Copy)]
struct Transport {
    from: Roles,
    to: Roles,
}

#[derive(Clone, Copy)]
struct ExitWitness {
    point: UnitDropPoint,
    roles: Roles,
    cleanup_start: usize,
    cleanup_end: Option<usize>,
    next: Option<ValueId>,
    returned: Option<ValueId>,
}

#[derive(Clone, Copy)]
pub(super) struct ConditionalState {
    shape: map_iteration::QualifiedConditional,
    roles: Roles,
    birth: Roles,
    checkpoint: Option<MapBranchCheckpoint>,
    transports: [[Option<Transport>; 2]; 3],
    completed: usize,
    exits: [Option<ExitWitness>; 5],
}

impl ConditionalState {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        lowerer: &UnitExpressionLowerer<'_>,
        shape: map_iteration::QualifiedConditional,
        root: UnitSymbolId,
        source_slot: usize,
        cursor_slot: usize,
        cursor_type: SsaTypeId,
        key: LoanId,
        value: LoanId,
        span: Span,
    ) -> Result<Self, LoweringError> {
        let roles = Roles::read(
            lowerer,
            root,
            source_slot,
            cursor_slot,
            cursor_type,
            Some(key),
            Some(value),
            None,
            span,
        )?;
        for (loan, operation) in [
            (
                key,
                Operation::MapIterationKeyLoan {
                    source: roles.source,
                    cursor: roles.cursor,
                },
            ),
            (
                value,
                Operation::MapIterationValueLoan {
                    source: roles.source,
                    cursor: roles.cursor,
                },
            ),
        ] {
            let instruction = carrier::instruction(lowerer.function, EntityId::Loan(loan), span)?;
            if instruction.block != roles.block || instruction.operation != operation {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
        }
        Ok(Self {
            shape,
            roles,
            birth: roles,
            checkpoint: None,
            transports: [[None; 2]; 3],
            completed: 0,
            exits: [None; 5],
        })
    }

    fn point_index(self, carrier: MapCarrier, point: UnitDropPoint) -> Option<usize> {
        self.shape
            .transfers
            .iter()
            .position(|kind| match kind {
                UnitIterationExitKind::Continue(expression)
                | UnitIterationExitKind::Break(expression)
                | UnitIterationExitKind::Return(expression) => {
                    point == UnitDropPoint::ControlTransfer(*expression)
                }
                _ => false,
            })
            .or_else(|| (point == carrier.body_point).then_some(3))
            .or_else(|| (point == carrier.exhaustion_point).then_some(4))
    }
}

impl Roles {
    #[allow(clippy::too_many_arguments)]
    fn read(
        lowerer: &UnitExpressionLowerer<'_>,
        root: UnitSymbolId,
        source_slot: usize,
        cursor_slot: usize,
        cursor_type: SsaTypeId,
        key: Option<LoanId>,
        value: Option<LoanId>,
        local: Option<ValueId>,
        span: Span,
    ) -> Result<Self, LoweringError> {
        let seed = match (
            lowerer.bindings.get(&root),
            lowerer.borrow_bindings.get(&root),
        ) {
            (Some(LoweredValue::Value(owner)), None) => EntityId::Value(*owner),
            (None, Some(loan)) => EntityId::Loan(*loan),
            _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        };
        Ok(Self {
            block: lowerer.block,
            seed,
            source: lowerer.map_pending_source(source_slot, span)?,
            cursor: lowerer.map_pending_cursor(cursor_slot, cursor_type, span)?,
            key,
            value,
            local,
        })
    }

    fn transported(
        self,
        function: &crate::ssa::model::Function,
        edge: &Edge,
        span: Span,
    ) -> Result<Self, LoweringError> {
        let entity = |before| transport_entity(function, edge, before, span);
        let EntityId::Loan(source) = entity(EntityId::Loan(self.source))? else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let EntityId::Value(cursor) = entity(EntityId::Value(self.cursor))? else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let loan = |before: Option<LoanId>| {
            before
                .map(|before| match entity(EntityId::Loan(before))? {
                    EntityId::Loan(loan) => Ok(loan),
                    _ => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
                })
                .transpose()
        };
        Ok(Self {
            block: edge.target,
            seed: entity(self.seed)?,
            source,
            cursor,
            key: loan(self.key)?,
            value: loan(self.value)?,
            local: self
                .local
                .map(|owner| match entity(EntityId::Value(owner))? {
                    EntityId::Value(owner) => Ok(owner),
                    _ => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
                })
                .transpose()?,
        })
    }
}

fn transport_entity(
    function: &crate::ssa::model::Function,
    edge: &Edge,
    before: EntityId,
    span: Span,
) -> Result<EntityId, LoweringError> {
    let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
    let parameters = &function.block(edge.target).ok_or_else(bad)?.parameters;
    if parameters.len() != edge.arguments.len() {
        return Err(bad());
    }
    let columns = edge
        .arguments
        .iter()
        .enumerate()
        .filter_map(|(index, argument)| (*argument == before).then_some(index))
        .collect::<Vec<_>>();
    let [index] = columns.as_slice() else {
        return Err(bad());
    };
    let parameter = *parameters.get(*index).ok_or_else(bad)?;
    if function.entity(parameter).map(|data| data.definition)
        != Some(Definition::BlockParameter {
            block: edge.target,
            index: *index,
        })
        || function.entity(before).map(|data| data.ty)
            != function.entity(parameter).map(|data| data.ty)
    {
        return Err(bad());
    }
    Ok(parameter)
}

impl MapState {
    pub(super) fn body_block(self) -> BlockId {
        self.conditional
            .map_or(self.carrier.active_body(), |state| state.roles.block)
    }

    pub(super) fn terminal_block(
        self,
        point: UnitDropPoint,
        span: Span,
    ) -> Result<BlockId, LoweringError> {
        if let Some(branch) = self.conditional {
            match branch.point_index(self.carrier, point) {
                Some(1 | 2) => Ok(branch.roles.block),
                Some(4) if branch.roles.block == self.carrier.exhaustion => Ok(branch.roles.block),
                _ => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
            }
        } else {
            self.carrier.terminal_block(point, span)
        }
    }
}

impl UnitExpressionLowerer<'_> {
    fn conditional_map(&self) -> Option<(UnitStatementId, MapState)> {
        self.loops
            .iter()
            .rev()
            .filter_map(|context| context.iteration.as_ref())
            .find_map(|iteration| match &iteration.state {
                IterationState::Map(state) if state.conditional.is_some() => {
                    Some((iteration.statement, **state))
                }
                _ => None,
            })
    }

    fn conditional_map_mut(
        &mut self,
        statement: UnitStatementId,
        span: Span,
    ) -> Result<&mut ConditionalState, LoweringError> {
        self.loops
            .iter_mut()
            .rev()
            .filter_map(|context| context.iteration.as_mut())
            .find_map(|iteration| match &mut iteration.state {
                IterationState::Map(state) if iteration.statement == statement => {
                    state.conditional.as_mut()
                }
                _ => None,
            })
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
    }

    pub(super) fn validate_map_state_current(
        &self,
        state: MapState,
        block: BlockId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(branch) = state.conditional else {
            return state.carrier.validate_current(
                self,
                state.source_slot,
                state.cursor_slot,
                block,
                span,
            );
        };
        let actual = Roles::read(
            self,
            state.carrier.identity.parameter_root(span)?,
            state.source_slot,
            state.cursor_slot,
            state.cursor_type,
            branch.roles.key,
            branch.roles.value,
            branch.roles.local,
            span,
        )?;
        if actual != branch.roles || actual.block != block {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    fn validate_map_live_components(
        &self,
        state: MapState,
        span: Span,
    ) -> Result<(), LoweringError> {
        let branch = state
            .conditional
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let (local, _) = branch
            .shape
            .local
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if branch.roles.local.is_none()
            || self.bindings.get(&local).copied() != branch.roles.local.map(LoweredValue::Value)
            || !matches!(
                state.phase,
                MapPhase::Body {
                    key_live: true,
                    value_live: true
                }
            )
            || branch.roles.key != Some(self.map_pending_source(state.key_slot, span)?)
            || branch.roles.value != Some(self.map_pending_source(state.value_slot, span)?)
            || self.borrow_bindings.get(&state.key_binding).copied() != branch.roles.key
            || self.borrow_bindings.get(&state.value_binding).copied() != branch.roles.value
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_map_state_current(state, branch.roles.block, span)
    }

    pub(in super::super::super) fn checkpoint_map_conditional(
        &mut self,
        control: UnitExpressionId,
        result_required: bool,
        has_else: bool,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some((statement, mut state)) = self.conditional_map() else {
            return Ok(());
        };
        let mut branch = state
            .conditional
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if result_required
            || has_else
            || branch.shape.controls.get(branch.completed) != Some(&control)
            || branch.checkpoint.is_some()
            || !self.temporaries.is_empty()
            || !self.pending_call_frames.is_empty()
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        if branch.completed == 0 && branch.roles.local.is_none() {
            let owner = self.conditional_map_local_birth(branch, span)?;
            branch.roles.local = Some(owner);
            branch.birth.local = Some(owner);
            let actual = self.conditional_map_mut(statement, span)?;
            actual.roles.local = Some(owner);
            actual.birth.local = Some(owner);
            state.conditional = Some(branch);
        }
        self.validate_map_live_components(state, span)?;
        // Called only after real condition lowering has ended its loans and temporary owner.
        self.conditional_map_mut(statement, span)?.checkpoint = Some(MapBranchCheckpoint {
            control,
            index: branch.completed,
            roles: branch.roles,
            phase: state.phase,
        });
        Ok(())
    }

    pub(in super::super::super) fn enter_map_conditional_arm(
        &mut self,
        point: UnitDropPoint,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some((statement, state)) = self.conditional_map() else {
            return Ok(());
        };
        let UnitDropPoint::BranchExit {
            control,
            branch: index,
        } = point
        else {
            return Ok(());
        };
        let branch = state
            .conditional
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let checkpoint = branch
            .checkpoint
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if checkpoint.control != control
            || index > 1
            || branch.transports[checkpoint.index][index].is_some()
            || (index == 1 && branch.transports[checkpoint.index][0].is_none())
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let Some(TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        }) = self
            .function
            .block(checkpoint.roles.block)
            .and_then(|block| block.terminator.as_ref())
            .map(|term| &term.kind)
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let edge = if index == 0 { when_true } else { when_false };
        let roles = checkpoint.roles.transported(self.function, edge, span)?;
        if self.block != roles.block {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        // Restore only this path's phase and current roles, preserving all earlier exits/Nexts.
        self.set_map_phase(statement, checkpoint.phase, span)?;
        let actual = self.conditional_map_mut(statement, span)?;
        actual.roles = roles;
        actual.transports[checkpoint.index][index] = Some(Transport {
            from: checkpoint.roles,
            to: roles,
        });
        self.validate_map_live_components(self.map_state(statement, span)?, span)
    }

    pub(in super::super::super) fn finish_map_conditional_true(
        &mut self,
        control: UnitExpressionId,
        block: BlockId,
        diverged: bool,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some((statement, state)) = self.conditional_map() else {
            return Ok(());
        };
        let branch = state
            .conditional
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let checkpoint = branch
            .checkpoint
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let witness = branch.exits[checkpoint.index]
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if checkpoint.control != control
            || !diverged
            || self.block != block
            || witness.roles.block != block
            || witness.cleanup_end.is_none()
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if checkpoint.index == 2 {
            let result = self.validate_conditional_map_return(state, witness, span)?;
            self.conditional_map_mut(statement, span)?.exits[2]
                .as_mut()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
                .returned = Some(result);
        } else if checkpoint.index == 0 && witness.next.is_none() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    pub(in super::super::super) fn validate_map_conditional_exits(
        &self,
        control: UnitExpressionId,
        exits: &[BranchExit],
        false_block: BlockId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some((_, state)) = self.conditional_map() else {
            return Ok(());
        };
        let branch = state
            .conditional
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if branch
            .checkpoint
            .is_none_or(|checkpoint| checkpoint.control != control)
            || !matches!(exits, [exit] if exit.block == false_block && exit.result == LoweredValue::Unit)
        {
            return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
        }
        Ok(())
    }

    pub(in super::super::super) fn finish_map_conditional_merge(
        &mut self,
        control: UnitExpressionId,
        false_block: BlockId,
        result: LoweredValue,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some((statement, state)) = self.conditional_map() else {
            return Ok(());
        };
        let branch = state
            .conditional
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let checkpoint = branch
            .checkpoint
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        if checkpoint.control != control
            || result != LoweredValue::Unit
            || self.block != false_block
            || branch.transports[checkpoint.index][1]
                .is_none_or(|transport| transport.to != branch.roles)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.validate_map_live_components(state, span)?;
        let branch = self.conditional_map_mut(statement, span)?;
        branch.completed += 1;
        branch.checkpoint = None;
        Ok(())
    }

    pub(super) fn restore_conditional_map_exhaustion(
        &self,
        state: &mut MapState,
        span: Span,
    ) -> Result<(), LoweringError> {
        if state.conditional.is_none() {
            return Ok(());
        }
        state.carrier.validate_current(
            self,
            state.source_slot,
            state.cursor_slot,
            state.carrier.exhaustion,
            span,
        )?;
        let roles = Roles::read(
            self,
            state.carrier.identity.parameter_root(span)?,
            state.source_slot,
            state.cursor_slot,
            state.cursor_type,
            None,
            None,
            None,
            span,
        )?;
        if let Some(branch) = state.conditional.as_mut() {
            branch.roles = roles;
        }
        Ok(())
    }

    pub(super) fn map_terminal_owner(
        &self,
        state: MapState,
        point: UnitDropPoint,
        span: Span,
    ) -> Result<ValueId, LoweringError> {
        let Some(branch) = state.conditional else {
            return state.carrier.terminal_owner(self, point, span);
        };
        let EntityId::Value(owner) = branch.roles.seed else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if self.block != state.terminal_block(point, span)?
            || state.carrier.identity.current(self, span)? != EntityId::Value(owner)
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(owner)
    }

    pub(in super::super) fn begin_conditional_map_cleanup(
        &mut self,
        point: UnitDropPoint,
    ) -> Result<(), LoweringError> {
        let Some((statement, state)) = self.conditional_map() else {
            return Ok(());
        };
        let branch = state.conditional.ok_or(LoweringError {
            kind: LoweringErrorKind::MissingFact,
            span: None,
        })?;
        let Some(index) = branch.point_index(state.carrier, point) else {
            return Ok(());
        };
        let span = self.statement_span(statement.statement())?;
        if branch.exits[index].is_some() || (index == 3 && branch.completed != 3) {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        if index < 4 {
            self.validate_map_live_components(state, span)?;
        } else {
            self.validate_map_state_current(state, state.carrier.exhaustion, span)?;
        }
        let cleanup_start = self
            .function
            .block(self.block)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
            .instructions
            .len();
        self.conditional_map_mut(statement, span)?.exits[index] = Some(ExitWitness {
            point,
            roles: branch.roles,
            cleanup_start,
            cleanup_end: None,
            next: None,
            returned: None,
        });
        Ok(())
    }

    pub(super) fn record_conditional_map_next(
        &mut self,
        state: MapState,
        next: ValueId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(branch) = state.conditional else {
            return Ok(());
        };
        let candidates = branch
            .exits
            .iter()
            .enumerate()
            .filter_map(|(index, witness)| {
                witness
                    .filter(|witness| witness.roles.block == self.block)
                    .map(|witness| (index, witness))
            })
            .collect::<Vec<_>>();
        let [(index, witness)] = candidates.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        if ![0, 3].contains(index) || witness.next.is_some() || witness.cleanup_end.is_none() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let (statement, _) = self
            .conditional_map()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        self.conditional_map_mut(statement, span)?.exits[*index]
            .as_mut()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?
            .next = Some(next);
        Ok(())
    }
}
