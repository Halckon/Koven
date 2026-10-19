//! Ordinary branch checkpoints are Copy path state; evidence is separately owned and append-only.
mod stack;
mod validate;
use super::*;
use crate::ssa::model::Edge;
use lang_frontend::ownership_checking::{UnitDropTarget, UnitIterationCleanupAction as Action};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in super::super::super) struct Roles {
    block: BlockId,
    seed: EntityId,
    source: LoanId,
    cursor: ValueId,
    key: Option<LoanId>,
    value: Option<LoanId>,
}
#[derive(Clone, Copy)]
pub(in super::super::super) struct MapBranchCheckpoint {
    statement: UnitStatementId,
    roles: Roles,
    phase: MapPhase,
}
#[derive(Clone, Copy)]
struct Transport {
    from: Roles,
    to: Roles,
}
struct CleanupStart {
    point: UnitDropPoint,
    kind: UnitIterationExitKind,
    roles: Roles,
    position: usize,
    operations: Vec<(Operation, Option<Origin>)>,
    delegated: bool,
}
#[derive(Clone, Copy)]
struct ReturnDelivery {
    point: UnitDropPoint,
    result: LoweredValue,
    block: BlockId,
    position: usize,
}
#[derive(Default)]
pub(super) struct Evidence {
    transports: Vec<Transport>,
    starts: Vec<CleanupStart>,
    cleanups: Vec<(usize, usize)>,
    nexts: Vec<(usize, ValueId)>,
    returns: Vec<ReturnDelivery>,
}
#[derive(Clone, Copy)]
pub(super) struct ConditionalState {
    shape: map_iteration::QualifiedConditional,
    roles: Roles,
    birth: Roles,
    cleanup: Option<usize>,
    exit: Option<UnitIterationExitKind>,
}
impl ConditionalState {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        lowerer: &UnitExpressionLowerer<'_>,
        shape: map_iteration::QualifiedConditional,
        source_slot: usize,
        cursor_slot: usize,
        cursor_type: SsaTypeId,
        key: LoanId,
        value: LoanId,
        span: Span,
    ) -> Result<Self, LoweringError> {
        let roles = Roles::read(
            lowerer,
            shape.source,
            source_slot,
            cursor_slot,
            cursor_type,
            Some(key),
            Some(value),
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
            cleanup: None,
            exit: None,
        })
    }
}
impl Roles {
    #[allow(clippy::too_many_arguments)]
    fn read(
        lowerer: &UnitExpressionLowerer<'_>,
        identity: SourceIdentity,
        source_slot: usize,
        cursor_slot: usize,
        cursor_type: SsaTypeId,
        key: Option<LoanId>,
        value: Option<LoanId>,
        span: Span,
    ) -> Result<Self, LoweringError> {
        Ok(Self {
            block: lowerer.block,
            seed: identity.current(lowerer, span)?,
            source: lowerer.map_pending_source(source_slot, span)?,
            cursor: lowerer.map_pending_cursor(cursor_slot, cursor_type, span)?,
            key,
            value,
        })
    }
    fn from_exit(state: MapState, exit: &BranchExit, span: Span) -> Result<Self, LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let seed = match state.carrier.identity {
            SourceIdentity::BindingChoice(proof) => {
                match (
                    exit.bindings.get(&proof.root),
                    exit.borrow_bindings.get(&proof.root),
                ) {
                    (Some(LoweredValue::Value(owner)), None) => EntityId::Value(*owner),
                    _ => return Err(bad()),
                }
            }
            SourceIdentity::BorrowedBinding(proof) => {
                if exit.bindings.contains_key(&proof.binding) {
                    return Err(bad());
                }
                EntityId::Loan(*exit.borrow_bindings.get(&proof.binding).ok_or_else(bad)?)
            }
            SourceIdentity::Temporary(e) => exit
                .temporaries
                .get(&e)
                .copied()
                .map(EntityId::Value)
                .ok_or_else(bad)?,
            SourceIdentity::Parameter { root, .. }
            | SourceIdentity::Local { root, .. }
            | SourceIdentity::Formed { root, .. } => {
                match (exit.bindings.get(&root), exit.borrow_bindings.get(&root)) {
                    (Some(LoweredValue::Value(owner)), None) => EntityId::Value(*owner),
                    (None, Some(loan)) => EntityId::Loan(*loan),
                    _ => return Err(bad()),
                }
            }
        };
        let (Some(EntityId::Loan(source)), Some(EntityId::Value(cursor))) = (
            exit.pending_operands.get(state.source_slot),
            exit.pending_operands.get(state.cursor_slot),
        ) else {
            return Err(bad());
        };
        Ok(Self {
            block: exit.block,
            seed,
            source: *source,
            cursor: *cursor,
            key: Some(
                *exit
                    .borrow_bindings
                    .get(&state.key_binding)
                    .ok_or_else(bad)?,
            ),
            value: Some(
                *exit
                    .borrow_bindings
                    .get(&state.value_binding)
                    .ok_or_else(bad)?,
            ),
        })
    }
    fn transported(
        self,
        function: &crate::ssa::model::Function,
        edge: &Edge,
        span: Span,
    ) -> Result<Self, LoweringError> {
        let entity = |e| transport::along(function, edge, e, span);
        let EntityId::Loan(source) = entity(EntityId::Loan(self.source))? else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let EntityId::Value(cursor) = entity(EntityId::Value(self.cursor))? else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let loan = |before: Option<LoanId>| {
            before
                .map(|l| match entity(EntityId::Loan(l))? {
                    EntityId::Loan(l) => Ok(l),
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
        })
    }
}
impl MapState {
    pub(super) fn body_block(self) -> BlockId {
        self.conditional
            .map_or(self.carrier.active_body(), |s| s.roles.block)
    }
    pub(super) fn terminal_block(
        self,
        point: UnitDropPoint,
        span: Span,
    ) -> Result<BlockId, LoweringError> {
        if let Some(branch) = self.conditional {
            if matches!(branch.exit, Some(UnitIterationExitKind::Break(e) | UnitIterationExitKind::Return(e))
                if point == UnitDropPoint::ControlTransfer(e))
                || (branch.exit == Some(UnitIterationExitKind::Exhaustion)
                    && point == self.carrier.exhaustion_point
                    && branch.roles.block == self.carrier.exhaustion)
            {
                return Ok(branch.roles.block);
            }
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        self.carrier.terminal_block(point, span)
    }
    pub(super) fn is_body_point(
        self,
        owned: &lang_frontend::ownership_checking::CompilationUnitOwnership,
        point: UnitDropPoint,
        span: Span,
    ) -> Result<bool, LoweringError> {
        let Some(branch) = self.conditional else {
            return Ok(self.carrier.is_body_point(point));
        };
        let plan = owned
            .map_iteration(branch.shape.statement)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let exits = plan
            .exits()
            .iter()
            .filter(|e| e.point() == point)
            .collect::<Vec<_>>();
        match exits.as_slice() {
            [] => Ok(false),
            [e] => Ok(e.kind() != UnitIterationExitKind::Exhaustion),
            _ => Err(lowering_error(LoweringErrorKind::MissingFact, span)),
        }
    }
}
impl UnitExpressionLowerer<'_> {
    fn conditional_map(&self) -> Option<(UnitStatementId, MapState)> {
        self.loops
            .iter()
            .rev()
            .filter_map(|c| c.iteration.as_ref())
            .find_map(|i| match &i.state {
                IterationState::Map(s) if s.conditional.is_some() => Some((i.statement, s.path)),
                _ => None,
            })
    }
    pub(super) fn conditional_runtime_mut(
        &mut self,
        statement: UnitStatementId,
        span: Span,
    ) -> Result<&mut MapRuntime, LoweringError> {
        self.loops
            .iter_mut()
            .rev()
            .filter_map(|c| c.iteration.as_mut())
            .find_map(|i| match &mut i.state {
                IterationState::Map(s) if i.statement == statement => Some(s.as_mut()),
                _ => None,
            })
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))
    }
    fn conditional_map_mut(
        &mut self,
        statement: UnitStatementId,
        span: Span,
    ) -> Result<&mut ConditionalState, LoweringError> {
        self.conditional_runtime_mut(statement, span)?
            .conditional
            .as_mut()
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
            state.carrier.identity,
            state.source_slot,
            state.cursor_slot,
            state.cursor_type,
            branch.roles.key,
            branch.roles.value,
            span,
        )?;
        if actual != branch.roles
            || actual.block != block
            || branch.shape.source != state.carrier.identity
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        let [_, source, cursor] = state.carrier.header_entities(self.function, span)?;
        for (current, anchor) in [
            (actual.seed, state.carrier.anchor),
            (EntityId::Loan(actual.source), source),
            (EntityId::Value(actual.cursor), cursor),
        ] {
            transport::from_anchor(self.function, current, anchor, span)?;
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
        if !matches!(
            state.phase,
            MapPhase::Body {
                key_live: true,
                value_live: true
            }
        ) || branch.roles.key != Some(self.map_pending_source(state.key_slot, span)?)
            || branch.roles.value != Some(self.map_pending_source(state.value_slot, span)?)
            || self.borrow_bindings.get(&state.key_binding).copied() != branch.roles.key
            || self.borrow_bindings.get(&state.value_binding).copied() != branch.roles.value
        {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        for (now, birth) in [
            (branch.roles.key, branch.birth.key),
            (branch.roles.value, branch.birth.value),
        ] {
            let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
            transport::from_anchor(
                self.function,
                EntityId::Loan(now.ok_or_else(bad)?),
                EntityId::Loan(birth.ok_or_else(bad)?),
                span,
            )?;
        }
        self.validate_map_state_current(state, branch.roles.block, span)
    }

    pub(super) fn resume_conditional_map_checked(
        &mut self,
        state: MapState,
        from: BlockId,
        before_borrows: &BTreeMap<UnitSymbolId, LoanId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let branch = state.conditional.ok_or_else(bad)?;
        if branch.roles.block != from
            || !matches!(
                state.phase,
                MapPhase::Body {
                    key_live: true,
                    value_live: true
                }
            )
            || before_borrows.get(&state.key_binding).copied() != branch.roles.key
            || before_borrows.get(&state.value_binding).copied() != branch.roles.value
        {
            return Err(bad());
        }
        let roles = Roles::read(
            self,
            state.carrier.identity,
            state.source_slot,
            state.cursor_slot,
            state.cursor_type,
            Some(self.map_pending_source(state.key_slot, span)?),
            Some(self.map_pending_source(state.value_slot, span)?),
            span,
        )?;
        let incoming = transport::incoming(self.function, self.block);
        let [(origin, edge)] = incoming.as_slice() else {
            return Err(bad());
        };
        if *origin != from || branch.roles.transported(self.function, edge, span)? != roles {
            return Err(bad());
        }
        let Some(TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        }) = self
            .function
            .block(from)
            .and_then(|b| b.terminator.as_ref())
            .map(|t| &t.kind)
        else {
            return Err(bad());
        };
        let abort = if when_true.target == roles.block {
            when_false.target
        } else if when_false.target == roles.block {
            when_true.target
        } else {
            return Err(bad());
        };
        if !matches!(
            self.function
                .block(abort)
                .and_then(|b| b.terminator.as_ref())
                .map(|t| &t.kind),
            Some(TerminatorKind::Abort)
        ) {
            return Err(bad());
        }
        let runtime = self.conditional_runtime_mut(branch.shape.statement, span)?;
        runtime.conditional.as_mut().ok_or_else(bad)?.roles = roles;
        runtime.evidence.transports.push(Transport {
            from: branch.roles,
            to: roles,
        });
        self.validate_map_live_components(self.map_state(branch.shape.statement, span)?, span)
    }
    fn checkpoint_map_frame(
        &mut self,
        statement: UnitStatementId,
        state: MapState,
        span: Span,
    ) -> Result<Option<MapBranchCheckpoint>, LoweringError> {
        let roles = Roles::read(
            self,
            state.carrier.identity,
            state.source_slot,
            state.cursor_slot,
            state.cursor_type,
            Some(self.map_pending_source(state.key_slot, span)?),
            Some(self.map_pending_source(state.value_slot, span)?),
            span,
        )?;
        self.conditional_map_mut(statement, span)?.roles = roles;
        self.validate_map_live_components(self.map_state(statement, span)?, span)?;
        Ok(Some(MapBranchCheckpoint {
            statement,
            roles,
            phase: state.phase,
        }))
    }
    fn enter_map_frame_arm(
        &mut self,
        checkpoint: Option<MapBranchCheckpoint>,
        target: BlockId,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(checkpoint) = checkpoint else {
            return Ok(());
        };
        let block = self
            .function
            .block(checkpoint.roles.block)
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        let Some(TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        }) = block.terminator.as_ref().map(|t| &t.kind)
        else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let edges = [when_true, when_false]
            .into_iter()
            .filter(|e| e.target == target)
            .collect::<Vec<_>>();
        let [edge] = edges.as_slice() else {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        };
        let roles = checkpoint.roles.transported(self.function, edge, span)?;
        let runtime = self.conditional_runtime_mut(checkpoint.statement, span)?;
        runtime.phase = checkpoint.phase;
        let path = runtime
            .conditional
            .as_mut()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        path.roles = roles;
        path.cleanup = None;
        path.exit = None;
        runtime.evidence.transports.push(Transport {
            from: checkpoint.roles,
            to: roles,
        });
        Ok(())
    }
    fn map_frame_exits(
        &self,
        checkpoint: Option<MapBranchCheckpoint>,
        exits: &[BranchExit],
        span: Span,
    ) -> Result<Vec<Roles>, LoweringError> {
        let Some(checkpoint) = checkpoint else {
            return Ok(Vec::new());
        };
        let state = self.map_state(checkpoint.statement, span)?;
        exits
            .iter()
            .map(|e| Roles::from_exit(state, e, span))
            .collect()
    }
    fn finish_map_frame_merge(
        &mut self,
        checkpoint: Option<MapBranchCheckpoint>,
        exits: Vec<Roles>,
        result: LoweredValue,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(checkpoint) = checkpoint else {
            return Ok(());
        };
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        if result == LoweredValue::Diverged {
            if !exits.is_empty() {
                return Err(bad());
            }
            return Ok(());
        }
        let state = self.map_state(checkpoint.statement, span)?;
        let roles = Roles::read(
            self,
            state.carrier.identity,
            state.source_slot,
            state.cursor_slot,
            state.cursor_type,
            Some(self.map_pending_source(state.key_slot, span)?),
            Some(self.map_pending_source(state.value_slot, span)?),
            span,
        )?;
        if exits.is_empty() {
            return Err(bad());
        }
        let mut transports = Vec::new();
        if exits.len() == 1 {
            if exits[0] != roles {
                return Err(bad());
            }
        } else {
            let incoming = transport::incoming(self.function, roles.block);
            if incoming.len() != exits.len() {
                return Err(bad());
            }
            for before in exits {
                let edges = incoming
                    .iter()
                    .filter(|(b, _)| *b == before.block)
                    .collect::<Vec<_>>();
                let [(_, edge)] = edges.as_slice() else {
                    return Err(bad());
                };
                if before.transported(self.function, edge, span)? != roles {
                    return Err(bad());
                }
                transports.push(Transport {
                    from: before,
                    to: roles,
                });
            }
        }
        let runtime = self.conditional_runtime_mut(checkpoint.statement, span)?;
        runtime.phase = checkpoint.phase;
        let branch = runtime.conditional.as_mut().ok_or_else(bad)?;
        branch.roles = roles;
        branch.cleanup = None;
        branch.exit = None;
        runtime.evidence.transports.extend(transports);
        self.validate_map_live_components(self.map_state(checkpoint.statement, span)?, span)
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
            state.carrier.identity,
            state.source_slot,
            state.cursor_slot,
            state.cursor_type,
            None,
            None,
            span,
        )?;
        if let Some(branch) = state.conditional.as_mut() {
            branch.roles = roles;
            branch.cleanup = None;
            branch.exit = Some(UnitIterationExitKind::Exhaustion);
        }
        Ok(())
    }
    pub(super) fn map_cached_seed(&self, state: MapState) -> Option<EntityId> {
        state.conditional.map(|branch| branch.roles.seed)
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
    fn begin_map_frame_cleanup(
        &mut self,
        statement: UnitStatementId,
        state: MapState,
        point: UnitDropPoint,
    ) -> Result<(), LoweringError> {
        let span = self.statement_span(statement.statement())?;
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let branch = state.conditional.ok_or_else(bad)?;
        let plan = self.owned.map_iteration(statement).ok_or_else(bad)?;
        let exits = plan
            .exits()
            .iter()
            .filter(|e| e.point() == point)
            .collect::<Vec<_>>();
        let exit = match exits.as_slice() {
            [] => return Ok(()),
            [e] => *e,
            _ => return Err(bad()),
        };
        if exit.kind() == UnitIterationExitKind::Exhaustion {
            self.validate_map_state_current(state, state.carrier.exhaustion, span)?;
        } else {
            self.validate_map_live_components(state, span)?;
        }
        let mut operations = Vec::new();
        let mut delegated = false;
        for action in self.owned.iteration_cleanup_at(point).ok_or_else(bad)? {
            let expected = match action {
                Action::Drop(fact) => {
                    let owner = match fact.target() {
                        UnitDropTarget::Named(s) => match self.bindings.get(&s) {
                            Some(LoweredValue::Value(v)) => *v,
                            _ => return Err(bad()),
                        },
                        UnitDropTarget::Temporary(e) => {
                            *self.temporaries.get(&e).ok_or_else(bad)?
                        }
                        _ => {
                            delegated = true;
                            continue;
                        }
                    };
                    (
                        Operation::Drop { owner },
                        Some(Origin::Source(fact.value_origin())),
                    )
                }
                Action::EndBinding {
                    statement: s,
                    symbol,
                } if *s == statement => {
                    let loan = if *symbol == state.value_binding {
                        branch.roles.value
                    } else if *symbol == state.key_binding {
                        branch.roles.key
                    } else {
                        None
                    };
                    (
                        Operation::BorrowEnd {
                            loan: loan.ok_or_else(bad)?,
                        },
                        None,
                    )
                }
                Action::FinishProvider(s) if *s == statement => continue,
                Action::EndSource(s) if *s == statement => (
                    Operation::BorrowEnd {
                        loan: branch.roles.source,
                    },
                    None,
                ),
                _ => {
                    delegated = true;
                    continue;
                }
            };
            operations.push(expected);
        }
        let position = self
            .function
            .block(self.block)
            .ok_or_else(bad)?
            .instructions
            .len();
        let kind = exit.kind();
        let runtime = self.conditional_runtime_mut(statement, span)?;
        if runtime.evidence.starts.iter().any(|w| w.point == point) {
            return Err(bad());
        }
        let index = runtime.evidence.starts.len();
        runtime.evidence.starts.push(CleanupStart {
            point,
            kind,
            roles: branch.roles,
            position,
            operations,
            delegated,
        });
        let branch = runtime.conditional.as_mut().ok_or_else(bad)?;
        branch.cleanup = Some(index);
        branch.exit = Some(kind);
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
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let index = branch.cleanup.ok_or_else(bad)?;
        let (statement, _) = self.conditional_map().ok_or_else(bad)?;
        let runtime = self.conditional_runtime_mut(statement, span)?;
        if !matches!(
            branch.exit,
            Some(UnitIterationExitKind::Fallthrough | UnitIterationExitKind::Continue(_))
        ) || runtime.evidence.nexts.iter().any(|(i, _)| *i == index)
            || !runtime.evidence.cleanups.iter().any(|(i, _)| *i == index)
        {
            return Err(bad());
        }
        runtime.evidence.nexts.push((index, next));
        Ok(())
    }
}
