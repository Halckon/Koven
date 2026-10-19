//! One public action list is emitted once, routing each provider action to its live frame.
use super::*;

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
