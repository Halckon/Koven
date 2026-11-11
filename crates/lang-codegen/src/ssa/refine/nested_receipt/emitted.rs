//! Consume producer tags plus actual SSA block/slot mappings, including every parent edge.
use super::*;
use crate::ssa::model::{EntityId, Function, Operation, PlaceAccess, TerminatorKind};
#[derive(Clone, Debug)]
struct Layout {
    blocks: BTreeMap<BlockRef, BlockId>,
    parameters: BTreeMap<BlockId, Vec<Slot>>,
    entities: BTreeMap<BlockId, Vec<EntityId>>,
    split_blocks: Vec<BlockId>,
    leaf_blocks: Vec<BlockId>,
    source: LoanId,
    flags: Vec<LoanId>,
    sources: Vec<LoanId>,
}
#[derive(Default)]
pub(in crate::ssa::refine) struct Emitted {
    layout: Option<Layout>,
    reads: Vec<Option<(InstructionId, ValueId)>>,
    calls: Vec<Option<(InstructionId, LoanId)>>,
    edges: Vec<Option<[Edge; 2]>>,
    returned: Vec<bool>,
}
fn loan(value: Option<EntityId>, span: Span) -> Result<LoanId, LoweringError> {
    match value {
        Some(EntityId::Loan(id)) => Ok(id),
        _ => Err(invalid(span)),
    }
}
impl Emitted {
    pub(in crate::ssa::refine) fn new(
        plans: &Plans,
        function: &Function,
        span: Span,
    ) -> Result<Self, LoweringError> {
        plans.finish(span)?;
        if plans
            .plan
            .as_ref()
            .is_some_and(|p| p.caller != function.id())
        {
            return Err(invalid(span));
        }
        let Some(p) = &plans.plan else {
            return Ok(Self::default());
        };
        let (splits, leaves) = p.counts();
        Ok(Self {
            reads: vec![None; splits],
            calls: vec![None; leaves],
            edges: vec![None; splits],
            returned: vec![false; leaves],
            layout: None,
        })
    }
    pub(in crate::ssa::refine) fn layout(
        &mut self,
        plans: &Plans,
        function: &Function,
        blocks: &BTreeMap<BlockRef, BlockId>,
        parameters: &BTreeMap<BlockRef, Vec<Slot>>,
        entities: &BTreeMap<BlockRef, Vec<EntityId>>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &plans.plan else {
            return Ok(());
        };
        if self.layout.is_some()
            || blocks.len() != p.counts().0 + p.counts().1
            || p.splits.len() != p.counts().0
            || p.paths.len() != p.counts().1
            || p.splits
                .iter()
                .any(|s| s.join.is_none_or(|j| blocks.contains_key(&j)))
        {
            return Err(invalid(span));
        }
        let resolve = |b: Option<BlockRef>| {
            b.and_then(|b| blocks.get(&b).copied())
                .ok_or_else(|| invalid(span))
        };
        let mapped = |b: Option<BlockRef>, slot: Option<Slot>| -> Result<LoanId, LoweringError> {
            let b = b.ok_or_else(|| invalid(span))?;
            let slot = slot.ok_or_else(|| invalid(span))?;
            let slots = parameters.get(&b).ok_or_else(|| invalid(span))?;
            let values = entities.get(&b).ok_or_else(|| invalid(span))?;
            if slots.len() != values.len()
                || function.block(resolve(Some(b))?).map(|b| &b.parameters) != Some(values)
            {
                return Err(invalid(span));
            }
            let indices = slots
                .iter()
                .enumerate()
                .filter(|(_, s)| **s == slot)
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            let [index] = indices.as_slice() else {
                return Err(invalid(span));
            };
            loan(values.get(*index).copied(), span)
        };
        let split_blocks = p
            .splits
            .iter()
            .map(|s| resolve(s.entry))
            .collect::<Result<Vec<_>, _>>()?;
        let leaf_blocks = p
            .paths
            .iter()
            .map(|p| resolve(p.block))
            .collect::<Result<Vec<_>, _>>()?;
        let unique = split_blocks
            .iter()
            .chain(&leaf_blocks)
            .collect::<std::collections::BTreeSet<_>>();
        if unique.len() != p.counts().0 + p.counts().1
            || function.blocks.first().map(|b| b.id) != Some(split_blocks[0])
        {
            return Err(invalid(span));
        }
        let source = mapped(p.splits[0].entry, p.source)?;
        let entry_flags = p
            .splits
            .iter()
            .map(|s| mapped(p.splits[0].entry, s.flag))
            .collect::<Result<Vec<_>, _>>()?;
        let flags = p
            .splits
            .iter()
            .map(|s| mapped(s.entry, s.flag))
            .collect::<Result<Vec<_>, _>>()?;
        let sources = p
            .paths
            .iter()
            .map(|p2| mapped(p2.block, p.source))
            .collect::<Result<Vec<_>, _>>()?;
        let expected_entry = std::iter::once(source)
            .chain(entry_flags)
            .map(EntityId::Loan)
            .collect::<Vec<_>>();
        if expected_entry
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != p.counts().0 + 1
            || function.block(split_blocks[0]).map(|b| &b.parameters) != Some(&expected_entry)
        {
            return Err(invalid(span));
        }
        for i in 1..p.splits.len() {
            let inner_source = mapped(p.splits[i].entry, p.source)?;
            if inner_source == flags[i]
                || function.block(split_blocks[i]).is_none_or(|b| {
                    b.parameters.len() != 2
                        || !b.parameters.contains(&EntityId::Loan(flags[i]))
                        || !b.parameters.contains(&EntityId::Loan(inner_source))
                })
            {
                return Err(invalid(span));
            }
        }
        if leaf_blocks.iter().zip(&sources).any(|(block, source)| {
            function.block(*block).map(|b| b.parameters.as_slice())
                != Some(&[EntityId::Loan(*source)][..])
        }) {
            return Err(invalid(span));
        }
        let mut final_parameters = BTreeMap::new();
        let mut final_entities = BTreeMap::new();
        for (b, id) in blocks {
            final_parameters.insert(
                *id,
                parameters.get(b).cloned().ok_or_else(|| invalid(span))?,
            );
            final_entities.insert(*id, entities.get(b).cloned().ok_or_else(|| invalid(span))?);
        }
        self.layout = Some(Layout {
            blocks: blocks.clone(),
            parameters: final_parameters,
            entities: final_entities,
            split_blocks,
            leaf_blocks,
            source,
            flags,
            sources,
        });
        Ok(())
    }
    pub(in crate::ssa::refine) fn statement(
        &mut self,
        plans: &Plans,
        tag: Tag,
        function: &Function,
        instruction: InstructionId,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let p = plans.plan.as_ref().ok_or_else(|| invalid(span))?;
        let l = self.layout.as_ref().ok_or_else(|| invalid(span))?;
        let data = function
            .instruction(instruction)
            .ok_or_else(|| invalid(span))?;
        if function.id() != p.caller {
            return Err(invalid(span));
        }
        match tag {
            Tag::Condition(i) => {
                let s = p.splits.get(i).ok_or_else(|| invalid(span))?;
                let [EntityId::Value(value)] = data.results.as_slice() else {
                    return Err(invalid(span));
                };
                if i >= p.counts().0
                    || self.reads[i].is_some()
                    || data.block != l.split_blocks[i]
                    || data.operation
                        != (Operation::Read {
                            source: PlaceAccess::Loan(l.flags[i]),
                        })
                    || s.value.and_then(|s| current.get(&s)) != Some(&EntityId::Value(*value))
                {
                    return Err(invalid(span));
                }
                self.reads[i] = Some((instruction, *value));
            }
            Tag::Call(i) => {
                let path = p.paths.get(i).ok_or_else(|| invalid(span))?;
                let source = loan(path.source.and_then(|s| current.get(&s).copied()), span)?;
                let result = loan(path.result.and_then(|s| current.get(&s).copied()), span)?;
                if i >= p.counts().1
                    || self.calls[i].is_some()
                    || data.block != l.leaf_blocks[i]
                    || source != l.sources[i]
                    || source == result
                    || data.results != [EntityId::Loan(result)]
                    || data.operation
                        != (Operation::BorrowCall {
                            callee: path.callee,
                            arguments: vec![EntityId::Loan(source)],
                            source,
                        })
                {
                    return Err(invalid(span));
                }
                self.calls[i] = Some((instruction, result));
            }
        }
        Ok(())
    }
    pub(in crate::ssa::refine) fn terminator(
        &mut self,
        plans: &Plans,
        function: &Function,
        block: BlockId,
        current: &BTreeMap<Slot, EntityId>,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &plans.plan else {
            return Ok(());
        };
        let l = self.layout.as_ref().ok_or_else(|| invalid(span))?;
        let term = &function
            .block(block)
            .and_then(|b| b.terminator.as_ref())
            .ok_or_else(|| invalid(span))?
            .kind;
        if let Some(i) = l.split_blocks.iter().position(|b| *b == block) {
            let (_, value) = self.reads[i].ok_or_else(|| invalid(span))?;
            let TerminatorKind::Conditional {
                condition,
                when_true,
                when_false,
            } = term
            else {
                return Err(invalid(span));
            };
            let arms = p.splits[i].arms.ok_or_else(|| invalid(span))?;
            if self.edges[i].is_some() || *condition != value {
                return Err(invalid(span));
            }
            for (edge, target) in [when_true, when_false].into_iter().zip(arms) {
                if l.blocks.get(&target) != Some(&edge.target) {
                    return Err(invalid(span));
                }
                let expected = l
                    .parameters
                    .get(&edge.target)
                    .ok_or_else(|| invalid(span))?
                    .iter()
                    .map(|s| current.get(s).copied().ok_or_else(|| invalid(span)))
                    .collect::<Result<Vec<_>, _>>()?;
                if edge.arguments != expected
                    || function.block(edge.target).map(|b| &b.parameters)
                        != l.entities.get(&edge.target)
                {
                    return Err(invalid(span));
                }
            }
            self.edges[i] = Some([when_true.clone(), when_false.clone()]);
        } else {
            let i = l
                .leaf_blocks
                .iter()
                .position(|b| *b == block)
                .ok_or_else(|| invalid(span))?;
            let (_, result) = self.calls[i].ok_or_else(|| invalid(span))?;
            if self.returned[i]
                || *term != (TerminatorKind::BorrowReturn { loan: result })
                || p.paths[i].result.and_then(|s| current.get(&s)) != Some(&EntityId::Loan(result))
            {
                return Err(invalid(span));
            }
            self.returned[i] = true;
        }
        Ok(())
    }
    pub(in crate::ssa::refine) fn finish(
        self,
        plans: &Plans,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
        let Some(p) = &plans.plan else {
            return if self.layout.is_none()
                && self.reads.iter().all(Option::is_none)
                && self.calls.iter().all(Option::is_none)
                && self.edges.iter().all(Option::is_none)
                && self.returned.is_empty()
            {
                Ok(())
            } else {
                Err(invalid(span))
            };
        };
        let l = self.layout.ok_or_else(|| invalid(span))?;
        if self.returned.len() != p.counts().1
            || self.returned.iter().any(|returned| !returned)
            || function.blocks.len() != p.counts().0 + p.counts().1
            || function.instructions.len() != p.counts().0 + p.counts().1
        {
            return Err(invalid(span));
        }
        let edges = self
            .edges
            .into_iter()
            .map(|edge| edge.ok_or_else(|| invalid(span)))
            .collect::<Result<Vec<_>, _>>()?;
        let mut splits = Vec::new();
        for (i, s) in p.splits.iter().enumerate() {
            let (read, value) = self.reads[i].ok_or_else(|| invalid(span))?;
            splits.push(SplitReceipt {
                split: s.split,
                condition: s.condition,
                block: l.split_blocks[i],
                flag: l.flags[i],
                read,
                value,
                edges: edges[i].clone(),
            });
        }
        let mut paths = Vec::new();
        for (i, path) in p.paths.iter().enumerate() {
            let (instruction, result) = self.calls[i].ok_or_else(|| invalid(span))?;
            paths.push(PathReceipt {
                control: path.control,
                value: path.value,
                ids: path.ids.clone(),
                callee: path.callee,
                block: l.leaf_blocks[i],
                instruction,
                source: l.sources[i],
                result,
                edges: p
                    .leaf_route(i)
                    .into_iter()
                    .map(|(split, arm)| edges[split][arm].clone())
                    .collect(),
            });
        }
        let receipt = Receipt {
            topology: p.topology,
            callable: p.callable,
            caller: p.caller,
            entry: l.split_blocks[0],
            source: l.source,
            splits,
            paths,
        };
        receipt.validate(plans, function, span)?;
        #[cfg(test)]
        ci_tests::record(plans, function, &receipt, span);
        #[cfg(test)]
        RECEIPTS.with(|v| {
            if let Some(v) = v.borrow_mut().as_mut() {
                v.pending.push(receipt);
            }
        });
        Ok(())
    }
}
impl Receipt {
    fn validate(
        &self,
        plans: &Plans,
        function: &Function,
        span: Span,
    ) -> Result<(), LoweringError> {
        plans.finish(span)?;
        let p = plans.plan.as_ref().ok_or_else(|| invalid(span))?;
        if self.topology != p.topology
            || self.callable != p.callable
            || self.caller != p.caller
            || self.caller != function.id()
            || self.splits.len() != p.counts().0
            || self.paths.len() != p.counts().1
            || function.blocks.len() != p.counts().0 + p.counts().1
            || function.instructions.len() != p.counts().0 + p.counts().1
            || function.blocks.first().map(|b| b.id) != Some(self.entry)
            || self.splits[0].block != self.entry
        {
            return Err(invalid(span));
        }
        let mapped = |edge: &Edge, source: LoanId| -> Result<LoanId, LoweringError> {
            let indices = edge
                .arguments
                .iter()
                .enumerate()
                .filter(|(_, e)| **e == EntityId::Loan(source))
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            let [i] = indices.as_slice() else {
                return Err(invalid(span));
            };
            loan(
                function
                    .block(edge.target)
                    .and_then(|b| b.parameters.get(*i))
                    .copied(),
                span,
            )
        };
        let entry = function.block(self.entry).ok_or_else(|| invalid(span))?;
        if entry.parameters.len() != p.counts().0 + 1 {
            return Err(invalid(span));
        }
        let source = loan(entry.parameters.first().copied(), span)?;
        let outer = loan(entry.parameters.get(1).copied(), span)?;
        if self.source != source || self.splits[0].flag != outer {
            return Err(invalid(span));
        }
        for i in 1..self.splits.len() {
            let parent_arm = p.parent_arm(i).ok_or_else(|| invalid(span))?;
            let flag = loan(entry.parameters.get(i + 1).copied(), span)?;
            let edge = &self.splits[0].edges[parent_arm];
            if edge.target != self.splits[i].block || mapped(edge, flag)? != self.splits[i].flag {
                return Err(invalid(span));
            }
        }
        for (i, s) in self.splits.iter().enumerate() {
            let read = function.instruction(s.read).ok_or_else(|| invalid(span))?;
            if s.split != p.splits[i].split
                || s.condition != p.splits[i].condition
                || read.block != s.block
                || read.results != [EntityId::Value(s.value)]
                || read.operation
                    != (Operation::Read {
                        source: PlaceAccess::Loan(s.flag),
                    })
                || function
                    .block(s.block)
                    .and_then(|b| b.terminator.as_ref())
                    .map(|t| &t.kind)
                    != Some(&TerminatorKind::Conditional {
                        condition: s.value,
                        when_true: s.edges[0].clone(),
                        when_false: s.edges[1].clone(),
                    })
            {
                return Err(invalid(span));
            }
        }
        let ids = self
            .splits
            .iter()
            .map(|s| s.block)
            .chain(self.paths.iter().map(|p| p.block))
            .collect::<std::collections::BTreeSet<_>>();
        if ids.len() != p.counts().0 + p.counts().1 {
            return Err(invalid(span));
        }
        for (i, path) in self.paths.iter().enumerate() {
            let expected = p
                .leaf_route(i)
                .into_iter()
                .map(|(split, arm)| self.splits[split].edges[arm].clone())
                .collect::<Vec<_>>();
            let mut source = self.source;
            for edge in &path.edges {
                source = mapped(edge, source)?;
            }
            let call = function
                .instruction(path.instruction)
                .ok_or_else(|| invalid(span))?;
            if path.control != p.paths[i].control
                || path.value != p.paths[i].value
                || path.ids != p.paths[i].ids
                || path.callee != p.paths[i].callee
                || path.edges != expected
                || path.edges.last().map(|e| e.target) != Some(path.block)
                || path.source != source
                || path.result == source
                || call.block != path.block
                || call.results != [EntityId::Loan(path.result)]
                || call.operation
                    != (Operation::BorrowCall {
                        callee: path.callee,
                        arguments: vec![EntityId::Loan(source)],
                        source,
                    })
                || function
                    .block(path.block)
                    .and_then(|b| b.terminator.as_ref())
                    .map(|t| &t.kind)
                    != Some(&TerminatorKind::BorrowReturn { loan: path.result })
            {
                return Err(invalid(span));
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod ci_tests;
