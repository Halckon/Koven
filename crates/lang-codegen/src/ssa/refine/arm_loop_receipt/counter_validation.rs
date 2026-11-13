//! Final SSA evidence for the bounded, same-source two-arm counter loop.
use super::counter::AliasUse;
use super::*;
use crate::ssa::model::{
    AbortReason, CheckedArithmeticOperator, ComparisonOperator, Operation, Origin, PlaceAccess,
    ScalarConstant, TerminatorKind,
};

impl Emitted {
    fn counter_end(&self, b: BlockRef, slot: Slot, s: Span) -> Result<EntityId, LoweringError> {
        self.ends
            .get(&self.block(b, s)?)
            .and_then(|m| m.get(&slot))
            .copied()
            .ok_or_else(|| invalid(s))
    }
    fn counter_alias(&self, usage: AliasUse, s: Span) -> Result<EntityId, LoweringError> {
        let (input, output, entity) = self
            .aliases
            .get(&(usage.block, usage.index))
            .copied()
            .ok_or_else(|| invalid(s))?;
        if input != usage.input || output != usage.output || !matches!(entity, EntityId::Value(_)) {
            return Err(invalid(s));
        }
        if self.counter_end(usage.block, output, s)? != entity {
            return Err(invalid(s));
        }
        Ok(entity)
    }
    fn counter_constant(
        &self,
        f: &Function,
        id: InstructionId,
        b: BlockRef,
        expected: (i128, Span),
        s: Span,
    ) -> Result<EntityId, LoweringError> {
        let (value, origin) = expected;
        let inst = f.instruction(id).ok_or_else(|| invalid(s))?;
        if inst.block != self.block(b, s)?
            || inst.origin != Origin::Source(origin)
            || inst.operation != Operation::Constant(ScalarConstant::Integer(value))
            || inst.results.len() != 1
            || !matches!(inst.results[0], EntityId::Value(_))
        {
            return Err(invalid(s));
        }
        Ok(inst.results[0])
    }
    pub(super) fn validate_counter(
        &self,
        parameters: &[Slot],
        f: &Function,
        p: &Plan,
        s: Span,
    ) -> Result<(), LoweringError> {
        let counter = p.counter.as_ref().ok_or_else(|| invalid(s))?;
        counter.ready(s)?;
        let init = counter.init.ok_or_else(|| invalid(s))?;
        let update = counter.update.ok_or_else(|| invalid(s))?;
        let ([op_block, success, abort], [result_slot, failed_slot, left_slot, one_slot]) =
            counter.checked.ok_or_else(|| invalid(s))?;
        let [entry, header, split, finished, exit] = p.blocks.ok_or_else(|| invalid(s))?;
        let [actual_split, arm_a, arm_b] = p.split.ok_or_else(|| invalid(s))?;
        let join = p.join.ok_or_else(|| invalid(s))?;
        if init.block != entry
            || op_block != join
            || split != actual_split
            || init.output != left_slot
            || update.block != success
            || p.used[0].map(|x| x.0) != Some(header)
        {
            return Err(invalid(s));
        }
        // All selected edges retain positional slot/entity transport, including the checked edge.
        for (from, to) in [
            (entry, vec![header]),
            (header, vec![split, finished]),
            (split, vec![arm_a, arm_b]),
            (arm_a, vec![join]),
            (arm_b, vec![join]),
            (join, vec![abort, success]),
            (success, vec![header]),
        ] {
            self.links(f, from, &to, s)?;
        }
        for (to, from) in [
            (header, vec![entry, success]),
            (split, vec![header]),
            (arm_a, vec![split]),
            (arm_b, vec![split]),
            (join, vec![arm_a, arm_b]),
            (success, vec![join]),
            (abort, vec![join]),
            (finished, vec![header]),
        ] {
            self.incoming(f, to, &from, s)?;
        }
        if finished != exit {
            self.links(f, finished, &[exit], s)?;
            self.incoming(f, exit, &[finished], s)?;
        }
        let rounds_slot = *parameters.get(p.roles[1]).ok_or_else(|| invalid(s))?;
        let rounds = self.entity(entry, rounds_slot, s)?;
        let initial = self.counter_alias(init, s)?;
        let initial_ids = self.ids(f, entry, s)?;
        match rounds {
            EntityId::Loan(loan) => {
                if initial_ids.len() != 1 {
                    return Err(invalid(s));
                }
                let inst = f.instruction(initial_ids[0]).ok_or_else(|| invalid(s))?;
                if inst.operation
                    != (Operation::Read {
                        source: PlaceAccess::Loan(loan),
                    })
                    || inst.results != [initial]
                    || inst.block != self.block(entry, s)?
                    || inst.origin != Origin::Source(counter.spec.initializer_span)
                {
                    return Err(invalid(s));
                }
            }
            EntityId::Value(_) => {
                if !initial_ids.is_empty() || rounds != initial || init.input != rounds_slot {
                    return Err(invalid(s));
                }
            }
            _ => return Err(invalid(s)),
        }
        let hids = self.ids(f, header, s)?;
        if hids.len() != 2 {
            return Err(invalid(s));
        }
        let zero = self.counter_constant(f, hids[0], header, (0, counter.spec.zero_span), s)?;
        let current = self.entity(header, init.output, s)?;
        let compare = f.instruction(hids[1]).ok_or_else(|| invalid(s))?;
        let (EntityId::Value(left), EntityId::Value(right)) = (current, zero) else {
            return Err(invalid(s));
        };
        let (_, condition_slot) = p.used[0].ok_or_else(|| invalid(s))?;
        let condition = self.counter_end(header, condition_slot, s)?;
        if compare.operation
            != (Operation::Compare {
                operator: ComparisonOperator::GreaterThan,
                left,
                right,
            })
            || compare.results != [condition]
            || compare.origin != Origin::Source(counter.spec.condition_span)
            || compare.block != self.block(header, s)?
        {
            return Err(invalid(s));
        }
        let header_term = f
            .block(self.block(header, s)?)
            .and_then(|b| b.terminator.as_ref())
            .ok_or_else(|| invalid(s))?;
        if !matches!(header_term.kind, TerminatorKind::Conditional{condition:c,..} if EntityId::Value(c)==condition)
        {
            return Err(invalid(s));
        }
        self.condition(parameters, f, p, 1, s)?;
        let uids = self.ids(f, join, s)?;
        if uids.len() != 2 {
            return Err(invalid(s));
        }
        let one = self.counter_constant(f, uids[0], join, (1, counter.spec.one_span), s)?;
        if self.counter_end(join, one_slot, s)? != one {
            return Err(invalid(s));
        }
        let previous = self.entity(join, init.output, s)?;
        let (EntityId::Value(left), EntityId::Value(right)) = (previous, one) else {
            return Err(invalid(s));
        };
        let result = self.counter_end(join, result_slot, s)?;
        let failed = self.counter_end(join, failed_slot, s)?;
        let arithmetic = f.instruction(uids[1]).ok_or_else(|| invalid(s))?;
        if arithmetic.operation
            != (Operation::CheckedArithmetic {
                operator: CheckedArithmeticOperator::Subtract,
                left,
                right,
            })
            || arithmetic.results != [result, failed]
            || result == failed
            || !matches!(result, EntityId::Value(_))
            || !matches!(failed, EntityId::Value(_))
            || arithmetic.block != self.block(join, s)?
            || arithmetic.origin != Origin::Source(counter.spec.update_span)
        {
            return Err(invalid(s));
        }
        let term = f
            .block(self.block(join, s)?)
            .and_then(|b| b.terminator.as_ref())
            .ok_or_else(|| invalid(s))?;
        if !matches!(term.kind,TerminatorKind::Conditional{condition,..} if EntityId::Value(condition)==failed)
        {
            return Err(invalid(s));
        }
        let updated = self.counter_alias(update, s)?;
        if updated != self.entity(success, result_slot, s)?
            || !self.ids(f, success, s)?.is_empty()
            || !self.ids(f, abort, s)?.is_empty()
            || !self.ids(f, finished, s)?.is_empty()
        {
            return Err(invalid(s));
        }
        let abort_term = f
            .block(self.block(abort, s)?)
            .and_then(|b| b.terminator.as_ref())
            .ok_or_else(|| invalid(s))?;
        let divisor = if self
            .parameters
            .get(&abort)
            .is_some_and(|v| v.contains(&one_slot))
        {
            self.entity(abort, one_slot, s)?
        } else {
            one
        };
        let EntityId::Value(divisor) = divisor else {
            return Err(invalid(s));
        };
        if abort_term.kind != TerminatorKind::Abort
            || abort_term.abort
                != Some(AbortReason::CheckedArithmetic {
                    operator: CheckedArithmeticOperator::Subtract,
                    divisor,
                })
            || abort_term.origin != Origin::Source(counter.spec.update_span)
        {
            return Err(invalid(s));
        }
        // No unrecorded alias may read or overwrite the local counter.
        for (position, (input, output, _)) in &self.aliases {
            if (*input == init.output || *output == init.output)
                && *position != (init.block, init.index)
                && *position != (update.block, update.index)
            {
                return Err(invalid(s));
            }
        }
        // The old counter is transported only through the loop's header, choice and arms.
        // Checked-result/one slots are permitted on both checked siblings by emitter liveness.
        let live_counter_blocks = [header, split, arm_a, arm_b, join];
        let mut counter_entities = vec![initial, result, updated];
        for (b, slots) in &self.parameters {
            for slot in slots {
                if *slot == init.output {
                    if !live_counter_blocks.contains(b) {
                        return Err(invalid(s));
                    }
                    counter_entities.push(self.entity(*b, *slot, s)?);
                } else if !parameters.contains(slot) {
                    if ![success, abort].contains(b) || ![result_slot, one_slot].contains(slot) {
                        return Err(invalid(s));
                    }
                    if *slot == result_slot {
                        counter_entities.push(self.entity(*b, *slot, s)?);
                    }
                }
            }
        }
        for inst in &f.instructions {
            if inst
                .operation
                .entities()
                .iter()
                .any(|e| counter_entities.contains(e))
                && ![hids[1], uids[1]].contains(&inst.id)
            {
                return Err(invalid(s));
            }
        }
        // Counter values may leave a block only through already checked selected edges.
        let transports = [entry, header, split, arm_a, arm_b, join, success];
        for block in &f.blocks {
            if block.terminator.as_ref().is_some_and(|t| {
                t.kind
                    .entities()
                    .iter()
                    .any(|e| counter_entities.contains(e))
            }) && !transports
                .iter()
                .any(|b| self.blocks.get(b) == Some(&block.id))
            {
                return Err(invalid(s));
            }
        }
        Ok(())
    }
}
