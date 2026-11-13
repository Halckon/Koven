//! Validate the captured instructions and complete CFG independently of builder leases.
use super::*;
use crate::ssa::model::{Edge, Operation, Origin, PlaceAccess, TerminatorKind};
impl Emitted {
    pub(super) fn block(&self, b: BlockRef, s: Span) -> Result<BlockId, LoweringError> {
        self.blocks.get(&b).copied().ok_or_else(|| invalid(s))
    }
    pub(super) fn entity(
        &self,
        b: BlockRef,
        slot: Slot,
        s: Span,
    ) -> Result<EntityId, LoweringError> {
        let i = self
            .parameters
            .get(&b)
            .and_then(|v| v.iter().position(|x| *x == slot))
            .ok_or_else(|| invalid(s))?;
        self.entities
            .get(&b)
            .and_then(|v| v.get(i))
            .copied()
            .ok_or_else(|| invalid(s))
    }
    pub(super) fn edges<'a>(
        &self,
        f: &'a Function,
        b: BlockRef,
        s: Span,
    ) -> Result<Vec<&'a Edge>, LoweringError> {
        let t = &f
            .block(self.block(b, s)?)
            .and_then(|b| b.terminator.as_ref())
            .ok_or_else(|| invalid(s))?
            .kind;
        match t {
            TerminatorKind::Branch(e) => Ok(vec![e]),
            TerminatorKind::Conditional {
                when_true,
                when_false,
                ..
            } => Ok(vec![when_true, when_false]),
            _ => Err(invalid(s)),
        }
    }
    pub(super) fn links(
        &self,
        f: &Function,
        from: BlockRef,
        to: &[BlockRef],
        s: Span,
    ) -> Result<(), LoweringError> {
        let edges = self.edges(f, from, s)?;
        if edges.len() != to.len() {
            return Err(invalid(s));
        }
        for (edge, t) in edges.iter().zip(to) {
            if edge.target != self.block(*t, s)? {
                return Err(invalid(s));
            }
            let slots = self.parameters.get(t).ok_or_else(|| invalid(s))?;
            let current = self
                .ends
                .get(&self.block(from, s)?)
                .ok_or_else(|| invalid(s))?;
            let expected = slots
                .iter()
                .map(|slot| current.get(slot).copied().ok_or_else(|| invalid(s)))
                .collect::<Result<Vec<_>, _>>()?;
            if edge.arguments != expected {
                return Err(invalid(s));
            }
        }
        Ok(())
    }
    pub(super) fn incoming(
        &self,
        f: &Function,
        to: BlockRef,
        from: &[BlockRef],
        s: Span,
    ) -> Result<(), LoweringError> {
        let target = self.block(to, s)?;
        let mut actual = Vec::new();
        for b in &f.blocks {
            let t = b.terminator.as_ref().ok_or_else(|| invalid(s))?;
            for next in t.kind.targets() {
                if next == target {
                    actual.push(b.id);
                }
            }
        }
        let mut expected = from
            .iter()
            .map(|b| self.block(*b, s))
            .collect::<Result<Vec<_>, _>>()?;
        actual.sort();
        expected.sort();
        if actual != expected {
            return Err(invalid(s));
        }
        Ok(())
    }
    pub(super) fn ids(
        &self,
        f: &Function,
        b: BlockRef,
        s: Span,
    ) -> Result<Vec<InstructionId>, LoweringError> {
        let ids = self
            .instructions
            .iter()
            .filter(|((block, _), _)| *block == b)
            .map(|(_, id)| *id)
            .collect::<Vec<_>>();
        if f.block(self.block(b, s)?)
            .ok_or_else(|| invalid(s))?
            .instructions
            != ids
        {
            return Err(invalid(s));
        }
        Ok(ids)
    }
    pub(super) fn condition(
        &self,
        parameters: &[Slot],
        f: &Function,
        p: &Plan,
        i: usize,
        s: Span,
    ) -> Result<(), LoweringError> {
        let (b, slot) = p.used[i].ok_or_else(|| invalid(s))?;
        let param = *parameters.get(p.roles[i + 1]).ok_or_else(|| invalid(s))?;
        let original = self.entity(b, param, s)?;
        let ids = self.ids(f, b, s)?;
        let actual = self
            .ends
            .get(&self.block(b, s)?)
            .and_then(|v| v.get(&slot))
            .copied()
            .ok_or_else(|| invalid(s))?;
        match original {
            EntityId::Loan(loan) => {
                if ids.len() != 1 {
                    return Err(invalid(s));
                }
                let inst = f.instruction(ids[0]).ok_or_else(|| invalid(s))?;
                if inst.block != self.block(b, s)?
                    || inst.origin != Origin::Source(p.condition_spans[i])
                {
                    return Err(invalid(s));
                }
                if inst.operation
                    != (Operation::Read {
                        source: PlaceAccess::Loan(loan),
                    })
                    || inst.results != [actual]
                {
                    return Err(invalid(s));
                }
            }
            EntityId::Value(_) => {
                if !ids.is_empty() || actual != original {
                    return Err(invalid(s));
                }
            }
            _ => return Err(invalid(s)),
        }
        let term = &f
            .block(self.block(b, s)?)
            .and_then(|b| b.terminator.as_ref())
            .ok_or_else(|| invalid(s))?
            .kind;
        if !matches!(term,TerminatorKind::Conditional{condition,..} if EntityId::Value(*condition)==actual)
        {
            return Err(invalid(s));
        }
        Ok(())
    }
    pub(super) fn validate(
        &self,
        parameters: &[Slot],
        f: &Function,
        p: &Plan,
        s: Span,
    ) -> Result<(), LoweringError> {
        for (block, id) in &self.blocks {
            let slots = self.parameters.get(block).ok_or_else(|| invalid(s))?;
            let entities = self.entities.get(block).ok_or_else(|| invalid(s))?;
            if slots.len() != entities.len()
                || slots
                    .iter()
                    .enumerate()
                    .any(|(i, x)| slots[..i].contains(x))
                || entities
                    .iter()
                    .enumerate()
                    .any(|(i, x)| entities[..i].contains(x))
                || f.block(*id).ok_or_else(|| invalid(s))?.parameters != *entities
            {
                return Err(invalid(s));
            }
        }
        let [entry, header, split, finished, exit] = p.blocks.ok_or_else(|| invalid(s))?;
        let [actual_split, then_block, else_block] = p.split.ok_or_else(|| invalid(s))?;
        let join = p.join.ok_or_else(|| invalid(s))?;
        let source = p.source.ok_or_else(|| invalid(s))?;
        if split != actual_split || source != parameters[p.roles[0]] || then_block == else_block {
            return Err(invalid(s));
        }
        if p.counter.is_some() {
            self.validate_counter(parameters, f, p, s)?;
        } else {
            self.links(f, entry, &[header], s)?;
            self.links(f, header, &[split, finished], s)?;
            self.links(f, split, &[then_block, else_block], s)?;
            self.links(f, then_block, &[join], s)?;
            self.links(f, else_block, &[join], s)?;
            self.links(f, join, &[header], s)?;
            self.incoming(f, header, &[entry, join], s)?;
            self.incoming(f, split, &[header], s)?;
            self.incoming(f, then_block, &[split], s)?;
            self.incoming(f, else_block, &[split], s)?;
            self.incoming(f, join, &[then_block, else_block], s)?;
            self.incoming(f, finished, &[header], s)?;
            if finished != exit {
                self.links(f, finished, &[exit], s)?;
                self.incoming(f, exit, &[finished], s)?;
            }
            // Local arm results must never enter another block's parameters, including loop reentry.
            for b in [header, split, then_block, else_block, join, finished, exit] {
                if self
                    .parameters
                    .get(&b)
                    .ok_or_else(|| invalid(s))?
                    .iter()
                    .any(|slot| !parameters.contains(slot))
                {
                    return Err(invalid(s));
                }
            }
            self.condition(parameters, f, p, 0, s)?;
            self.condition(parameters, f, p, 1, s)?;
            if !self.ids(f, join, s)?.is_empty() || !self.ids(f, finished, s)?.is_empty() {
                return Err(invalid(s));
            }
        }
        let mut results = Vec::new();
        for (index, b) in [then_block, else_block].into_iter().enumerate() {
            let arm = &p.arms[index];
            let ids = self.ids(f, b, s)?;
            if ids.len() != arm.consumers.len() + 2 || ids.len() != arm.spans.len() {
                return Err(invalid(s));
            }
            let parent = self.entity(b, source, s)?;
            let EntityId::Loan(parent_loan) = parent else {
                return Err(invalid(s));
            };
            let first = f.instruction(ids[0]).ok_or_else(|| invalid(s))?;
            if first.operation
                != (Operation::BorrowCall {
                    callee: arm.callee,
                    arguments: vec![parent],
                    source: parent_loan,
                })
                || first.results.len() != 1
            {
                return Err(invalid(s));
            }
            let fresh = first.results[0];
            let EntityId::Loan(fresh_loan) = fresh else {
                return Err(invalid(s));
            };
            if fresh == parent || results.contains(&fresh) {
                return Err(invalid(s));
            }
            results.push(fresh);
            for (position, id) in ids.iter().enumerate() {
                let inst = f.instruction(*id).ok_or_else(|| invalid(s))?;
                if inst.block != self.block(b, s)?
                    || inst.origin != Origin::Source(arm.spans[position])
                {
                    return Err(invalid(s));
                }
                if position > 0
                    && position <= arm.consumers.len()
                    && (inst.operation
                        != (Operation::DirectCall {
                            callee: arm.consumers[position - 1],
                            receiver: None,
                            arguments: vec![fresh],
                        })
                        || !inst.results.is_empty())
                {
                    return Err(invalid(s));
                }
                if position == ids.len() - 1
                    && (inst.operation != (Operation::BorrowEnd { loan: fresh_loan })
                        || !inst.results.is_empty())
                {
                    return Err(invalid(s));
                }
            }
            // The only uses/definition of this fresh result are this arm's validated chain.
            for inst in &f.instructions {
                if (inst.operation.entities().contains(&fresh) || inst.results.contains(&fresh))
                    && !ids.contains(&inst.id)
                {
                    return Err(invalid(s));
                }
            }
            for block in &f.blocks {
                if block.parameters.contains(&fresh)
                    || block
                        .terminator
                        .as_ref()
                        .is_some_and(|t| t.kind.entities().contains(&fresh))
                {
                    return Err(invalid(s));
                }
            }
        }
        Ok(())
    }
}
