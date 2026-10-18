//! Dynamic exact-point cleanup, backedge and Return evidence from the ordinary CFG.
use super::*;
impl UnitExpressionLowerer<'_> {
    pub(in super::super) fn validate_conditional_map_cleanup(
        &mut self,
        point: UnitDropPoint,
    ) -> Result<(), LoweringError> {
        let Some((statement, state)) = self.conditional_map() else {
            return Ok(());
        };
        let span = self.statement_span(statement.statement())?;
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let Some(index) = state.conditional.ok_or_else(bad)?.cleanup else {
            return Ok(());
        };
        let runtime = self
            .loops
            .iter()
            .rev()
            .filter_map(|c| c.iteration.as_ref())
            .find_map(|i| match &i.state {
                IterationState::Map(s) if i.statement == statement => Some(s.as_ref()),
                _ => None,
            })
            .ok_or_else(bad)?;
        let witness = runtime.evidence.starts.get(index).ok_or_else(bad)?;
        if witness.point != point {
            return Ok(());
        }
        if self.block != witness.roles.block
            || runtime.evidence.cleanups.iter().any(|(i, _)| *i == index)
        {
            return Err(bad());
        }
        let block = self.function.block(self.block).ok_or_else(bad)?;
        let actual = block
            .instructions
            .get(witness.position..)
            .ok_or_else(bad)?
            .iter()
            .map(|id| self.function.instruction(*id).ok_or_else(bad))
            .collect::<Result<Vec<_>, _>>()?;
        let mut previous = None;
        for (operation, origin) in &witness.operations {
            let positions = actual
                .iter()
                .enumerate()
                .filter_map(|(p, i)| {
                    (i.operation == *operation && origin.as_ref().is_none_or(|o| &i.origin == o))
                        .then_some(p)
                })
                .collect::<Vec<_>>();
            let [position] = positions.as_slice() else {
                return Err(bad());
            };
            if previous.is_some_and(|p| p >= *position) {
                return Err(bad());
            }
            previous = Some(*position);
        }
        // Other public actions are emitted by the same general action dispatcher;
        // their call/receiver/capture protocol is not duplicated in this provider.
        if !witness.delegated && actual.len() != witness.operations.len() {
            return Err(bad());
        }
        let end = block.instructions.len();
        self.conditional_runtime_mut(statement, span)?
            .evidence
            .cleanups
            .push((index, end));
        Ok(())
    }
    pub(in super::super) fn record_conditional_return_delivery(
        &mut self,
        expression: UnitExpressionId,
        result: LoweredValue,
        position: usize,
        span: Span,
    ) -> Result<(), LoweringError> {
        let point = UnitDropPoint::ControlTransfer(expression);
        let targets = self
            .loops
            .iter()
            .filter_map(|c| c.iteration.as_ref())
            .filter_map(|i| match &i.state {
                IterationState::Map(s) if s.conditional.is_some() => Some(i.statement),
                _ => None,
            })
            .collect::<Vec<_>>();
        for statement in targets {
            let plan = self
                .owned
                .map_iteration(statement)
                .ok_or_else(|| lowering_error(LoweringErrorKind::MissingFact, span))?;
            let exits = plan
                .exits()
                .iter()
                .filter(|e| e.point() == point)
                .collect::<Vec<_>>();
            let exit = match exits.as_slice() {
                [] => continue,
                [e] => *e,
                _ => return Err(lowering_error(LoweringErrorKind::MissingFact, span)),
            };
            if exit.kind() != UnitIterationExitKind::Return(expression) {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            let block = self.block;
            let runtime = self.conditional_runtime_mut(statement, span)?;
            if runtime.evidence.returns.iter().any(|r| r.point == point) {
                return Err(lowering_error(LoweringErrorKind::MissingFact, span));
            }
            runtime.evidence.returns.push(ReturnDelivery {
                point,
                result,
                block,
                position,
            });
        }
        Ok(())
    }
    fn conditional_return(
        &self,
        witness: &CleanupStart,
        evidence: &Evidence,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let returned = evidence
            .returns
            .iter()
            .filter(|r| r.point == witness.point)
            .collect::<Vec<_>>();
        let [returned] = returned.as_slice() else {
            return Err(bad());
        };
        let block = self.function.block(witness.roles.block).ok_or_else(bad)?;
        let Some(TerminatorKind::Return { values }) = block.terminator.as_ref().map(|t| &t.kind)
        else {
            return Err(bad());
        };
        if returned.block != block.id
            || returned.position > witness.position
            || *values != self.return_values(returned.result, span)?
        {
            return Err(bad());
        }
        for value in values {
            let entity = EntityId::Value(*value);
            let data = self.function.entity(entity).ok_or_else(bad)?;
            if let crate::ssa::model::Definition::InstructionResult { instruction, index } =
                data.definition
            {
                let producer = self.function.instruction(instruction).ok_or_else(bad)?;
                if producer.results.get(index) != Some(&entity) {
                    return Err(bad());
                }
                if producer.block == block.id
                    && block
                        .instructions
                        .iter()
                        .position(|id| *id == instruction)
                        .is_none_or(|p| p >= returned.position)
                {
                    return Err(bad());
                }
            }
        }
        if block.instructions.iter().any(|id| {
            self.function
                .instruction(*id)
                .is_some_and(|i| matches!(i.operation, Operation::MapIterationNext { .. }))
        }) {
            return Err(bad());
        }
        Ok(())
    }
    fn conditional_backedge(
        &self,
        state: MapState,
        index: usize,
        evidence: &Evidence,
        jump: &BranchExit,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let witness = evidence.starts.get(index).ok_or_else(bad)?;
        let nexts = evidence
            .nexts
            .iter()
            .filter(|(i, _)| *i == index)
            .collect::<Vec<_>>();
        let [(_, next)] = nexts.as_slice() else {
            return Err(bad());
        };
        let ends = evidence
            .cleanups
            .iter()
            .filter(|(i, _)| *i == index)
            .collect::<Vec<_>>();
        let [(_, end)] = ends.as_slice() else {
            return Err(bad());
        };
        let block = self.function.block(witness.roles.block).ok_or_else(bad)?;
        let Some(TerminatorKind::Branch(edge)) = block.terminator.as_ref().map(|t| &t.kind) else {
            return Err(bad());
        };
        let [seed, source, cursor] = state.carrier.header_entities(self.function, span)?;
        if edge.target != state.carrier.header
            || jump.block != block.id
            || transport::along(self.function, edge, witness.roles.seed, span)? != seed
            || transport::along(
                self.function,
                edge,
                EntityId::Loan(witness.roles.source),
                span,
            )? != source
            || transport::along(self.function, edge, EntityId::Value(*next), span)? != cursor
            || jump.pending_operands.get(state.source_slot)
                != Some(&EntityId::Loan(witness.roles.source))
            || jump.pending_operands.get(state.cursor_slot) != Some(&EntityId::Value(*next))
            || [witness.roles.key, witness.roles.value]
                .into_iter()
                .flatten()
                .any(|l| edge.arguments.contains(&EntityId::Loan(l)))
        {
            return Err(bad());
        }
        let advance = carrier::instruction(self.function, EntityId::Value(*next), span)?;
        if advance.block != block.id
            || block.instructions.get(*end) != Some(&advance.id)
            || *end + 1 != block.instructions.len()
            || advance.operation
                != (Operation::MapIterationNext {
                    source: witness.roles.source,
                    cursor: witness.roles.cursor,
                })
        {
            return Err(bad());
        }
        Ok(())
    }
    pub(in super::super) fn validate_conditional_map_body(
        &self,
        context: &LoopContext,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_conditional_evidence(context, false, span)
    }
    pub(in super::super) fn validate_conditional_map_complete(
        &self,
        context: &LoopContext,
        span: Span,
    ) -> Result<(), LoweringError> {
        self.validate_conditional_evidence(context, true, span)
    }
    fn validate_conditional_evidence(
        &self,
        context: &LoopContext,
        complete: bool,
        span: Span,
    ) -> Result<(), LoweringError> {
        let bad = || lowering_error(LoweringErrorKind::MissingFact, span);
        let iteration = context.iteration.as_ref().ok_or_else(bad)?;
        let IterationState::Map(runtime) = &iteration.state else {
            return Err(bad());
        };
        let state = runtime.path;
        let Some(branch) = state.conditional else {
            return Ok(());
        };
        let evidence = &runtime.evidence;
        let plan = self
            .owned
            .map_iteration(iteration.statement)
            .ok_or_else(bad)?;
        let expected = plan
            .exits()
            .iter()
            .filter(|e| complete || e.kind() != UnitIterationExitKind::Exhaustion)
            .collect::<Vec<_>>();
        if evidence.starts.len() != expected.len() || evidence.cleanups.len() != expected.len() {
            return Err(bad());
        }
        for exit in expected {
            let witnesses = evidence
                .starts
                .iter()
                .enumerate()
                .filter(|(_, w)| w.point == exit.point() && w.kind == exit.kind())
                .collect::<Vec<_>>();
            let [(index, witness)] = witnesses.as_slice() else {
                return Err(bad());
            };
            if evidence.cleanups.iter().filter(|(i, _)| i == index).count() != 1 {
                return Err(bad());
            }
            match exit.kind() {
                UnitIterationExitKind::Fallthrough | UnitIterationExitKind::Continue(_) => {
                    let jumps = context
                        .continues
                        .iter()
                        .filter(|j| j.state.block == witness.roles.block)
                        .collect::<Vec<_>>();
                    let [jump] = jumps.as_slice() else {
                        return Err(bad());
                    };
                    self.conditional_backedge(state, *index, evidence, &jump.state, span)?;
                }
                UnitIterationExitKind::Break(_) => {
                    if context
                        .breaks
                        .iter()
                        .filter(|j| j.state.block == witness.roles.block)
                        .count()
                        != 1
                        || evidence.nexts.iter().any(|(i, _)| i == index)
                    {
                        return Err(bad());
                    }
                }
                UnitIterationExitKind::Return(_) => {
                    if evidence.nexts.iter().any(|(i, _)| i == index)
                        || context
                            .breaks
                            .iter()
                            .chain(&context.continues)
                            .any(|j| j.state.block == witness.roles.block)
                    {
                        return Err(bad());
                    }
                    self.conditional_return(witness, evidence, span)?;
                }
                UnitIterationExitKind::Exhaustion => {
                    if witness.roles.block != state.carrier.exhaustion
                        || witness.roles.key.is_some()
                        || witness.roles.value.is_some()
                        || evidence.nexts.iter().any(|(i, _)| i == index)
                    {
                        return Err(bad());
                    }
                }
            }
        }
        let continue_count = evidence
            .starts
            .iter()
            .filter(|w| {
                matches!(
                    w.kind,
                    UnitIterationExitKind::Fallthrough | UnitIterationExitKind::Continue(_)
                )
            })
            .count();
        let break_count = evidence
            .starts
            .iter()
            .filter(|w| matches!(w.kind, UnitIterationExitKind::Break(_)))
            .count();
        if context.continues.len() != continue_count
            || context.breaks.len() != break_count
            || evidence.nexts.len() != continue_count
        {
            return Err(bad());
        }
        for t in &evidence.transports {
            let edges = transport::incoming(self.function, t.to.block)
                .into_iter()
                .filter(|(b, _)| *b == t.from.block)
                .collect::<Vec<_>>();
            let [(_, edge)] = edges.as_slice() else {
                return Err(bad());
            };
            if t.from.transported(self.function, edge, span)? != t.to {
                return Err(bad());
            }
        }
        if branch.shape.statement != iteration.statement
            || branch.shape.source != state.carrier.identity
        {
            return Err(bad());
        }
        Ok(())
    }
}
