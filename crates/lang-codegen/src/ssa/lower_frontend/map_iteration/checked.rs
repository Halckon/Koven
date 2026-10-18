//! Checked success uses ordinary Map transport once, then verifies its exact before/after roles.
use super::*;
use control::map_frames::Snapshot;

#[derive(Clone, Copy)]
pub(super) struct Roles {
    pub source: LoanId,
    pub cursor: ValueId,
    pub key: LoanId,
    pub value: LoanId,
}

fn same_path(left: Snapshot, right: Snapshot) -> bool {
    left.statement == right.statement
        && left.source == right.source
        && left.components == right.components
        && left.finished == right.finished
        && left.owner == right.owner
        && left.body == right.body
        && left.cursor == right.cursor
        && left.cursor_type == right.cursor_type
        && left.kind == right.kind
        && left.cleaned == right.cleaned
        && left.next == right.next
        && left.returned == right.returned
        && left.cleanup_start == right.cleanup_start
}

impl ExpressionLowerer<'_> {
    /// The ordinary carrier already owns source/cursor columns. This checkpoint
    /// validates the real before-snapshot and never adds duplicate private slots.
    pub(in super::super) fn carry_checked_map(
        &self,
        carried: &LinearBindings,
        span: Span,
    ) -> Result<(), LoweringError> {
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let mut active = 0;
        for context in &self.loops {
            let Some(state) = &context.map_loop else {
                continue;
            };
            if state.nested.is_some() {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let actual = state.path_snapshot().ok_or_else(missing)?;
            let before = *carried
                .map_paths
                .get(&actual.statement.index())
                .ok_or_else(missing)?;
            if !same_path(actual, before)
                || before.body != self.block
                || before.statement != state.plan.descriptor().statement()
                || before.cleaned
                || before.finished
                || before.next.is_some()
                || before.returned.is_some()
                || before.cleanup_start.is_some()
            {
                return Err(missing());
            }
            let [(key_symbol, Some(key)), (value_symbol, Some(value))] = before.components else {
                return Err(missing());
            };
            if self.borrow_bindings.get(&key_symbol) != Some(&key)
                || self.borrow_bindings.get(&value_symbol) != Some(&value)
                || state
                    .owner
                    .is_some_and(|(_, identity, owner)| identity.current_owner(self) != Some(owner))
            {
                return Err(missing());
            }
            let entities = [
                Some(EntityId::Loan(before.source.ok_or_else(missing)?)),
                Some(EntityId::Value(before.cursor)),
                Some(EntityId::Loan(key)),
                Some(EntityId::Loan(value)),
                before.owner.map(|(_, owner)| EntityId::Value(owner)),
            ];
            for entity in entities.into_iter().flatten() {
                let slot = unique(carried.slots.iter().filter(|slot| slot.source == entity))
                    .ok_or_else(missing)?;
                if self.function.entity(entity).map(|data| data.ty) != Some(slot.ty) {
                    return Err(missing());
                }
            }
            active += 1;
        }
        if carried.map_paths.len() != active {
            return Err(missing());
        }
        Ok(())
    }

    /// `rebind_linear_bindings` has restored this success path. Verify the real
    /// checked edge against the saved input columns; only carrier evidence changes.
    pub(in super::super) fn resume_checked_map(
        &mut self,
        from: BlockId,
        carried: &LinearBindings,
        span: Span,
    ) -> Result<(), LoweringError> {
        if carried.map_paths.is_empty()
            && self.loops.iter().all(|context| context.map_loop.is_none())
        {
            return Ok(());
        }
        let missing = || error(LoweringErrorKind::MissingFact, span);
        let success = self.function.block(self.block).ok_or_else(missing)?;
        let parameters = &success.parameters;
        let arguments = carried
            .slots
            .iter()
            .map(|slot| slot.source)
            .collect::<Vec<_>>();
        let Some(TerminatorKind::Conditional {
            when_true,
            when_false,
            ..
        }) = carrier::terminator(self.function, from)
        else {
            return Err(missing());
        };
        if when_false.target != self.block
            || when_false.arguments != arguments
            || when_true.arguments != arguments
            || when_true.target == self.block
            || !matches!(
                carrier::terminator(self.function, when_true.target),
                Some(TerminatorKind::Abort)
            )
            || parameters.len() != carried.slots.len()
            || success.origin != Origin::Source(span)
            || !self
                .function
                .block(from)
                .and_then(|block| block.terminator.as_ref())
                .is_some_and(|terminator| terminator.origin == Origin::Source(span))
        {
            return Err(missing());
        }
        // Success has one actual incoming edge. It cannot inherit a role from
        // a different predecessor that merely has a compatible represented type.
        let mut incoming = 0;
        for block in &self.function.blocks {
            let Some(terminator) = &block.terminator else {
                continue;
            };
            for _ in terminator
                .kind
                .targets()
                .into_iter()
                .filter(|target| *target == self.block)
            {
                if block.id != from {
                    return Err(missing());
                }
                incoming += 1;
            }
        }
        if incoming != 1 {
            return Err(missing());
        }
        for (index, (slot, &parameter)) in carried.slots.iter().zip(parameters).enumerate() {
            let data = self.function.entity(parameter).ok_or_else(missing)?;
            if self.function.entity(slot.source).map(|source| source.ty) != Some(slot.ty)
                || data.ty != slot.ty
                || data.definition
                    != (Definition::BlockParameter {
                        block: self.block,
                        index,
                    })
            {
                return Err(missing());
            }
        }
        let mapped = |entity| {
            let index = unique(
                carried
                    .slots
                    .iter()
                    .enumerate()
                    .filter_map(|(index, slot)| (slot.source == entity).then_some(index)),
            )
            .ok_or_else(missing)?;
            parameters.get(index).copied().ok_or_else(missing)
        };
        let mapped_loan = |loan| match mapped(EntityId::Loan(loan))? {
            EntityId::Loan(current) => Ok(current),
            _ => Err(missing()),
        };
        let mapped_value = |value| match mapped(EntityId::Value(value))? {
            EntityId::Value(current) => Ok(current),
            _ => Err(missing()),
        };
        let mut resumed = Vec::new();
        for (index, context) in self.loops.iter().enumerate() {
            let Some(state) = &context.map_loop else {
                continue;
            };
            if state.nested.is_some() {
                return Err(error(LoweringErrorKind::UnsupportedNode, span));
            }
            let actual = state.path_snapshot().ok_or_else(missing)?;
            let before = *carried
                .map_paths
                .get(&actual.statement.index())
                .ok_or_else(missing)?;
            if before.body != from || before.statement != state.plan.descriptor().statement() {
                return Err(missing());
            }
            let mut expected = before;
            expected.body = self.block;
            expected.source = Some(mapped_loan(before.source.ok_or_else(missing)?)?);
            expected.cursor = mapped_value(before.cursor)?;
            for (_, loan) in &mut expected.components {
                *loan = Some(mapped_loan(loan.ok_or_else(missing)?)?);
            }
            expected.owner = before
                .owner
                .map(|(root, owner)| mapped_value(owner).map(|current| (root, current)))
                .transpose()?;
            if !same_path(actual, expected)
                || state
                    .owner
                    .is_some_and(|(_, identity, owner)| identity.current_owner(self) != Some(owner))
            {
                return Err(missing());
            }
            let [(key_symbol, Some(key)), (value_symbol, Some(value))] = actual.components else {
                return Err(missing());
            };
            if self.borrow_bindings.get(&key_symbol) != Some(&key)
                || self.borrow_bindings.get(&value_symbol) != Some(&value)
            {
                return Err(missing());
            }
            resumed.push((
                index,
                Roles {
                    source: actual.source.ok_or_else(missing)?,
                    cursor: actual.cursor,
                    key,
                    value,
                },
            ));
        }
        if resumed.len() != carried.map_paths.len() {
            return Err(missing());
        }
        for (index, roles) in resumed {
            self.loops[index]
                .map_loop
                .as_mut()
                .ok_or_else(missing)?
                .resumed = Some(roles);
        }
        Ok(())
    }
}
