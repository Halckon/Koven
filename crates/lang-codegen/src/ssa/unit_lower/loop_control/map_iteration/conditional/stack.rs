//! Every active provider follows ordinary CFG edges; only Copy path state is restored.
use super::*;

pub(in super::super::super::super) struct FrameExits {
    statement: UnitStatementId,
    roles: Vec<Roles>,
}

impl UnitExpressionLowerer<'_> {
    fn conditional_frames(&self) -> Vec<(UnitStatementId, MapState)> {
        self.loops
            .iter()
            .filter_map(|c| c.iteration.as_ref())
            .filter_map(|i| match &i.state {
                IterationState::Map(s) if s.conditional.is_some() => Some((i.statement, s.path)),
                _ => None,
            })
            .collect()
    }

    pub(in super::super::super::super) fn checkpoint_map_conditional(
        &mut self,
        _control: UnitExpressionId,
        _result_required: bool,
        _has_else: bool,
        span: Span,
    ) -> Result<Vec<MapBranchCheckpoint>, LoweringError> {
        self.checkpoint_map_stack(span)
    }

    pub(in super::super) fn checkpoint_map_stack(
        &mut self,
        span: Span,
    ) -> Result<Vec<MapBranchCheckpoint>, LoweringError> {
        let mut checkpoints = Vec::new();
        for (statement, state) in self.conditional_frames() {
            let checkpoint = self
                .checkpoint_map_frame(statement, state, span)?
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            checkpoints.push(checkpoint);
        }
        Ok(checkpoints)
    }

    pub(in super::super::super::super) fn enter_map_conditional_arm(
        &mut self,
        checkpoints: &[MapBranchCheckpoint],
        target: BlockId,
        span: Span,
    ) -> Result<(), LoweringError> {
        for checkpoint in checkpoints {
            self.enter_map_frame_arm(Some(*checkpoint), target, span)?;
        }
        Ok(())
    }

    pub(in super::super::super::super) fn map_conditional_exits(
        &self,
        checkpoints: &[MapBranchCheckpoint],
        exits: &[BranchExit],
        span: Span,
    ) -> Result<Vec<FrameExits>, LoweringError> {
        checkpoints
            .iter()
            .map(|checkpoint| {
                Ok(FrameExits {
                    statement: checkpoint.statement,
                    roles: self.map_frame_exits(Some(*checkpoint), exits, span)?,
                })
            })
            .collect()
    }

    pub(in super::super::super::super) fn finish_map_conditional_merge(
        &mut self,
        checkpoints: &[MapBranchCheckpoint],
        exits: Vec<FrameExits>,
        result: LoweredValue,
        span: Span,
    ) -> Result<(), LoweringError> {
        if checkpoints.len() != exits.len() {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        for (checkpoint, exit) in checkpoints.iter().zip(exits) {
            if checkpoint.statement != exit.statement {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            self.finish_map_frame_merge(Some(*checkpoint), exit.roles, result, span)?;
        }
        Ok(())
    }

    pub(in super::super) fn transport_map_stack(
        &mut self,
        checkpoints: &[MapBranchCheckpoint],
        edge: &Edge,
        span: Span,
    ) -> Result<Vec<MapBranchCheckpoint>, LoweringError> {
        let mut result = Vec::new();
        for checkpoint in checkpoints {
            let state = self.map_state(checkpoint.statement, span)?;
            let roles = checkpoint.roles.transported(self.function, edge, span)?;
            let actual = Roles::read(
                self,
                state.carrier.identity,
                state.source_slot,
                state.cursor_slot,
                state.cursor_type,
                Some(self.map_pending_source(state.key_slot, span)?),
                Some(self.map_pending_source(state.value_slot, span)?),
                span,
            )?;
            if actual != roles {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            self.install_map_path(*checkpoint, roles, span)?;
            result.push(MapBranchCheckpoint {
                roles,
                ..*checkpoint
            });
        }
        Ok(result)
    }

    fn install_map_path(
        &mut self,
        checkpoint: MapBranchCheckpoint,
        roles: Roles,
        span: Span,
    ) -> Result<(), LoweringError> {
        let runtime = self.conditional_runtime_mut(checkpoint.statement, span)?;
        runtime.phase = checkpoint.phase;
        let branch = runtime
            .conditional
            .as_mut()
            .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
        branch.roles = roles;
        branch.cleanup = None;
        branch.exit = None;
        runtime.evidence.transports.push(Transport {
            from: checkpoint.roles,
            to: roles,
        });
        Ok(())
    }

    pub(in super::super) fn restore_map_stack(
        &mut self,
        checkpoints: &[MapBranchCheckpoint],
        span: Span,
    ) -> Result<(), LoweringError> {
        for checkpoint in checkpoints {
            let state = self.map_state(checkpoint.statement, span)?;
            let actual = Roles::read(
                self,
                state.carrier.identity,
                state.source_slot,
                state.cursor_slot,
                state.cursor_type,
                Some(self.map_pending_source(state.key_slot, span)?),
                Some(self.map_pending_source(state.value_slot, span)?),
                span,
            )?;
            if actual != checkpoint.roles {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            // A sibling path changes only phase/current identities, never cumulative evidence.
            let runtime = self.conditional_runtime_mut(checkpoint.statement, span)?;
            runtime.phase = checkpoint.phase;
            let branch = runtime
                .conditional
                .as_mut()
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            branch.roles = checkpoint.roles;
            branch.cleanup = None;
            branch.exit = None;
        }
        Ok(())
    }

    pub(in super::super) fn validate_map_stack_backedges(
        &mut self,
        checkpoints: &[MapBranchCheckpoint],
        context: &LoopContext,
        span: Span,
    ) -> Result<(), LoweringError> {
        for checkpoint in checkpoints {
            let state = self.map_state(checkpoint.statement, span)?;
            let mut transports = Vec::new();
            let incoming = transport::incoming(self.function, context.header);
            if incoming.len() != context.continues.len() + 1 {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            for jump in &context.continues {
                let before = Roles::from_exit(state, &jump.state, span)?;
                let edges = incoming
                    .iter()
                    .filter(|(b, _)| *b == before.block)
                    .collect::<Vec<_>>();
                let [(_, edge)] = edges.as_slice() else {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                };
                if before.transported(self.function, edge, span)? != checkpoint.roles {
                    return Err(lowering_error(LoweringErrorKind::MissingFact, span));
                }
                transports.push(Transport {
                    from: before,
                    to: checkpoint.roles,
                });
            }
            self.conditional_runtime_mut(checkpoint.statement, span)?
                .evidence
                .transports
                .extend(transports);
        }
        Ok(())
    }

    pub(in super::super::super) fn begin_conditional_map_cleanup(
        &mut self,
        point: UnitDropPoint,
    ) -> Result<(), LoweringError> {
        for (statement, state) in self.conditional_frames() {
            self.begin_map_frame_cleanup(statement, state, point)?;
        }
        Ok(())
    }

    pub(in super::super::super) fn validate_conditional_map_cleanup(
        &mut self,
        point: UnitDropPoint,
    ) -> Result<(), LoweringError> {
        for (statement, state) in self.conditional_frames() {
            self.validate_map_frame_cleanup(statement, state, point)?;
        }
        Ok(())
    }

    pub(in super::super) fn validate_map_nesting(
        &self,
        qualified: map_iteration::QualifiedMap,
        span: Span,
    ) -> Result<(), LoweringError> {
        let parent = self.loops.iter().rev().find_map(|c| {
            c.iteration
                .as_ref()
                .and_then(|i| matches!(i.state, IterationState::Map(_)).then_some(i.statement))
        });
        if qualified.nested.and_then(|n| n.parent) != parent {
            return Err(lowering_error(LoweringErrorKind::MissingFact, span));
        }
        Ok(())
    }

    pub(in super::super) fn validate_map_parent_temporaries(
        &self,
        span: Span,
    ) -> Result<(), LoweringError> {
        for (expression, owner) in &self.temporaries {
            let frames = self
                .conditional_frames()
                .into_iter()
                .filter(|(_, s)| s.carrier.identity == SourceIdentity::Temporary(*expression))
                .collect::<Vec<_>>();
            let [(_, state)] = frames.as_slice() else {
                return Err(lowering_error(LoweringErrorKind::UnsupportedNode, span));
            };
            if state.carrier.identity.current(self, span)? != EntityId::Value(*owner) {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            transport::from_anchor(
                self.function,
                EntityId::Value(*owner),
                state.carrier.anchor,
                span,
            )?;
        }
        Ok(())
    }
}
