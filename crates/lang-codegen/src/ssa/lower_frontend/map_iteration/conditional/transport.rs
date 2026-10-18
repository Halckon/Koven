//! Post-condition checkpoint; only path state is restored, never exit witnesses.
use super::*;

pub(in super::super::super) struct Checkpoint {
    control: ExpressionId,
    from: BlockId,
    frame: Frame,
    owner: Option<(source_owner::Proof, SourceIdentity, ValueId)>,
    roles: Roles,
    arguments: Vec<EntityId>,
}

pub(in super::super) fn roles(
    lowerer: &ExpressionLowerer<'_>,
    state: &Context,
    span: Span,
) -> Result<Roles, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    let conditional = state.conditional.as_ref().ok_or_else(missing)?;
    if state.cleaned || state.frame.finished || state.body != lowerer.block {
        return Err(missing());
    }
    let [(key_symbol, Some(key)), (value_symbol, Some(value))] = state.frame.components else {
        return Err(missing());
    };
    if lowerer.borrow_bindings.get(&key_symbol) != Some(&key)
        || lowerer.borrow_bindings.get(&value_symbol) != Some(&value)
        || state
            .owner
            .is_some_and(|(_, symbol, owner)| symbol.current_owner(lowerer) != Some(owner))
    {
        return Err(missing());
    }
    let Some(LoweredValue::Value(local)) = lowerer.bindings.get(&conditional.shape.local.symbol)
    else {
        return Err(missing());
    };
    Ok(Roles {
        source: state.frame.source.ok_or_else(missing)?,
        cursor: state.cursor,
        key,
        value,
        owner: state.owner.map(|(_, _, value)| value),
        local: *local,
    })
}

fn entities(roles: Roles) -> Vec<EntityId> {
    [
        Some(EntityId::Loan(roles.source)),
        Some(EntityId::Value(roles.cursor)),
        Some(EntityId::Loan(roles.key)),
        Some(EntityId::Loan(roles.value)),
        roles.owner.map(EntityId::Value),
        Some(EntityId::Value(roles.local)),
    ]
    .into_iter()
    .flatten()
    .collect()
}

pub(super) fn forwarded(
    before: Roles,
    arguments: &[EntityId],
    parameters: &[EntityId],
    span: Span,
) -> Result<Roles, LoweringError> {
    let missing = || error(LoweringErrorKind::MissingFact, span);
    if arguments.len() != parameters.len() {
        return Err(missing());
    }
    let map = |entity| {
        unique(
            arguments
                .iter()
                .enumerate()
                .filter_map(|(index, actual)| (*actual == entity).then_some(parameters[index])),
        )
        .ok_or_else(missing)
    };
    let loan = |source| match map(EntityId::Loan(source))? {
        EntityId::Loan(value) => Ok(value),
        _ => Err(missing()),
    };
    let value = |source| match map(EntityId::Value(source))? {
        EntityId::Value(value) => Ok(value),
        _ => Err(missing()),
    };
    Ok(Roles {
        source: loan(before.source)?,
        cursor: value(before.cursor)?,
        key: loan(before.key)?,
        value: loan(before.value)?,
        owner: before.owner.map(value).transpose()?,
        local: value(before.local)?,
    })
}

impl ExpressionLowerer<'_> {
    pub(in super::super::super) fn checkpoint_map_if(
        &self,
        control: ExpressionId,
        carried: &mut LinearBindings,
        span: Span,
    ) -> Result<Option<Checkpoint>, LoweringError> {
        let Some(state) = self
            .loops
            .last()
            .and_then(|context| context.map_loop.as_ref())
        else {
            return Ok(None);
        };
        let Some(conditional) = state.conditional.as_ref() else {
            return Ok(None);
        };
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let index = conditional
            .shape
            .arms
            .iter()
            .position(|arm| arm.control == control)
            .ok_or_else(missing)?;
        if conditional.transfers.len() != 2 * index
            || conditional.exits.len() != index
            || !self.temporaries.is_empty()
            || !self.pending_call_loans.is_empty()
            || !self.capture_loans.is_empty()
            || !self.non_null_bindings.is_empty()
            || !self.result_source_loans.is_empty()
        {
            return Err(missing());
        }
        let roles = roles(self, state, span)?;
        // Ordinary slots already transport owners and component loans. Only the
        // provider-private lease/cursor need new unnamed columns.
        for entity in [EntityId::Loan(roles.source), EntityId::Value(roles.cursor)] {
            if !carried.slots.iter().any(|slot| slot.source == entity) {
                let ty = self.function.entity(entity).ok_or_else(missing)?.ty;
                carried.slots.push(LinearBindingSlot::new(None, entity, ty));
            }
        }
        let arguments: Vec<_> = carried.slots.iter().map(|slot| slot.source).collect();
        for entity in entities(roles) {
            unique(arguments.iter().filter(|&&actual| actual == entity)).ok_or_else(missing)?;
        }
        Ok(Some(Checkpoint {
            control,
            from: self.block,
            frame: state.frame,
            owner: state.owner,
            roles,
            arguments,
        }))
    }

    pub(in super::super::super) fn enter_map_if_arm(
        &mut self,
        checkpoint: Option<&Checkpoint>,
        branch: usize,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(checkpoint) = checkpoint else {
            return Ok(());
        };
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let Some(TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        }) = carrier::terminator(self.function, checkpoint.from)
        else {
            return Err(missing());
        };
        let edge = match branch {
            0 => when_true,
            1 => when_false,
            _ => return Err(missing()),
        };
        if edge.target != self.block || edge.arguments != checkpoint.arguments {
            return Err(missing());
        }
        let parameters = self.map_iteration_parameters(self.block, span)?;
        let after = forwarded(checkpoint.roles, &edge.arguments, &parameters, span)?;
        let mut state = self
            .loops
            .last_mut()
            .and_then(|context| context.map_loop.take())
            .ok_or_else(missing)?;
        state.body = self.block;
        state.frame = checkpoint.frame;
        state.frame.source = Some(after.source);
        state.frame.components[0].1 = Some(after.key);
        state.frame.components[1].1 = Some(after.value);
        state.owner = checkpoint
            .owner
            .map(|(proof, symbol, _)| {
                after
                    .owner
                    .map(|owner| (proof, symbol, owner))
                    .ok_or_else(missing)
            })
            .transpose()?;
        state.cursor = after.cursor;
        state.kind = IterationExitKind::Fallthrough;
        state.cleaned = false;
        state.next = None;
        state.returned = None;
        state.cleanup_start = None;
        if roles(self, &state, span)? != after {
            return Err(missing());
        }
        let conditional = state.conditional.as_mut().ok_or_else(missing)?;
        conditional.transfers.push(Transfer {
            control: checkpoint.control,
            branch,
            from: checkpoint.from,
            to: self.block,
            before: checkpoint.roles,
            after,
        });
        self.loops.last_mut().ok_or_else(missing)?.map_loop = Some(state);
        Ok(())
    }

    pub(in super::super::super) fn finish_map_if(
        &self,
        checkpoint: Option<&Checkpoint>,
        result: LoweredValue,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(checkpoint) = checkpoint else {
            return Ok(());
        };
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let state = self
            .loops
            .last()
            .and_then(|context| context.map_loop.as_ref())
            .ok_or_else(missing)?;
        let last = state
            .conditional
            .as_ref()
            .and_then(|conditional| conditional.transfers.last())
            .ok_or_else(missing)?;
        if result != LoweredValue::Unit
            || last.control != checkpoint.control
            || last.branch != 1
            || last.to != self.block
            || roles(self, state, span)? != last.after
        {
            return Err(missing());
        }
        Ok(())
    }
}
