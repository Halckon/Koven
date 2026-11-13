//! Read-only late SSA evidence for already emitted nested false cleanup.
//! No owner token, Drop emission, or parent-resume permission is created here.
use super::body::{BlockRef, Slot};
use crate::ssa::{
    lowering_support::{LoweringError, LoweringErrorKind, error},
    model::{BlockId, EntityId, Function, FunctionId, InstructionId, Operation, TerminatorKind},
};
use lang_frontend::{source::Span, type_checking::UnitStatementId};
use std::collections::BTreeMap;
mod pair;
pub(super) use pair::Emitted;
fn invalid(span: Span) -> LoweringError {
    error(LoweringErrorKind::InvalidModel, span)
}
#[derive(Clone, Debug)]
pub(super) struct DropSite {
    pub root: Slot,
    pub instruction: usize,
    pub span: Span,
}
#[derive(Clone, Debug)]
pub(super) struct Plan {
    pub control: UnitStatementId,
    pub header: BlockRef,
    pub exit: BlockRef,
    pub drops: Vec<DropSite>,
    pub roots: Vec<Slot>,
    pub live: Vec<Slot>,
    pub parent: bool,
}
#[derive(Clone, Debug, Default)]
pub(super) struct Plans {
    pub child: Option<Plan>,
    pub parent: Option<Plan>,
    pub parent_required: bool,
}
impl Plans {
    pub(super) fn fold(
        &mut self,
        block: BlockRef,
        successor: BlockRef,
        prefix: usize,
        span: Span,
    ) -> Result<(), LoweringError> {
        for p in [&mut self.child, &mut self.parent].into_iter().flatten() {
            if p.header == successor {
                p.header = block;
            }
            if p.exit == successor {
                p.exit = block;
                for d in &mut p.drops {
                    d.instruction = d
                        .instruction
                        .checked_add(prefix)
                        .ok_or_else(|| invalid(span))?;
                }
            }
            if p.header == p.exit {
                return Err(invalid(span));
            }
        }
        Ok(())
    }
}
#[derive(Default)]
struct Receipt {
    function: Option<FunctionId>,
    control: Option<UnitStatementId>,
    blocks: BTreeMap<BlockRef, BlockId>,
    slots: BTreeMap<BlockRef, Vec<Slot>>,
    entities: BTreeMap<BlockRef, Vec<EntityId>>,
    observed: BTreeMap<usize, (InstructionId, EntityId)>,
}
impl Receipt {
    pub(super) fn layout(
        &mut self,
        plan: &Option<Plan>,
        function: &Function,
        blocks: &BTreeMap<BlockRef, BlockId>,
        slots: &BTreeMap<BlockRef, Vec<Slot>>,
        entities: &BTreeMap<BlockRef, Vec<EntityId>>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = plan else {
            return Ok(());
        };
        if self.function.is_some() || p.header == p.exit {
            return Err(invalid(span));
        }
        if p.roots.is_empty()
            || p.roots
                .iter()
                .enumerate()
                .any(|(i, r)| p.roots[..i].contains(r))
            || p.drops.iter().any(|d| !p.roots.contains(&d.root))
            || p.live.iter().any(|r| !p.roots.contains(r))
            || p.live.len() + p.drops.len() != p.roots.len()
            || p.roots.iter().any(|r| {
                p.live.iter().filter(|v| *v == r).count()
                    + p.drops.iter().filter(|d| d.root == *r).count()
                    != 1
            })
        {
            return Err(invalid(span));
        }
        for b in [p.header, p.exit] {
            let id = blocks.get(&b).ok_or_else(|| invalid(span))?;
            let s = slots.get(&b).ok_or_else(|| invalid(span))?;
            let e = entities.get(&b).ok_or_else(|| invalid(span))?;
            if s.len() != e.len() || function.block(*id).map(|b| &b.parameters) != Some(e) {
                return Err(invalid(span));
            }
            for root in &p.roots {
                if s.iter().filter(|r| **r == *root).count() != 1 {
                    return Err(invalid(span));
                }
            }
        }
        self.function = Some(function.id());
        self.control = Some(p.control);
        self.blocks = blocks.clone();
        self.slots = slots.clone();
        self.entities = entities.clone();
        Ok(())
    }
    pub(super) fn statement(
        &mut self,
        plan: &Option<Plan>,
        function: &Function,
        position: (BlockRef, usize),
        instruction: InstructionId,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = plan else {
            return Ok(());
        };
        if position.0 != p.exit {
            return Ok(());
        }
        for (i, d) in p
            .drops
            .iter()
            .enumerate()
            .filter(|(_, d)| d.instruction == position.1)
        {
            let actual = function
                .instruction(instruction)
                .ok_or_else(|| invalid(span))?;
            let value = *current.get(&d.root).ok_or_else(|| invalid(span))?;
            let index = self.slots[&p.exit]
                .iter()
                .position(|s| *s == d.root)
                .ok_or_else(|| invalid(span))?;
            if self.control != Some(p.control)
                || self.function != Some(function.id())
                || actual.block != self.blocks[&p.exit]
                || actual.origin.span() != d.span
                || !matches!(actual.operation,Operation::Drop {owner} if EntityId::Value(owner)==value)
                || self.entities[&p.exit][index] != value
                || self.observed.insert(i, (instruction, value)).is_some()
            {
                return Err(invalid(span));
            }
        }
        Ok(())
    }
    pub(super) fn finish(
        &self,
        plan: &Option<Plan>,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = plan else {
            return Ok(());
        };
        if self.control != Some(p.control)
            || self.function != Some(function.id())
            || self.observed.len() != p.drops.len()
        {
            return Err(invalid(span));
        }
        let header = function
            .block(*self.blocks.get(&p.header).ok_or_else(|| invalid(span))?)
            .ok_or_else(|| invalid(span))?;
        let Some(term) = &header.terminator else {
            return Err(invalid(span));
        };
        let TerminatorKind::Conditional { when_false, .. } = &term.kind else {
            return Err(invalid(span));
        };
        if when_false.target != self.blocks[&p.exit]
            || when_false.arguments.len() != self.slots[&p.exit].len()
        {
            return Err(invalid(span));
        }
        let exit = function
            .block(self.blocks[&p.exit])
            .ok_or_else(|| invalid(span))?;
        // The approved parent false exit has exactly the header's false edge.
        // Another predecessor could otherwise feed unrelated same-typed owners.
        if p.parent {
            for block in &function.blocks {
                let term = block.terminator.as_ref().ok_or_else(|| invalid(span))?;
                let incoming = term
                    .kind
                    .targets()
                    .into_iter()
                    .filter(|target| *target == exit.id)
                    .count();
                if incoming != usize::from(block.id == header.id) {
                    return Err(invalid(span));
                }
            }
        }
        if header.parameters != self.entities[&p.header]
            || exit.parameters != self.entities[&p.exit]
        {
            return Err(invalid(span));
        }
        for root in &p.roots {
            let h = self.slots[&p.header]
                .iter()
                .position(|s| *s == *root)
                .ok_or_else(|| invalid(span))?;
            let e = self.slots[&p.exit]
                .iter()
                .position(|s| *s == *root)
                .ok_or_else(|| invalid(span))?;
            if when_false.arguments[e] != self.entities[&p.header][h] {
                return Err(invalid(span));
            }
        }
        let mut previous = None;
        for (i, d) in p.drops.iter().enumerate() {
            let (id, value) = self.observed.get(&i).ok_or_else(|| invalid(span))?;
            let actual = function.instruction(*id).ok_or_else(|| invalid(span))?;
            let positions = exit
                .instructions
                .iter()
                .enumerate()
                .filter_map(|(i, v)| (*v == *id).then_some(i))
                .collect::<Vec<_>>();
            let [position] = positions.as_slice() else {
                return Err(invalid(span));
            };
            if previous.is_some_and(|p| p >= *position) {
                return Err(invalid(span));
            }
            previous = Some(*position);

            if actual.block != when_false.target
                || actual.origin.span() != d.span
                || !matches!(actual.operation,Operation::Drop {owner} if EntityId::Value(owner)==*value)
            {
                return Err(invalid(span));
            }
        }
        // Only retired exit-parameter Values are constrained; later incarnations of the
        // same source symbol and all live roots retain their existing behavior.
        for (id, value) in self.observed.values() {
            for i in &function.instructions {
                if i.results.contains(value)
                    || i.operation
                        .entities()
                        .iter()
                        .filter(|e| *e == value)
                        .count()
                        != usize::from(i.id == *id)
                {
                    return Err(invalid(span));
                }
            }
            for b in &function.blocks {
                let definitions = b.parameters.iter().filter(|e| *e == value).count();
                if definitions != usize::from(b.id == exit.id)
                    || b.terminator
                        .as_ref()
                        .is_some_and(|t| t.kind.entities().contains(value))
                {
                    return Err(invalid(span));
                }
            }
        }
        #[cfg(test)]
        super::build::owner_consumer_test_support::record_action(
            if p.parent {
                "nested-parent-false-root-vector-complete"
            } else {
                "nested-false-root-vector-complete"
            },
            p.roots.len(),
        );
        #[cfg(test)]
        super::build::owner_consumer_test_support::record_action(
            if p.parent {
                "nested-parent-false-ssa-complete"
            } else {
                "nested-false-ssa-complete"
            },
            p.drops.len(),
        );
        Ok(())
    }
}

#[cfg(test)]
pub(super) fn inject_final(plans: &Plans, function: &mut Function) {
    use super::build::owner_consumer_test_support::{OwnerFault, record_action, take_fault};
    let Some(p) = &plans.child else {
        return;
    };
    let Some(d) = p.drops.first() else {
        return;
    };
    let Some((id, block)) = function
        .instructions
        .iter()
        .find(|i| i.origin.span() == d.span && matches!(i.operation, Operation::Drop { .. }))
        .map(|i| (i.id, i.block))
    else {
        return;
    };
    if take_fault(OwnerFault::NestedFalseSsaSwappedFalseArguments) {
        let value = match function.instruction(id).unwrap().operation {
            Operation::Drop { owner } => EntityId::Value(owner),
            _ => unreachable!(),
        };
        let position = function
            .block(block)
            .unwrap()
            .parameters
            .iter()
            .position(|v| *v == value)
            .unwrap();
        for b in &mut function.blocks {
            if let Some(term) = &mut b.terminator
                && let TerminatorKind::Conditional { when_false, .. } = &mut term.kind
                && when_false.target == block
                && when_false.arguments.len() > 1
            {
                let other = usize::from(position == 0);
                when_false.arguments.swap(position, other);
                record_action("nested-false-ssa-fault-hit", 1);
                break;
            }
        }
    }
    if take_fault(OwnerFault::NestedFalseSsaMissingBlockDrop)
        && let Some(b) = function.blocks.iter_mut().find(|b| b.id == block)
    {
        b.instructions.retain(|i| *i != id);
        record_action("nested-false-ssa-fault-hit", 1);
    }
    if take_fault(OwnerFault::NestedFalseSsaDuplicateBlockDrop)
        && let Some(b) = function.blocks.iter_mut().find(|b| b.id == block)
    {
        b.instructions.push(id);
        record_action("nested-false-ssa-fault-hit", 1);
    }
    let Operation::Drop { owner } = function.instruction(id).unwrap().operation else {
        return;
    };
    for (fault, operation, before) in [
        (
            OwnerFault::NestedFalseSsaExtraConsume,
            Operation::Consume { owner },
            true,
        ),
        (
            OwnerFault::NestedFalseSsaSecondDrop,
            Operation::Drop { owner },
            false,
        ),
    ] {
        if take_fault(fault) {
            let saved = function
                .blocks
                .iter_mut()
                .find(|b| b.id == block)
                .unwrap()
                .terminator
                .take();
            let origin = function.instruction(id).unwrap().origin.clone();
            let (extra, _) = function
                .append_instruction(block, operation, Vec::new(), origin)
                .unwrap();
            let b = function.blocks.iter_mut().find(|b| b.id == block).unwrap();
            b.terminator = saved;
            if before {
                b.instructions.retain(|i| *i != extra);
                let index = b.instructions.iter().position(|i| *i == id).unwrap();
                b.instructions.insert(index, extra);
            }
            record_action("nested-false-ssa-fault-hit", 1);
        }
    }
    if take_fault(OwnerFault::NestedFalseSsaTerminatorEscape)
        && let Some(term) = function
            .blocks
            .iter_mut()
            .find(|b| b.id == block)
            .and_then(|b| b.terminator.as_mut())
    {
        term.kind = TerminatorKind::Return {
            values: vec![owner],
        };
        record_action("nested-false-ssa-fault-hit", 1);
    }
}
