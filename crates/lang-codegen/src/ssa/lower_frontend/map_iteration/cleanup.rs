//! One public action list is emitted once, routing each provider action to its live frame.
use super::*;

pub(super) mod exit_shape {
    //! Bound the Map body to normal straight-line execution or one direct tail transfer.
    //! This describes capability only; the exit/action/owner facts still come from Phase 3.
    use super::*;
    use lang_frontend::type_checking::MapIterationDescriptor;

    pub(in super::super) fn body_exit(
        parsed: &ParsedFile,
        _names: &NameResolution,
        typed: &TypedFile,
        _descriptor: MapIterationDescriptor,
        body: StatementId,
        span: Span,
    ) -> Result<IterationExitKind, LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let unsupported = || error(LoweringErrorKind::UnsupportedNode, span);
        if qualify::straight_statement(parsed, typed, body)? {
            return Ok(IterationExitKind::Fallthrough);
        }
        let node = parsed.ast().statements().get(body).map_err(|_| missing())?;
        let Statement::Block { elements } = node.payload() else {
            return Err(unsupported());
        };
        let Some((last, prefix)) = elements.split_last() else {
            return Err(unsupported());
        };
        for statement in prefix {
            if !qualify::straight_statement(parsed, typed, *statement)? {
                return Err(unsupported());
            }
        }
        let node = parsed
            .ast()
            .statements()
            .get(*last)
            .map_err(|_| missing())?;
        let Statement::Expression { expression } = node.payload() else {
            return Err(unsupported());
        };
        let node = parsed
            .ast()
            .expressions()
            .get(*expression)
            .map_err(|_| missing())?;

        match node.payload() {
            Expression::Continue { .. } => Ok(IterationExitKind::Continue(*expression)),
            Expression::Break { .. } => Ok(IterationExitKind::Break(*expression)),
            // Operand evaluation and concrete return ABI belong to ordinary lower_return.
            // Complete FE has already checked the return contract and ownership delivery.
            Expression::Return { .. } => Ok(IterationExitKind::Return(*expression)),
            _ => Err(unsupported()),
        }
    }

    pub(in super::super) fn point(
        kind: IterationExitKind,
        statement: StatementId,
        body: StatementId,
    ) -> DropPoint {
        match kind {
            IterationExitKind::Fallthrough => DropPoint::AfterStatement(body),
            IterationExitKind::Continue(expression)
            | IterationExitKind::Break(expression)
            | IterationExitKind::Return(expression) => DropPoint::ControlTransfer(expression),
            IterationExitKind::Exhaustion => DropPoint::LoopExit(statement),
        }
    }

    pub(in super::super) fn terminal(kind: IterationExitKind) -> bool {
        matches!(
            kind,
            IterationExitKind::Break(_)
                | IterationExitKind::Return(_)
                | IterationExitKind::Exhaustion
        )
    }

    pub(in super::super) fn cleanup(
        owned: &OwnershipCheckedFile,
        plan: &MapIterationOwnershipPlan,
        body: StatementId,
        kind: IterationExitKind,
        span: Span,
    ) -> Result<(), LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let descriptor = plan.descriptor();
        let statement = descriptor.statement();
        if plan.exits().len() != 2 {
            return Err(missing());
        }
        for expected_kind in [kind, IterationExitKind::Exhaustion] {
            let exit = unique(
                plan.exits()
                    .iter()
                    .filter(|exit| exit.kind() == expected_kind),
            )
            .ok_or_else(missing)?;
            if exit.point() != point(expected_kind, statement, body)
                || owned.cleanup_conditions().get(exit.condition())
                    != Some(&CleanupCondition::Always)
                || owned
                    .cleanup_steps()
                    .iter()
                    .filter_map(|(point, action)| (*point == exit.point()).then_some(*action))
                    .collect::<Vec<_>>()
                    != exit.actions()
            {
                return Err(missing());
            }
            let map_actions = exit
                .actions()
                .iter()
                .filter(|action| !matches!(action, IterationCleanupAction::Drop(_)))
                .copied()
                .collect::<Vec<_>>();
            let mut expected = Vec::new();
            if expected_kind != IterationExitKind::Exhaustion {
                expected.extend([
                    IterationCleanupAction::EndBinding {
                        statement,
                        symbol: descriptor.value_binding(),
                    },
                    IterationCleanupAction::EndBinding {
                        statement,
                        symbol: descriptor.key_binding(),
                    },
                ]);
            }
            if terminal(expected_kind) {
                expected.extend([
                    IterationCleanupAction::FinishProvider(statement),
                    IterationCleanupAction::EndSource(statement),
                ]);
            }
            if map_actions != expected {
                return Err(missing());
            }
            // Local drops form one prefix. The Map sequence is contiguous, and
            // only the source owner may follow EndSource in this bounded shape.
            let start = exit
                .actions()
                .iter()
                .position(|action| expected.first() == Some(action))
                .ok_or_else(missing)?;
            if exit.actions()[..start]
                .iter()
                .any(|action| !matches!(action, IterationCleanupAction::Drop(_)))
                || exit.actions().get(start..start + expected.len()) != Some(expected.as_slice())
            {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let tail = &exit.actions()[start + expected.len()..];
            if !terminal(expected_kind) && !tail.is_empty() {
                return Err(missing());
            }
            if plan.source_access() == MapIterationSourceAccess::Shared && !tail.is_empty() {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
        }
        Ok(())
    }
}

impl ExpressionLowerer<'_> {
    pub(super) fn clean_map_flow_point(
        &mut self,
        kind: IterationExitKind,
        returned: Option<ValueId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let current = self
            .loops
            .last()
            .and_then(|context| context.map_loop.as_ref())
            .ok_or_else(missing)?;
        let exit = unique(
            current
                .plan
                .exits()
                .iter()
                .filter(|exit| exit.kind() == kind),
        )
        .ok_or_else(missing)?
        .clone();
        let point = exit.point();
        let actions = self
            .owned
            .cleanup_steps()
            .iter()
            .filter_map(|(actual, action)| (*actual == point).then_some(*action))
            .collect::<Vec<_>>();
        if actions != exit.actions() {
            return Err(missing());
        }
        let mut frames = Vec::new();
        let mut indices = Vec::new();
        for (index, context) in self.loops.iter().enumerate() {
            let Some(state) = context.map_loop.as_ref() else {
                // Ordinary loop cleanup remains with its own control frame.
                continue;
            };
            let included = actions.iter().any(|action| matches!(action,
                IterationCleanupAction::EndBinding { statement, .. } if *statement == state.frame.statement));
            if !included {
                continue;
            }
            if !matches!(kind, IterationExitKind::Return(_)) && index + 1 != self.loops.len() {
                return Err(missing());
            }
            let planned = unique(state.plan.exits().iter().filter(|exit| exit.kind() == kind))
                .ok_or_else(missing)?;
            if planned.point() != point || planned.actions() != actions || state.cleaned {
                return Err(missing());
            }
            let roles = conditional::transport::roles(self, state, span)?;
            frames.push(conditional::CleanupFrame {
                statement: state.frame.statement,
                roles,
                owner: state.owner,
            });
            indices.push(index);
        }
        if indices.last().copied() != Some(self.loops.len() - 1) {
            return Err(missing());
        }
        let start = self
            .function
            .block(self.block)
            .ok_or_else(missing)?
            .instructions
            .len();
        for action in &actions {
            let statement = match *action {
                IterationCleanupAction::EndBinding { statement, .. }
                | IterationCleanupAction::FinishProvider(statement)
                | IterationCleanupAction::EndSource(statement) => Some(statement),
                IterationCleanupAction::Drop(fact) => {
                    let mut owners = Vec::new();
                    for frame in &frames {
                        if let Some((proof, _, _)) = frame.owner
                            && proof.at_checked(point, self.owned)? == Some(fact)
                        {
                            owners.push(frame.statement);
                        }
                    }
                    match owners.as_slice() {
                        [] => None,
                        [statement] => Some(*statement),
                        _ => return Err(missing()),
                    }
                }
                _ => return Err(missing()),
            };
            if let Some(statement) = statement {
                let index = *indices
                    .iter()
                    .find(|&&index| {
                        self.loops[index]
                            .map_loop
                            .as_ref()
                            .is_some_and(|state| state.frame.statement == statement)
                    })
                    .ok_or_else(missing)?;
                let mut state = self.loops[index].map_loop.take().ok_or_else(missing)?;
                self.emit_map_cleanup_action(
                    action,
                    point,
                    &mut state.frame,
                    state.owner,
                    true,
                    span,
                )?;
                self.loops[index].map_loop = Some(state);
            } else if let IterationCleanupAction::Drop(fact) = action {
                self.emit_flow_map_local_drop(*fact, span)?;
            } else {
                return Err(missing());
            }
        }
        let end = self
            .function
            .block(self.block)
            .ok_or_else(missing)?
            .instructions
            .len();
        let mut dropped = Vec::new();
        for (proof, _, owner) in frames.iter().filter_map(|frame| frame.owner) {
            if proof.at_checked(point, self.owned)?.is_some() {
                dropped.push(owner);
            }
        }
        for index in indices {
            let current_owner = self.loops[index]
                .map_loop
                .as_ref()
                .ok_or_else(missing)?
                .owner
                .map(|(_, identity, _)| identity.current_owner(self));
            let state = self.loops[index].map_loop.as_mut().ok_or_else(missing)?;
            if state
                .frame
                .components
                .iter()
                .any(|(_, loan)| loan.is_some())
                || state.frame.finished != exit_shape::terminal(kind)
                || state.frame.source.is_none() != exit_shape::terminal(kind)
            {
                return Err(missing());
            }
            if let Some((_, _, owner)) = state.owner {
                let expected = (!dropped.contains(&owner)).then_some(owner);
                if current_owner != Some(expected) {
                    return Err(missing());
                }
            }
            let frame = frames
                .iter()
                .find(|frame| frame.statement == state.frame.statement)
                .ok_or_else(missing)?;
            state.kind = kind;
            state.cleaned = true;
            state.cleanup_start = Some(start);
            state.returned = returned;
            state
                .conditional
                .as_mut()
                .ok_or_else(missing)?
                .exits
                .push(conditional::Exit {
                    kind,
                    block: self.block,
                    roles: frame.roles,
                    frames: frames.clone(),
                    cleanup_start: start,
                    cleanup_end: end,
                    next: None,
                    returned,
                });
        }
        Ok(())
    }
}
