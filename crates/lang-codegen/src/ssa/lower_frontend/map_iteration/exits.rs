//! The bounded Map context participates in real jump/return dispatch.
//! No nested/body CFG is admitted, so no new general BranchExit state is needed.
use super::*;

pub(in super::super) struct Context {
    pub(super) plan: MapIterationOwnershipPlan,
    pub(super) kind: IterationExitKind,
    pub(super) body: BlockId,
    pub(super) frame: Frame,
    pub(super) owner: Option<(source_owner::Proof, SymbolId, ValueId)>,
    pub(super) cursor: ValueId,
    pub(super) cursor_type: SsaTypeId,
    pub(super) next: Option<ValueId>,
    pub(super) cleaned: bool,
    pub(super) returned: Option<ValueId>,
    pub(super) cleanup_start: Option<usize>,
}

impl ExpressionLowerer<'_> {
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
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let mut state = self
            .loops
            .last_mut()
            .and_then(|context| context.map_loop.take())
            .ok_or_else(missing)?;
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
        self.emit_map_iteration_cleanup(exit, &mut state.frame, state.owner, span)?;
        if state
            .frame
            .components
            .iter()
            .any(|(_, loan)| loan.is_some())
            || (exit_shape::terminal(kind) != state.frame.finished)
            || (exit_shape::terminal(kind) != state.frame.source.is_none())
            || (exit_shape::terminal(kind)
                && state
                    .owner
                    .is_some_and(|(_, symbol, _)| self.bindings.contains_key(&symbol)))
        {
            return Err(missing());
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
        self.loops
            .last_mut()
            .and_then(|context| context.map_loop.as_mut())
            .ok_or_else(missing)?
            .next = Some(next);
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
            let LoweredValue::Value(value) = result else {
                return Err(error(LoweringErrorKind::MissingFact, span));
            };
            self.clean_map_iteration_body(IterationExitKind::Return(expression), span)?;
            self.loops
                .last_mut()
                .and_then(|context| context.map_loop.as_mut())
                .ok_or_else(|| error(LoweringErrorKind::MissingFact, span))?
                .returned = Some(value);
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
