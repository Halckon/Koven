//! The Map context participates in ordinary CFG and real jump/return dispatch.
//! Path-local roles restore independently of cumulative exit evidence; ordinary
//! BranchExit retains the actual break/continue collections.
use super::*;

pub(in super::super) struct Context {
    pub(super) plan: MapIterationOwnershipPlan,
    pub(super) kind: IterationExitKind,
    pub(super) body: BlockId,
    pub(super) frame: Frame,
    pub(super) owner: Option<(source_owner::Proof, SourceIdentity, ValueId)>,
    pub(super) cursor: ValueId,
    pub(super) cursor_type: SsaTypeId,
    pub(super) next: Option<ValueId>,
    pub(super) cleaned: bool,
    pub(super) returned: Option<ValueId>,
    pub(super) cleanup_start: Option<usize>,
    pub(super) conditional: Option<conditional::State>,
    pub(super) nested: Option<nested::State>,
    pub(super) nested_events: Vec<nested_events::Event>,
    pub(super) resumed: Option<checked::Roles>,
}

// Ordinary CFG transport restores only these path fields. The qualification
// plan, source proof and cumulative conditional/nested evidence stay in place.
impl Context {
    pub(in super::super) fn path_snapshot(&self) -> Option<control::map_frames::Snapshot> {
        if self.nested.is_some() {
            return None;
        }
        Some(control::map_frames::Snapshot {
            statement: self.frame.statement,
            source: self.frame.source,
            components: self.frame.components,
            finished: self.frame.finished,
            owner: self
                .owner
                .map(|(_, source, owner)| (path_owner_root(source), owner)),
            body: self.body,
            cursor: self.cursor,
            cursor_type: self.cursor_type,
            kind: self.kind,
            cleaned: self.cleaned,
            next: self.next,
            returned: self.returned,
            cleanup_start: self.cleanup_start,
        })
    }

    pub(in super::super) fn restore_path(
        &mut self,
        path: control::map_frames::Snapshot,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.nested.is_some()
            || self.frame.statement != path.statement
            || self.plan.descriptor().statement() != path.statement
            || self.cursor_type != path.cursor_type
            || self.owner.map(|(_, source, _)| path_owner_root(source))
                != path.owner.map(|(root, _)| root)
            || self.frame.components.map(|(symbol, _)| symbol)
                != path.components.map(|(symbol, _)| symbol)
        {
            return Err(error(LoweringErrorKind::MissingFact, span));
        }
        if let (Some((_, _, owner)), Some((_, current))) = (&mut self.owner, path.owner) {
            *owner = current;
        }
        self.frame.source = path.source;
        self.frame.components = path.components;
        self.frame.finished = path.finished;
        self.body = path.body;
        self.cursor = path.cursor;
        self.kind = path.kind;
        self.cleaned = path.cleaned;
        self.next = path.next;
        self.returned = path.returned;
        self.cleanup_start = path.cleanup_start;
        Ok(())
    }
}

fn path_owner_root(source: SourceIdentity) -> control::map_frames::OwnerRoot {
    use control::map_frames::OwnerRoot;
    match source {
        SourceIdentity::Parameter(symbol) => OwnerRoot::Parameter(symbol),
        SourceIdentity::Local {
            symbol,
            initializer,
        } => OwnerRoot::Local {
            symbol,
            initializer,
        },
        SourceIdentity::Formed { symbol, producer } => OwnerRoot::Formed { symbol, producer },
        SourceIdentity::Temporary(expression) => OwnerRoot::Temporary(expression),
    }
}

impl ExpressionLowerer<'_> {
    /// A planned local Drop must consume its current ordinary CFG binding once.
    /// Unlike the temporary-tail constructor proof this accepts a block parameter
    /// whose producer was transported from an earlier dominating block.
    pub(super) fn emit_flow_map_local_drop(
        &mut self,
        fact: lang_frontend::ownership_checking::DropFact,
        span: Span,
    ) -> Result<(), LoweringError> {
        use lang_frontend::ownership_checking::DropTarget;
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let owner = match fact.target() {
            DropTarget::Named(symbol) => match self.bindings.get(&symbol).copied() {
                Some(LoweredValue::Value(value)) => value,
                _ => return Err(missing()),
            },
            DropTarget::Temporary(expression) => *self
                .temporaries
                .get(&expression.index())
                .ok_or_else(missing)?,
            _ => return Err(missing()),
        };
        let before = self
            .function
            .block(self.block)
            .ok_or_else(missing)?
            .instructions
            .len();
        self.emit_iteration_cleanup_action(&IterationCleanupAction::Drop(fact), span)?;
        let emitted = &self
            .function
            .block(self.block)
            .ok_or_else(missing)?
            .instructions[before..];
        let drops = emitted
            .iter()
            .filter_map(|id| self.function.instruction(*id))
            .filter(|instruction| instruction.operation == Operation::Drop { owner })
            .collect::<Vec<_>>();
        if !matches!(drops.as_slice(), [instruction] if instruction.origin == Origin::Source(fact.value_origin()))
        {
            return Err(missing());
        }
        Ok(())
    }
    pub(in super::super) fn lower_map_iteration_break(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        self.clean_map_iteration_body(IterationExitKind::Break(expression), span)?;
        let exit = self.branch_exit(LoweredValue::Unit);
        self.loops
            .last_mut()
            .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
            .breaks
            .push(exit);
        Ok(LoweredValue::Diverged)
    }

    pub(in super::super) fn lower_map_iteration_continue(
        &mut self,
        expression: ExpressionId,
        span: Span,
    ) -> Result<LoweredValue, LoweringError> {
        self.clean_map_iteration_body(IterationExitKind::Continue(expression), span)?;
        self.advance_map_iteration_body(span)?;
        Ok(LoweredValue::Diverged)
    }

    pub(in super::super) fn clean_map_iteration_body(
        &mut self,
        kind: IterationExitKind,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self.clean_nested_map_point(kind, None, span)? {
            return Ok(());
        }
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let mut state = self
            .loops
            .last_mut()
            .and_then(|context| context.map_loop.take())
            .ok_or_else(missing)?;
        let conditional_roles = if let Some(conditional) = &state.conditional {
            if conditional.shape.statement != state.plan.descriptor().statement()
                || !state.plan.exits().iter().any(|exit| exit.kind() == kind)
            {
                return Err(missing());
            }
            let roles = conditional::transport::roles(self, &state, span)?;
            state.kind = kind;
            Some(roles)
        } else {
            None
        };
        if state.kind != kind || self.block != state.body || state.cleaned {
            return Err(missing());
        }
        let exit = unique(state.plan.exits().iter().filter(|exit| exit.kind() == kind))
            .ok_or_else(missing)?;
        state.cleanup_start = Some(
            self.function
                .block(self.block)
                .ok_or_else(missing)?
                .instructions
                .len(),
        );
        self.emit_map_iteration_cleanup(
            exit,
            &mut state.frame,
            state.owner,
            state.conditional.is_some(),
            span,
        )?;
        let owner_mismatch = if let Some((proof, source, value)) = state.owner {
            source.current_owner(self)
                != proof
                    .at_checked(exit.point(), self.owned)?
                    .is_none()
                    .then_some(value)
        } else {
            false
        };
        if state
            .frame
            .components
            .iter()
            .any(|(_, loan)| loan.is_some())
            || (exit_shape::terminal(kind) != state.frame.finished)
            || (exit_shape::terminal(kind) != state.frame.source.is_none())
            || owner_mismatch
        {
            return Err(missing());
        }
        if let Some(roles) = conditional_roles {
            state
                .conditional
                .as_mut()
                .ok_or_else(missing)?
                .exits
                .push(conditional::Exit {
                    kind,
                    block: self.block,
                    roles,
                    cleanup_start: state.cleanup_start.ok_or_else(missing)?,
                    next: None,
                    returned: None,
                });
        }
        state.cleaned = true;
        self.loops.last_mut().ok_or_else(missing)?.map_loop = Some(state);
        Ok(())
    }

    pub(in super::super) fn advance_map_iteration_body(
        &mut self,
        span: Span,
    ) -> Result<(), LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let (source, cursor, cursor_type, header, carried) = {
            let context = self.loops.last().ok_or_else(missing)?;
            let state = context.map_loop.as_ref().ok_or_else(missing)?;
            if !state.cleaned
                || state.next.is_some()
                || state.frame.finished
                || state
                    .frame
                    .components
                    .iter()
                    .any(|(_, loan)| loan.is_some())
                || state.body != self.block
                || !matches!(
                    state.kind,
                    IterationExitKind::Fallthrough | IterationExitKind::Continue(_)
                )
            {
                return Err(missing());
            }
            (
                state.frame.source.ok_or_else(missing)?,
                state.cursor,
                state.cursor_type,
                context.header,
                context.carried.clone(),
            )
        };
        let next = self.map_iteration_value(
            Operation::MapIterationNext { source, cursor },
            cursor_type,
            span,
        )?;
        let mut arguments = self.map_iteration_carried_arguments(&carried, span)?;
        arguments.extend([EntityId::Loan(source), EntityId::Value(next)]);
        self.function
            .set_terminator(
                self.block,
                TerminatorKind::Branch(Edge {
                    target: header,
                    arguments,
                }),
                Origin::Source(span),
            )
            .map_err(|_| error(LoweringErrorKind::InvalidModel, span))?;
        let state = self
            .loops
            .last_mut()
            .and_then(|context| context.map_loop.as_mut())
            .ok_or_else(missing)?;
        state.next = Some(next);
        if let Some(conditional) = &mut state.conditional {
            conditional.exits.last_mut().ok_or_else(missing)?.next = Some(next);
        }
        Ok(())
    }

    /// Called only after the actual return operand has been evaluated/delivered.
    /// Keep the existing Sequential/Numeric dispatch unchanged when no Map is active.
    pub(in super::super) fn emit_active_iteration_return_cleanup(
        &mut self,
        expression: ExpressionId,
        result: LoweredValue,
        span: Span,
    ) -> Result<(), LoweringError> {
        if self
            .loops
            .last()
            .is_some_and(|context| context.map_loop.is_some())
        {
            let value = match result {
                LoweredValue::Value(value) => Some(value),
                LoweredValue::Unit => None,
                LoweredValue::Diverged => return Err(error(LoweringErrorKind::MissingFact, span)),
            };
            if self.clean_nested_map_point(IterationExitKind::Return(expression), value, span)? {
                return Ok(());
            }
            self.clean_map_iteration_body(IterationExitKind::Return(expression), span)?;
            let state = self
                .loops
                .last_mut()
                .and_then(|context| context.map_loop.as_mut())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?;
            state.returned = value;
            if let Some(conditional) = &mut state.conditional {
                conditional
                    .exits
                    .last_mut()
                    .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                    .returned = value;
            }
            return Ok(());
        }
        let exit = self.loops.iter().rev().find_map(|context| {
            context.iteration_plan().and_then(|plan| {
                plan.exits()
                    .iter()
                    .find(|exit| exit.kind() == IterationExitKind::Return(expression))
                    .cloned()
            })
        });
        if let Some(exit) = exit {
            self.emit_iteration_exit_plan(&exit, span)
        } else {
            self.emit_control_transfer_cleanup(expression)
        }
    }
}
